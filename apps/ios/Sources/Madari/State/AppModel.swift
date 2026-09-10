import Foundation
import SwiftUI

/// Orchestrates every screen, mirroring the Android client's `TvViewModel`.
///
/// Source validation, resume policy, episode ordering and authorization all stay in
/// the Rust core. This type only sequences calls and shapes the results for the UI.
///
/// `ObservableObject` rather than the newer `@Observable`: the observation macro's
/// compiler plugin does not build in this cross-compile setup (xtool #197).
@MainActor
final class AppModel: ObservableObject {
    /// The value every screen renders. Screens change it through the methods below,
    /// except for the player's full-screen dismiss binding, which clears `playback`.
    @Published var state = AppState()
    /// One navigation stack per tab, so each tab remembers where it was.
    ///
    /// A `NavigationPath` rather than `[Route]`: a typed array can only hold one value
    /// type, so a `NavigationLink(value:)` pushing anything else — the settings
    /// sub-pages — was silently rejected and merely highlighted. A path is heterogeneous
    /// on purpose.
    @Published private var paths: [Tab: NavigationPath] = [:]
    @Published var tab: Tab = .home

    private let core = NativeCore.shared
    private let defaults = UserDefaults.standard
    private var traktTask: Task<Void, Never>?
    private var webTask: Task<Void, Never>?

    // MARK: - Paths

    func path(_ tab: Tab) -> Binding<NavigationPath> {
        Binding(
            get: { self.paths[tab] ?? NavigationPath() },
            set: { self.paths[tab] = $0 }
        )
    }

    func push(_ route: Route, on tab: Tab? = nil) {
        let target = tab ?? self.tab
        paths[target, default: NavigationPath()].append(route)
    }

    // MARK: - Launch

    func start() async {
        DebugLog.write("start")
        do {
            let support = try FileManager.default
                .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                .appendingPathComponent("madari", isDirectory: true)
            try await core.initialize(storagePath: support.path)
            DebugLog.write("core initialized")
        } catch {
            DebugLog.write("initialize failed: \(error.localizedDescription)")
            state.error = error.localizedDescription
            return
        }
        await loadProfiles()
        await startWeb()
    }

    // MARK: - Profiles

    private func loadProfiles() async {
        do {
            let result = try await core.call("profiles")
            state.profiles = result.objects("profiles")
            state.profileAvatars = result.objects("avatars")
            state.activeKids = result["active_kids"]?.text("id") ?? ""
            DebugLog.write("profiles=\(state.profiles.count) avatars=\(state.profileAvatars.count) activeKids=\(state.activeKids.isEmpty ? "none" : state.activeKids)")
        } catch {
            DebugLog.write("profiles failed: \(error.localizedDescription)")
            state.error = error.localizedDescription
        }
    }

    func createProfile(name: String, pin: String, kids: Bool, avatar: String) async {
        await perform {
            _ = try await self.core.call("create_profile", [
                "name": .string(name), "pin": .string(pin),
                "kids": .bool(kids), "avatar": .string(avatar),
            ])
            await self.loadProfiles()
        }
    }

    /// Creates a profile from the picker.
    ///
    /// The core requires an authorized regular profile once any profile exists, so
    /// this unlocks the chosen adult, authorizes it, creates the profile, and always
    /// closes the temporary session afterwards.
    func addProfile(
        adult: JSONValue,
        adultPin: String,
        name: String,
        pin: String,
        kids: Bool,
        avatar: String
    ) async {
        await perform {
            defer { Task { try? await self.core.call("leave", ["pin": .string(adultPin)]) } }
            _ = try await self.core.call("unlock", ["id": .string(adult.text("id")), "pin": .string(adultPin)])
            _ = try await self.core.call("authorize", ["pin": .string(adultPin)])
            _ = try await self.core.call("create_profile", [
                "name": .string(name), "pin": .string(pin),
                "kids": .bool(kids), "avatar": .string(avatar),
            ])
            await self.loadProfiles()
        }
    }

    /// Renames a profile or replaces its PIN, then closes the temporary session.
    func updateProfile(
        profile: JSONValue,
        currentPin: String,
        name: String,
        pin: String,
        avatar: String
    ) async {
        await perform {
            defer { Task { try? await self.core.call("leave", ["pin": .string(currentPin)]) } }
            _ = try await self.core.call("unlock", ["id": .string(profile.text("id")), "pin": .string(currentPin)])
            _ = try await self.core.call("authorize", ["pin": .string(currentPin)])
            _ = try await self.core.call("update_profile", [
                "name": .string(name), "pin": .string(pin), "avatar": .string(avatar),
            ])
            await self.loadProfiles()
        }
    }

    /// Deletes a profile and everything stored against it.
    ///
    /// Destructive and irreversible, so it opens the profile, authorizes it with the
    /// PIN the caller supplied, and deletes only then. The core refuses a profile that
    /// still guards a kids profile, and reports the names in its message.
    func deleteProfile(_ profile: JSONValue, pin: String) async {
        DebugLog.write("delete profile \(profile.text("name"))")
        await perform {
            defer {
                // Never leave the picker holding another profile's session.
                Task { try? await self.core.call("leave", ["pin": .string(pin)]) }
            }
            _ = try await self.core.call("unlock", ["id": .string(profile.text("id")), "pin": .string(pin)])
            _ = try await self.core.call("authorize", ["pin": .string(pin)])
            _ = try await self.core.call("delete_profile", ["pin": .string(pin)])
            await self.loadProfiles()
        }
    }

    func unlock(_ profile: JSONValue, pin: String) async {
        DebugLog.write("unlock \(profile.text("name")) pin=\(pin.isEmpty ? "empty" : "set")")
        await perform {
            let selected = try await self.core.call("unlock", [
                "id": .string(profile.text("id")), "pin": .string(pin),
            ])
            self.state.detail = nil
            self.state.sources = nil
            self.state.playback = nil
            self.state.profile = selected
            self.paths = [:]
            await self.refreshSnapshot()
            await self.installDefaultAddonsIfNeeded(pin: pin)
            await self.loadHome()
        }
    }

    /// Offers the curated addons to this profile.
    ///
    /// The decision is the core's, not this client's: it remembers per profile whether
    /// the defaults have been offered, installs whichever are missing, and never
    /// re-adds one the user removed. A guard here on "the profile has no addons" would
    /// skip that logic entirely, which is exactly how a profile that fetched one
    /// default and not the other stayed permanently incomplete.
    ///
    /// Installing is a settings change, so the session is authorized first — unlocking
    /// a profile is not enough on its own. A profile with no PIN authorizes with an
    /// empty PIN. A kids profile needs its guardian's PIN, which is why this can
    /// legitimately fail there: the guardian installs them from Settings instead.
    private func installDefaultAddonsIfNeeded(pin: String) async {
        do {
            _ = try await core.call("authorize", ["pin": .string(pin)])
            let outcome = try await core.call("install_defaults")
            await refreshSnapshot()
            let failures = outcome.objects("failed")
            for failure in failures {
                DebugLog.write("default addon failed: \(failure.text("url")) reason=\(failure.text("reason"))")
            }
            if !failures.isEmpty {
                state.notices.append("Some default addons could not be installed. Add them from Settings.")
            }
        } catch {
            // A profile is still usable without them, so this never blocks entry.
            DebugLog.write("default addons unavailable: \(error.localizedDescription)")
            state.notices.append("Default addons could not be installed. Add them from Settings.")
        }
    }

    /// Leaves the active profile. Kids profiles require the guardian's PIN, which
    /// the core enforces.
    func leave(pin: String) async {
        await perform {
            _ = try await self.core.call("leave", ["pin": .string(pin)])
            let web = self.state.web
            let address = self.state.webAddress
            self.state = AppState()
            self.state.web = web
            self.state.webAddress = address
            self.paths = [:]
            await self.loadProfiles()
        }
    }

    func authorize(pin: String) async {
        await perform {
            _ = try await self.core.call("authorize", ["pin": .string(pin)])
            self.state.settingsUnlocked = true
            await self.loadProfiles()
        }
    }

    // MARK: - Catalogs

    /// Every catalog declared by an enabled addon.
    func catalogs() -> [Catalog] {
        state.addons.flatMap { addon -> [Catalog] in
            let manifest = addon["manifest"] ?? .object([:])
            return manifest.objects("catalogs").map {
                Catalog(
                    provider: addon.text("installation_id"),
                    providerName: manifest.text("name"),
                    raw: $0
                )
            }
        }
    }

    private func fetch(_ catalog: Catalog, extras: [String: String] = [:], skip: Int = 0) async throws -> Shelf {        var fields = extras
        if catalog.pageable, skip > 0 { fields["skip"] = String(skip) }
        let result = try await core.call("query", [
            "installation_id": .string(catalog.provider),
            "request": [
                "resource": .string("catalog"),
                "type": .string(catalog.type),
                "id": .string(catalog.catalogId),
                "extra": .object(fields.mapValues { JSONValue.string($0) }),
            ],
        ])
        let titles = result.objects("data")
            .map { Title(provider: catalog.provider, raw: $0) }
            .reduce(into: [Title]()) { unique, title in
                if !unique.contains(where: { $0.identity == title.identity }) { unique.append(title) }
            }
        return Shelf(
            id: catalog.identity,
            name: catalog.name,
            titles: titles,
            catalog: catalog,
            skip: skip,
            // A full page means the addon may have more; short pages end paging.
            more: catalog.pageable && titles.count >= 100,
            extras: extras
        )
    }

    /// Runs one catalog query. Exposed for the Explore screen, which owns its own
    /// paging so it does not disturb the Home and Search shelf lists.
    func query(_ catalog: Catalog, extras: [String: String] = [:], skip: Int = 0) async throws -> Shelf {
        try await fetch(catalog, extras: extras, skip: skip)
    }

    /// Loads Home: every enabled catalog that needs no user input, three at a time.
    ///
    /// The TV client refreshed the list as each response arrived. iOS would re-lay
    /// out the whole page for every arrival, so rows are collected and published
    /// once per completion instead.
    func loadHome() async {
        let declared = catalogs().filter { $0.required.isEmpty }
        guard !declared.isEmpty else {
            state.homeShelves = []
            return
        }
        await withTaskGroup(of: (Int, Result<Shelf, Error>).self) { group in
            var next = 0
            let limit = min(3, declared.count)
            while next < limit {
                let index = next
                group.addTask { [self] in (index, await fetchOutcome(declared[index])) }
                next += 1
            }
            while let (index, outcome) = await group.next() {
                switch outcome {
                case .success(let shelf):
                    var shelves = state.homeShelves.filter { $0.id != shelf.id }
                    shelves.append(shelf)
                    // Keep addon order stable regardless of response order.
                    shelves.sort { left, right in
                        (declared.firstIndex { $0.identity == left.id } ?? 0)
                            < (declared.firstIndex { $0.identity == right.id } ?? 0)
                    }
                    state.homeShelves = shelves
                case .failure:
                    let catalog = declared[index]
                    let notice = "\(catalog.providerName) · \(catalog.name) could not load"
                    if !state.notices.contains(notice) { state.notices.append(notice) }
                }
                if next < declared.count {
                    let index = next
                    group.addTask { [self] in (index, await fetchOutcome(declared[index])) }
                    next += 1
                }
            }
        }
    }

    private func fetchOutcome(_ catalog: Catalog) async -> Result<Shelf, Error> {
        do { return .success(try await fetch(catalog)) } catch { return .failure(error) }
    }

    func search(_ query: String) async {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        state.query = trimmed
        state.searchShelves = []
        state.notices = []
        guard !trimmed.isEmpty else { return }
        await perform {
            for catalog in self.catalogs().filter({ $0.searchable && $0.required.allSatisfy { $0 == "search" } }) {
                do {
                    let shelf = try await self.fetch(catalog, extras: ["search": trimmed])
                    self.state.searchShelves.append(shelf)
                } catch {
                    self.state.notices.append("\(catalog.providerName) search failed")
                }
            }
        }
    }

    func loadMore(_ shelf: Shelf, into list: ShelfList) async {
        guard let catalog = shelf.catalog else { return }
        await perform {
            do {
                let next = try await self.fetch(catalog, extras: shelf.extras, skip: shelf.skip + 100)
                let merged = (shelf.titles + next.titles).reduce(into: [Title]()) { unique, title in
                    if !unique.contains(where: { $0.identity == title.identity }) { unique.append(title) }
                }
                let updated = Shelf(
                    id: shelf.id, name: shelf.name, titles: merged, catalog: catalog,
                    skip: next.skip,
                    more: next.more && merged.count > shelf.titles.count,
                    extras: shelf.extras
                )
                switch list {
                case .home:
                    self.state.homeShelves = self.state.homeShelves.map { $0.id == shelf.id ? updated : $0 }
                case .search:
                    self.state.searchShelves = self.state.searchShelves.map { $0.id == shelf.id ? updated : $0 }
                }
            } catch {
                self.state.notices.append("\(shelf.name) could not load more titles")
            }
        }
    }

    func refresh() async {
        await perform {
            await self.refreshSnapshot()
            await self.loadHome()
        }
    }

    // MARK: - Title details

    /// Resolves full metadata and the video the core wants to play next.
    func open(_ title: Title) async {
        await perform {
            self.state.detail = title
            self.state.videoId = nil
            self.state.sources = nil
            let metadata = try await self.core.call("metadata", ["key": title.key, "preview": title.raw])
            let episode = try await self.core.call("episode", [
                "meta": metadata, "key": title.key, "today": .string(Self.today),
            ])
            self.state.detail = Title(provider: title.provider, raw: metadata)
            self.state.videoId = episode["video"]?.text("id")
        }
    }

    /// Resolves Continue watching metadata in one cached batch, then asks the core
    /// which video each series should resume.
    func resolveContinue(_ titles: [Title]) async {
        guard !titles.isEmpty else { return }
        let keys = JSONValue.array(titles.map(\.key))
        var metas: [String: JSONValue] = [:]
        if let resolved = try? await core.call("continue_metadata", keys) {
            for pair in resolved.elements {
                guard let key = pair.elements.first, let meta = pair.elements.dropFirst().first?["meta"] else { continue }
                metas["\(key.text("installation_id"))|\(key.text("content_type"))|\(key.text("item_id"))"] = meta
            }
        }
        var entries: [String: ContinueEntry] = [:]
        for title in titles {
            let meta = metas[title.identity] ?? title.raw
            let hasVideos = !meta.objects("videos").isEmpty
            var episode: JSONValue?
            if title.isSeries, hasVideos {
                episode = try? await core.call("episode", [
                    "meta": meta, "key": title.key, "today": .string(Self.today),
                ])["video"]
            }
            entries[title.identity] = ContinueEntry(title: title, meta: meta, hasVideos: hasVideos, episode: episode)
        }
        state.continueEntries = entries
    }

    /// Resumes or advances a Continue watching card.
    ///
    /// Automatic source reuse follows the saved `bingeGroup`, exactly as on the TV;
    /// if none of the preferred sources open, the picker is shown instead.
    func resumeContinue(_ title: Title, knownVideoId: String? = nil) async {
        state.resumingTitle = title.identity
        defer { state.resumingTitle = nil }
        await perform {
            let metadata = try await self.core.call("metadata", ["key": title.key, "preview": title.raw])
            let full = Title(provider: title.provider, raw: metadata)
            let history = self.state.snapshot.objects("progress").filter { sameKey($0["key"], title.key) }

            var episode = knownVideoId
            if episode == nil, full.isSeries {
                episode = try? await self.core.call("episode", [
                    "meta": metadata, "key": title.key, "today": .string(Self.today),
                ])["video"]?.text("id")
            }
            if full.isSeries, !full.videos.isEmpty, episode == nil {
                // Nothing left to continue: show the title instead of failing.
                self.state.detail = full
                self.state.videoId = nil
                self.state.sources = nil
                self.push(.details(full))
                return
            }
            let video = episode
                ?? history.last?.text("video_id").nilIfEmpty
                ?? metadata["behaviorHints"]?["defaultVideoId"]?.stringValue.nilIfEmpty
                ?? full.contentId

            self.state.detail = full
            self.state.videoId = video
            let sources = await self.fetchSources(full, videoId: video)
            let saved = history.last { $0.text("video_id") == video } ?? history.last
            let group = saved?.text("binge_group") ?? ""
            if !group.isEmpty {
                let matches = sources
                    .filter { $0.bingeGroup == group }
                    .sorted { left, _ in left.provider != saved?.text("source_provider") }
                for source in matches.prefix(3) {
                    if await self.preparePlayback(full, videoId: video, source: source) { return }
                }
            }
            self.state.sources = sources
            self.push(.sources(full, videoId: video))
        }
    }

    var isDetailSaved: Bool {
        guard let title = state.detail else { return false }
        return isSaved(title)
    }

    func isSaved(_ title: Title) -> Bool {
        state.snapshot.objects("library").contains { sameKey($0["key"], title.key) }
    }

    func toggleSaved(_ title: Title) async {
        await perform {
            if self.isSaved(title) {
                _ = try await self.core.call("remove", title.key)
            } else {
                _ = try await self.core.call("save", [
                    "key": title.key, "title": .string(title.name), "metadata": title.raw,
                ])
            }
            await self.refreshSnapshot()
        }
    }

    func hideContinue(_ title: Title) async {
        await perform {
            _ = try await self.core.call("hide_continue", ["key": title.key, "hidden": true])
            await self.refreshSnapshot()
        }
    }

    // MARK: - Sources and playback

    private func fetchSources(_ title: Title, videoId: String) async -> [Source] {
        let names = Dictionary(
            uniqueKeysWithValues: state.addons.map {
                ($0.text("installation_id"), $0["manifest"]?.text("name") ?? "Addon")
            }
        )
        guard let result = try? await core.call("query_all", [
            "resource": .string("stream"), "type": .string(title.type), "id": .string(videoId),
        ]) else { return [] }
        var sources: [Source] = []
        for provider in result.elements {
            let id = provider.text("installation_id")
            // Each addon reports its own failure; one bad addon must not hide the rest.
            let data = provider["result"]?["Ok"]?["data"] ?? .array([])
            for entry in data.elements {
                sources.append(Source(provider: id, name: names[id] ?? "Addon", raw: entry))
            }
        }
        return sources
    }

    /// Loads the source picker for a title and video.
    func loadSources(_ title: Title, videoId: String) async {
        state.sourcesLoading = true
        state.sources = []
        state.notices = []
        state.videoId = videoId
        await perform {
            let names = Dictionary(
                uniqueKeysWithValues: self.state.addons.map {
                    ($0.text("installation_id"), $0["manifest"]?.text("name") ?? "Addon")
                }
            )
            let result = try await self.core.call("query_all", [
                "resource": .string("stream"), "type": .string(title.type), "id": .string(videoId),
            ])
            var sources: [Source] = []
            var errors: [String] = []
            for provider in result.elements {
                let id = provider.text("installation_id")
                let name = names[id] ?? "Addon"
                if provider["result"]?["Err"] != nil { errors.append("\(name) could not return sources") }
                for entry in (provider["result"]?["Ok"]?["data"] ?? .array([])).elements {
                    sources.append(Source(provider: id, name: name, raw: entry))
                }
            }
            self.state.sources = sources
            self.state.notices = errors
        }
        state.sourcesLoading = false
    }

    /// Prepares a source and starts playback. Returns false when it cannot be played.
    @discardableResult
    private func preparePlayback(_ title: Title, videoId: String, source: Source) async -> Bool {
        do {
            let result = try await core.call("prepare", [
                "source": source.raw,
                "key": title.key,
                "video_id": .string(videoId),
                // Unlike Android, AVPlayer cannot play a torrent directly, so the
                // client advertises the internal-media capability and streams the
                // bytes itself through AVAssetResourceLoader.
                "capabilities": [
                    "torrent": true, "request_headers": true,
                    "url_schemes": .array([.string("http"), .string("https")]),
                ],
            ])
            let kind = result["delivery"]?.text("kind") ?? ""
            guard kind == "direct" || kind == "torrent" else {
                state.error = "This source cannot play on this device. Choose another source."
                return false
            }
            _ = try await core.call("hide_continue", ["key": title.key, "hidden": false])
            let next = try await core.call("episode", [
                "meta": title.raw, "key": title.key, "current": .string(videoId), "today": .string(Self.today),
            ])
            let previous = try await core.call("episode", [
                "meta": title.raw, "key": title.key, "current": .string(videoId),
                "previous": true, "today": .string(Self.today),
            ])
            state.playback = Playback(
                title: title,
                videoId: videoId,
                source: source,
                prepared: result,
                nextVideo: next["video"]?.text("id"),
                previousVideo: previous["video"]?.text("id"),
                preferences: state.preferences
            )
            return true
        } catch {
            state.error = error.localizedDescription
            return false
        }
    }

    /// Returns false when the core refused the source, so the picker can show why.
    @discardableResult
    func play(_ title: Title, videoId: String, source: Source) async -> Bool {
        var started = false
        await perform { started = await self.preparePlayback(title, videoId: videoId, source: source) }
        return started
    }

    /// Replaces the running source or episode, persisting progress first.
    func replacePlayback(
        _ old: Playback,
        videoId: String,
        source: Source,
        positionMs: Double,
        durationMs: Double,
        completed: Bool
    ) async {
        await perform {
            if durationMs > 0 {
                await self.recordProgress(
                    old, positionMs: min(max(positionMs, 0), durationMs),
                    durationMs: durationMs, completed: completed
                )
            }
            await self.refreshSnapshot()
            await self.preparePlayback(old.title, videoId: videoId, source: source)
            if !old.token.isEmpty { _ = try? await self.core.call("revoke", ["token": .string(old.token)]) }
        }
    }

    func playerSources(_ title: Title, videoId: String) async -> [Source] {
        await fetchSources(title, videoId: videoId)
    }

    func torrentStats(_ token: String) async -> JSONValue? {
        try? await core.call("torrent_stats", ["token": .string(token)])
    }

    /// Closes the player, optionally continuing to the next episode's sources.
    func closePlayer(next: Bool) async {
        guard let playback = state.playback else { return }
        state.playback = nil
        await recordProgress(playback, positionMs: nil, durationMs: nil, completed: false)
        if !playback.token.isEmpty {
            _ = try? await core.call("revoke", ["token": .string(playback.token)])
        }
        await refreshSnapshot()
        if next, let nextVideo = playback.nextVideo {
            push(.sources(playback.title, videoId: nextVideo))
        }
    }

    /// Persists the playback position. Called on a timer and at the end of playback.
    /// The managed torrents, for the torrents page.
    func torrents() async -> [ManagedTorrent] {
        guard let value = try? await core.call("torrents") else { return [] }
        return value.elements.compactMap(ManagedTorrent.init)
    }

    /// Opens a managed torrent's file for playback.
    ///
    /// The core prioritises the file as part of this call, which is what gets the bytes
    /// the player is about to ask for fetched ahead of the rest of the swarm. A managed
    /// torrent has no catalog title behind it, so the player is given the torrent's own
    /// name and everything else it needs comes from the media token.
    func openTorrent(_ torrent: ManagedTorrent) async -> Playback? {
        guard let value = try? await core.call("torrent_play", [
            "id": .string(torrent.id),
            "file": .number(0),
        ]), let token = value["token"]?.stringValue.nilIfEmpty else { return nil }
        let name = torrent.fileName
        return Playback(
            title: Title(provider: "torrent", raw: [
                "id": .string(torrent.id),
                "name": .string(name),
                "type": .string("movie"),
            ]),
            videoId: torrent.id,
            source: Source(provider: "torrent", name: name, raw: .object([:])),
            prepared: [
                "delivery": [
                    "kind": "torrent",
                    "media": ["token": .string(token)],
                ]
            ],
            nextVideo: nil,
            previousVideo: nil,
            preferences: .object([:])
        )
    }

    /// Stops managing a torrent and lets the core remove its downloaded files.
    func removeTorrent(id: String) async {
        _ = try? await core.call("torrent_remove", ["id": .string(id)])
    }

    func saveProgress(_ playback: Playback, positionMs: Double, durationMs: Double, completed: Bool = false) async {
        guard positionMs >= 0, durationMs > 0 else { return }
        await recordProgress(
            playback,
            positionMs: min(positionMs, durationMs),
            durationMs: durationMs,
            completed: completed
        )
    }

    private func recordProgress(
        _ playback: Playback,
        positionMs: Double?,
        durationMs: Double?,
        completed: Bool
    ) async {
        var payload: JSONValue = [
            "key": playback.title.key,
            "metadata": playback.title.raw,
            "video_id": .string(playback.videoId),
            "source_provider": .string(playback.source.provider),
        ]
        if let positionMs { payload["position_ms"] = .number(positionMs) }
        if let durationMs {
            payload["duration_ms"] = .number(durationMs)
            payload["completed"] = .bool(completed)
        }
        let group = playback.source.bingeGroup
        if !group.isEmpty { payload["binge_group"] = .string(group) }
        _ = try? await core.call("progress", payload)
    }

    // MARK: - Snapshot

    private func refreshSnapshot() async {
        if let snapshot = try? await core.call("snapshot") {
            state.snapshot = snapshot
        }
    }

    func loadCalendar() async {
        await perform {
            if let calendar = try? await self.core.call("calendar") {
                self.state.calendar = calendar
            }
        }
    }

    // MARK: - Addons

    func install(url: String, allowLocal: Bool) async {
        await perform {
            _ = try await self.core.call("install", [
                "url": .string(url.trimmingCharacters(in: .whitespacesAndNewlines)),
                "allow_local": .bool(allowLocal),
            ])
            await self.refreshSnapshot()
        }
    }

    /// Reconfiguring a shared installation affects every linked profile.
    func configureAddon(id: String, url: String, allowLocal: Bool) async {
        await perform {
            _ = try await self.core.call("configure_addon", [
                "id": .string(id),
                "url": .string(url.trimmingCharacters(in: .whitespacesAndNewlines)),
                "allow_local": .bool(allowLocal),
            ])
            await self.refreshSnapshot()
        }
    }

    func shareAddon(_ addon: JSONValue, targetId: String, pin: String) async {
        await perform {
            _ = try await self.core.call("share", [
                "id": .string(addon.text("installation_id")),
                "target_id": .string(targetId),
                "pin": .string(pin),
            ])
            await self.refreshSnapshot()
        }
    }

    func linkedProfiles(_ addon: JSONValue) async -> [String] {
        guard let result = try? await core.call("linked_profiles", ["id": .string(addon.text("installation_id"))]) else {
            return []
        }
        return result.elements.map(\.stringValue)
    }

    func toggleAddon(_ addon: JSONValue) async {
        await perform {
            _ = try await self.core.call("enable", [
                "id": .string(addon.text("installation_id")),
                "enabled": .bool(!addon.boolean("enabled")),
            ])
            await self.refreshSnapshot()
        }
    }

    func removeAddon(_ addon: JSONValue) async {
        await perform {
            _ = try await self.core.call("remove_addon", ["id": .string(addon.text("installation_id"))])
            await self.refreshSnapshot()
        }
    }

    /// Persists a new addon order. The core stores the list itself, so the whole
    /// order is written rather than a single move.
    func reorderAddons(_ ids: [String]) async {
        await perform {
            _ = try await self.core.call("reorder", .array(ids.map { .string($0) }))
            await self.refreshSnapshot()
        }
    }

    func moveAddon(_ addon: JSONValue, by delta: Int) async {
        await perform {
            var ids = self.state.snapshot.objects("addons").map { $0.text("installation_id") }
            guard let index = ids.firstIndex(of: addon.text("installation_id")) else { return }
            let target = index + delta
            guard ids.indices.contains(target) else { return }
            ids.swapAt(index, target)
            // Order in the snapshot is what the core stores; the list is a plain array.
            _ = try await self.core.call("reorder", .array(ids.map { .string($0) }))
            await self.refreshSnapshot()
        }
    }

    // MARK: - Playback preferences

    /// Writes the profile-wide playback preferences and refreshes the snapshot.
    func savePreferences(_ mutate: (inout JSONValue) -> Void) async {
        var next = state.preferences
        mutate(&next)
        await perform {
            _ = try await self.core.call("preferences", next)
            await self.refreshSnapshot()
        }
    }

    func setPreferences(_ preferences: JSONValue) async {
        await perform {
            _ = try await self.core.call("preferences", preferences)
            await self.refreshSnapshot()
        }
    }

    // MARK: - Trakt

    var traktCredentials: (clientId: String, clientSecret: String, redirect: String) {
        (
            defaults.string(forKey: "trakt_client_id") ?? "",
            defaults.string(forKey: "trakt_client_secret") ?? "",
            defaults.string(forKey: "trakt_redirect") ?? "urn:ietf:wg:oauth:2.0:oob"
        )
    }

    func saveTraktCredentials(clientId: String, clientSecret: String, redirect: String) {
        defaults.set(clientId.trimmingCharacters(in: .whitespacesAndNewlines), forKey: "trakt_client_id")
        defaults.set(clientSecret.trimmingCharacters(in: .whitespacesAndNewlines), forKey: "trakt_client_secret")
        defaults.set(
            redirect.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "urn:ietf:wg:oauth:2.0:oob",
            forKey: "trakt_redirect"
        )
    }

    func refreshTrakt() async {
        if let status = try? await core.call("trakt_status") {
            state.trakt = status
        }
    }

    func traktConnect() async {
        let credentials = traktCredentials
        await perform {
            let code = try await self.core.call("trakt_connect_start", [
                "client_id": .string(credentials.clientId),
                "client_secret": .string(credentials.clientSecret),
                "redirect_uri": .string(credentials.redirect),
            ])
            self.state.traktDevice = code
            self.pollTrakt(code)
        }
    }

    func traktCancel() {
        traktTask?.cancel()
        traktTask = nil
        state.traktDevice = .object([:])
    }

    /// Polls until the device code is authorized, is refused, or expires.
    private func pollTrakt(_ code: JSONValue) {
        traktTask?.cancel()
        traktTask = Task { [weak self] in
            guard let self else { return }
            let deadline = Date().addingTimeInterval(max(code.number("expires_in"), 60))
            var interval = max(code.number("interval"), 1)
            while Date() < deadline, !Task.isCancelled {
                try? await Task.sleep(for: .seconds(interval))
                if Task.isCancelled { return }
                do {
                    let result = try await core.call("trakt_connect_poll")
                    switch result.text("status") {
                    case "authorized":
                        state.traktDevice = .object([:])
                        await refreshTrakt()
                        return
                    case "slow_down":
                        interval = max(result.number("interval"), interval)
                    default:
                        break
                    }
                } catch {
                    state.traktDevice = .object([:])
                    state.error = error.localizedDescription
                    return
                }
            }
            state.traktDevice = .object([:])
            state.error = "The Trakt code expired. Connect again for a new code."
        }
    }

    func traktSync() async {
        await perform {
            _ = try await self.core.call("trakt_sync", ["stale": false])
            await self.refreshTrakt()
            await self.refreshSnapshot()
        }
    }

    func traktDisconnect() async {
        await perform {
            _ = try await self.core.call("trakt_disconnect")
            self.state.trakt = ["connected": false]
        }
    }

    // MARK: - Web settings

    /// Starts the LAN settings server the core hosts, and remembers the address to
    /// show. iOS prompts for local-network access the first time this binds.
    func startWeb() async {
        do {
            let status = try await core.call("web_start")
            state.web = status
            state.webAddress = await webAddress(status)
        } catch {
            state.web = ["running": false]
            state.webAddress = error.localizedDescription
        }
    }

    func toggleWeb() async {
        if state.web.boolean("running") {
            let status = (try? await core.call("web_stop")) ?? .object([:])
            state.web = status
            state.webAddress = ""
        } else {
            await startWeb()
        }
    }

    /// The address a browser on the same network can reach. The core binds
    /// `0.0.0.0`; this picks the device's own IPv4 address to display.
    private func webAddress(_ status: JSONValue) async -> String {
        guard status.boolean("running") else { return "" }
        let port = status.integer("port") == 0 ? 11471 : status.integer("port")
        guard let ip = LocalNetwork.ipv4Address() else {
            return "Connect this device to Wi-Fi"
        }
        return "http://\(ip):\(port)"
    }

    // MARK: - Player display preferences (device-wide)

    var subtitleSize: String { defaults.string(forKey: "player_subtitle_size") ?? "medium" }
    var pictureSize: Int { defaults.integer(forKey: "player_resize") }
    var defaultSpeed: Double {
        let value = defaults.double(forKey: "player_speed")
        return value == 0 ? 1 : value
    }

    func setPlayerPreference(_ key: String, _ value: Any) {
        defaults.set(value, forKey: key)
        objectWillChange.send()
    }

    // MARK: - Plumbing

    /// Runs an operation with the shared busy/error handling of the TV client.
    ///
    /// Unlike Android there is no cancellation-by-replacement: sheets and stacks
    /// close on their own, and cancelling a native call mid-flight would leave the
    /// player's reader registry in an unknown state.
    private func perform(_ block: @escaping () async throws -> Void) async {
        state.busy = true
        state.error = nil
        do {
            try await block()
        } catch is CancellationError {
            // A cancelled task must not surface an error banner.
        } catch {
            state.error = error.localizedDescription
        }
        state.busy = false
    }

    func dismissError() {
        state.error = nil
    }

    static var today: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        return formatter.string(from: Date())
    }
}

extension String {
    /// The core treats an empty string and an absent field differently in a few
    /// places; this makes "was it really empty" explicit at the call site.
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
