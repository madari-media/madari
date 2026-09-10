import Foundation

/// JSON-backed domain types, mirroring the Android client.
///
/// Addons may attach arbitrary fields to a title or a source, so every type keeps
/// its original document in `raw` and reads through it. Nothing here has to know
/// the full Stremio schema, and unknown fields survive a round trip to the core.

struct Title: Sendable, Hashable, Identifiable {
    let provider: String
    let raw: JSONValue

    /// Stable across providers, unlike the bare content id.
    var id: String { identity }
    var contentId: String { raw.text("id") }
    var type: String { raw.text("type") }
    var name: String {
        let name = raw.text("name")
        return name.isEmpty ? contentId : name
    }
    var poster: String { raw.text("poster") }
    var background: String {
        let background = raw.text("background")
        return background.isEmpty ? poster : background
    }
    var description: String { raw.text("description") }
    var logo: String { raw.text("logo") }
    var releaseInfo: String { raw.text("releaseInfo") }
    var runtime: String { raw.text("runtime") }
    var imdbRating: String { raw.text("imdbRating") }
    var genres: [String] { raw.strings("genres") }
    var identity: String { "\(provider)|\(type)|\(contentId)" }

    /// The key the core stores library entries, progress and preferences against.
    var key: JSONValue {
        [
            "installation_id": .string(provider),
            "content_type": .string(type),
            "item_id": .string(contentId),
        ]
    }

    /// Episodes in broadcast order, which is the order the core reasons about.
    var videos: [JSONValue] {
        raw.objects("videos").sorted { left, right in
            if left.integer("season") != right.integer("season") {
                return left.integer("season") < right.integer("season")
            }
            return left.integer("episode") < right.integer("episode")
        }
    }

    var isSeries: Bool { type == "series" }
}

struct Catalog: Sendable, Hashable, Identifiable {
    let provider: String
    let providerName: String
    let raw: JSONValue

    var id: String { identity }
    var catalogId: String { raw.text("id") }
    var type: String { raw.text("type") }
    var name: String {
        let name = raw.text("name")
        return name.isEmpty ? catalogId : name
    }
    var identity: String { "\(provider)|\(type)|\(catalogId)" }
    var extras: [JSONValue] { raw.objects("extra") }

    /// A declared `search` extra is what makes a catalog usable from the search tab.
    var searchable: Bool {
        extras.contains { $0.text("name") == "search" } || raw.strings("extraSupported").contains("search")
    }
    /// Only catalogs that declare `skip` can be paged.
    var pageable: Bool {
        extras.contains { $0.text("name") == "skip" } || raw.strings("extraSupported").contains("skip")
    }
    var required: [String] {
        extras.filter { $0.boolean("isRequired") }.map { $0.text("name") } + raw.strings("extraRequired")
    }
    /// The fields the Explore screen offers, in declaration order, without `skip`
    /// which the client manages itself.
    var formFields: [String] {
        (extras.map { $0.text("name") } + required).filter { !$0.isEmpty && $0 != "skip" }.reduce(into: [String]()) {
            if !$0.contains($1) { $0.append($1) }
        }
    }
    func options(for field: String) -> [String] {
        extras.first { $0.text("name") == field }?.strings("options") ?? []
    }
    /// The declared type of a field, used to pick a keyboard on iOS.
    func isNumeric(_ field: String) -> Bool {
        extras.first { $0.text("name") == field }?.text("type") == "number"
    }
}

struct Shelf: Sendable, Identifiable {
    let id: String
    let name: String
    let titles: [Title]
    var catalog: Catalog?
    var skip: Int = 0
    var more: Bool = false
    var extras: [String: String] = [:]

    /// A row's heading carries the catalog type, matching the TV client.
    var heading: String {
        switch catalog?.type {
        case "movie": "\(name) · Movies"
        case "series": "\(name) · Series"
        default: name
        }
    }
}

struct Source: Sendable, Hashable {
    let provider: String
    let name: String
    let raw: JSONValue

    var identity: String {
        "\(provider)|\(raw.text("name"))|\(raw.text("url"))|\(raw.text("infoHash"))|\(raw.integer("fileIdx"))"
    }
    /// Many addons put multi-line quality information in `name`; the picker shows
    /// it on one line and falls back to the addon name.
    var displayName: String {
        let name = raw.text("name")
        return (name.isEmpty ? self.name : name).replacingOccurrences(of: "\n", with: " · ")
    }
    var detail: String {
        let description = raw.text("description").isEmpty ? raw.text("title") : raw.text("description")
        return description.replacingOccurrences(of: "\n", with: " · ")
    }
    /// The group the core uses to reuse this source for the next episode.
    var bingeGroup: String { raw["behaviorHints"]?["bingeGroup"]?.stringValue ?? "" }
}

/// What the player needs to start: the selected video, its source and the core's plan.
struct Playback: Sendable, Identifiable {
    let title: Title
    let videoId: String
    let source: Source
    let prepared: JSONValue
    let nextVideo: String?
    let previousVideo: String?
    let preferences: JSONValue

    var id: String { "\(title.identity)|\(videoId)" }
    var delivery: JSONValue { prepared["delivery"] ?? .object([:]) }
    var token: String { delivery["media"]?["token"]?.stringValue ?? "" }
    var isTorrent: Bool { delivery.text("kind") == "torrent" }
    /// Torrent bytes never leave the process, so they stream through an internal URI
    /// that `AVAssetResourceLoader` resolves.
    var uri: String {
        isTorrent ? "madari-internal://\(token)" : delivery.text("url")
    }
    var resumeMs: Int64 { Int64(prepared["plan"]?["resume_ms"]?.doubleValue ?? 0) }
    /// Per-source credentials. They must not be forwarded to unrelated subtitle URLs.
    var requestHeaders: [String: String] {
        guard let headers = delivery["request_headers"] else { return [:] }
        return headers.fields.compactMapValues { value in
            if case .string(let text) = value { return text }
            return nil
        }
    }
    /// Inline subtitles declared by the source addon.
    var subtitles: [(url: String, language: String, isVTT: Bool)] {
        (source.raw["subtitles"] ?? .array([])).elements.compactMap { entry in
            let url = entry.text("url")
            guard let parsed = URL(string: url), let scheme = parsed.scheme,
                  scheme == "http" || scheme == "https" else { return nil }
            return (url, entry.text("lang"), parsed.path.lowercased().hasSuffix(".vtt"))
        }
    }

    var episode: JSONValue? {
        title.videos.first { $0.text("id") == videoId }
    }
    /// Episode artwork when playing an episode, otherwise the title poster.
    var artwork: String {
        let thumbnail = episode?.text("thumbnail") ?? ""
        return thumbnail.isEmpty ? title.poster : thumbnail
    }
}

/// One audio or subtitle track the player can switch to.
struct MediaTrack: Sendable, Hashable, Identifiable {
    let id: Int
    let kind: String
    let label: String
    let language: String
    let codecs: String
    let channels: Int
    let selected: Bool

    var isAudio: Bool { kind == "audio" }
    var display: String {
        let language = Locale.current.localizedString(forLanguageCode: self.language) ?? self.language
        let parts = [label, language, codecs, channels > 0 ? "\(channels) ch" : ""]
        let joined = parts.filter { !$0.isEmpty }.joined(separator: " · ")
        return joined.isEmpty ? "Track \(id)" : joined
    }
}
