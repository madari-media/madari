import AVFoundation
import AVKit
import SwiftUI

/// The player.
///
/// Controls are drawn rather than delegated to `AVPlayerViewController` so the same
/// actions exist as on the TV client: episode and source switching, audio and
/// subtitle selection, playback speed, picture size and torrent statistics. AVPlayer
/// decodes the media; everything around it is shared policy from the core.
struct PlayerView: View {
    let playback: Playback
    /// Offered when libmpv can play this source, so the user can hand a stubborn file
    /// to the other backend.
    var onUseMpv: (() -> Void)? = nil

    @EnvironmentObject private var model: AppModel

    @StateObject private var engine = PlayerEngine()
    @State private var position: Double = 0
    @State private var duration: Double = 0
    @State private var playing = false
    @State private var ended = false
    @State private var failure: String?
    @State private var containerWarning: String?
    @State private var controlsVisible = true
    @State private var scrubbing = false
    @State private var scrubTarget: Double = 0
    @State private var pictureSize = 0
    @State private var speed = 1.0
    @State private var torrentStats: JSONValue?
    @State private var optionsOpen = false
    @State private var allSources: [Source] = []
    @State private var hideControls: Task<Void, Never>?

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            PlayerSurface(player: engine.player, pictureSize: pictureSize)
                .ignoresSafeArea()

            PlayerChrome(
                playback: playback,
                engine: engine,
                sources: allSources,
                torrentStats: torrentStats,
                speed: $speed,
                pictureSize: $pictureSize,
                onClose: close,
                onSwitchSource: switchSource,
                onSwitchEpisode: switchEpisode,
                onRestart: restart,
                otherBackend: onUseMpv.map { ("Play with libmpv", $0) }
            )
        }
        .statusBarHidden()
        .task {
            // Claims playback, so the libmpv backend stands down if it was playing.
            PlaybackCoordinator.shared.activate(engine)
            await start()
        }
        .task { await observeEngine() }
        .task { await trackTorrent() }
        .task {
            allSources = await model.playerSources(playback.title, videoId: playback.videoId)
            pictureSize = model.pictureSize
            speed = model.defaultSpeed
            engine.setSpeed(speed)
        }
        .onDisappear {
            PlaybackCoordinator.shared.deactivate(engine)
            engine.stop()
            Task {
                await model.saveProgress(
                    playback,
                    positionMs: engine.positionMs,
                    durationMs: engine.durationMs,
                    completed: ended
                )
            }
        }
    }

    // MARK: - Controls

    private var controls: some View {
        VStack {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(playback.title.name)
                        .font(MadariFont.semibold(17))
                        .foregroundStyle(.white)
                    Text(episodeCaption)
                        .madariLabel()
                        .foregroundStyle(.white.opacity(0.75))
                }
                Spacer()
                Button {
                    close()
                } label: {
                    GlyphIcon(glyph: .close, size: 16)
                        .padding(9)
                        .background(.black.opacity(0.45), in: Circle())
                        .foregroundStyle(.white)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close player")
            }
            .padding(18)
            .background(
                LinearGradient(colors: [.black.opacity(0.8), .clear], startPoint: .top, endPoint: .bottom)
                    .ignoresSafeArea(edges: .top)
            )

            Spacer()

            VStack(spacing: 10) {
                HStack(spacing: 6) {
                    Text(Self.timeLabel(scrubbing ? scrubTarget : position)).madariLabel().foregroundStyle(.white)
                    Text("/").madariLabel().foregroundStyle(.white.opacity(0.5))
                    Text(Self.timeLabel(duration)).madariLabel().foregroundStyle(.white.opacity(0.6))
                    Spacer()
                    if engine.buffering {
                        ProgressView().tint(.white).scaleEffect(0.7)
                    }
                    Text("\(Self.speedLabel(speed))").madariLabel().foregroundStyle(.white.opacity(0.6))
                }
                Slider(
                    value: Binding(
                        get: { scrubbing ? scrubTarget : position },
                        set: { scrubTarget = $0 }
                    ),
                    in: 0...max(duration, 1),
                    onEditingChanged: { editing in
                        if editing {
                            scrubbing = true
                            scrubTarget = position
                        } else {
                            engine.seek(to: scrubTarget)
                            position = scrubTarget
                            scrubbing = false
                            showControls()
                        }
                    }
                )
                .tint(MadariColors.accent)
                .accessibilityLabel("Playback position")

                HStack(spacing: 22) {
                    Button {
                        engine.seek(by: -10)
                        showControls()
                    } label: {
                        GlyphIcon(glyph: .backward, size: 20)
                    }
                    Button {
                        engine.togglePlay()
                        showControls()
                    } label: {
                        GlyphIcon(glyph: playing ? .pause : .play, size: 26)
                    }
                    Button {
                        engine.seek(by: 10)
                        showControls()
                    } label: {
                        GlyphIcon(glyph: .forward, size: 20)
                    }

                    Spacer()

                    if playback.previousVideo != nil {
                        Button("Previous") { switchEpisode(playback.previousVideo) }
                            .buttonStyle(.plain)
                            .font(MadariFont.regular(13))
                    }
                    if let next = playback.nextVideo {
                        Button("Next") { switchEpisode(next) }
                            .buttonStyle(.plain)
                            .font(MadariFont.regular(13))
                    }
                    Button {
                        optionsOpen = true
                    } label: {
                        GlyphIcon(glyph: .sliders, size: 20)
                    }
                    .accessibilityLabel("Playback options")
                }
                .foregroundStyle(.white)
            }
            .padding(18)
            .background(
                LinearGradient(colors: [.clear, .black.opacity(0.9)], startPoint: .top, endPoint: .bottom)
                    .ignoresSafeArea(edges: .bottom)
            )
        }
    }

    private var episodeCaption: String {
        if let episode = playback.episode {
            let title = episode.text("title").nilIfEmpty ?? ""
            return "Season \(episode.integer("season")) · Episode \(episode.integer("episode"))  \(title)"
        }
        return playback.source.name
    }

    // MARK: - Lifecycle

    private func start() async {
        let format = MediaFormat.inspect(playback)
        containerWarning = format.warning
        // Torrents stream through the resource loader; direct URLs, including HLS,
        // are left to AVFoundation.
        let loader = playback.isTorrent
            ? TorrentResourceLoader(uri: playback.uri, contentType: format.mime)
            : nil
        engine.load(
            uri: playback.uri,
            startAt: Double(playback.resumeMs) / 1000,
            headers: playback.requestHeaders,
            loader: loader,
            preferences: playback.preferences
        )
        showControls()
    }

    private func observeEngine() async {
        while !Task.isCancelled {
            position = engine.position
            duration = engine.duration
            playing = engine.playing
            ended = engine.ended
            failure = engine.failure
            // Once real frames arrive the container clearly decoded, so stop warning.
            if engine.position > 0.5 { containerWarning = nil }
            try? await Task.sleep(for: .milliseconds(400))
        }
    }

    private func trackTorrent() async {
        guard playback.isTorrent else { return }
        while !Task.isCancelled {
            torrentStats = await model.torrentStats(playback.token)
            try? await Task.sleep(for: .milliseconds(1500))
        }
    }

    private func restart() {
        ended = false
        failure = nil
        engine.seek(to: 0)
        engine.play()
        showControls()
    }

    private func switchSource(_ source: Source) {
        let position = engine.positionMs
        let duration = engine.durationMs
        engine.stop()
        Task {
            await model.replacePlayback(
                playback, videoId: playback.videoId, source: source,
                positionMs: position, durationMs: duration, completed: false
            )
        }
    }

    private func switchEpisode(_ videoId: String?) {
        guard let videoId else { return }
        let position = engine.positionMs
        let duration = engine.durationMs
        let completed = ended
        engine.stop()
        Task {
            await model.replacePlayback(
                playback, videoId: videoId, source: playback.source,
                positionMs: position, durationMs: duration, completed: completed
            )
        }
    }

    private func close() {
        let completed = ended
        let positionMs = engine.positionMs
        let durationMs = engine.durationMs
        engine.stop()
        model.state.playback = nil
        Task {
            await model.saveProgress(playback, positionMs: positionMs, durationMs: durationMs, completed: completed)
            await model.closePlayer(next: false)
        }
    }

    /// Keeps the controls up while paused, never hides them mid-scrub or with the
    /// options sheet open, matching the TV client's behaviour.
    private func showControls() {
        controlsVisible = true
        hideControls?.cancel()
        hideControls = Task {
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled, playing, !optionsOpen, failure == nil, !ended, !scrubbing else { return }
            withAnimation(.easeOut(duration: 0.3)) { controlsVisible = false }
        }
    }

    private static func timeLabel(_ seconds: Double) -> String {
        let total = Int(max(seconds, 0))
        return total >= 3600
            ? String(format: "%d:%02d:%02d", total / 3600, total / 60 % 60, total % 60)
            : String(format: "%d:%02d", total / 60, total % 60)
    }

    private static func speedLabel(_ value: Double) -> String {
        value == value.rounded() ? "\(Int(value))×" : "\(value)×"
    }
}

// MARK: - Playback engine

/// One selectable audio or subtitle track offered by AVFoundation.
struct TrackOption: Identifiable, Hashable {
    let id: Int
    let label: String
    let language: String

    var display: String {
        let name = language.isEmpty
            ? ""
            : (Locale.current.localizedString(forLanguageCode: language) ?? language)
        let parts = [label, name].filter { !$0.isEmpty }
        return parts.isEmpty ? "Track \(id + 1)" : parts.joined(separator: " · ")
    }
}

/// Owns the `AVPlayer` and publishes its state.
///
/// `ObservableObject` rather than `@Observable`: the observation macro's compiler
/// plugin does not build in this cross-compile setup (xtool #197).
@MainActor
final class PlayerEngine: ObservableObject {
    let player = AVPlayer()

    @Published private(set) var position: Double = 0
    @Published private(set) var duration: Double = 0
    @Published private(set) var playing = false
    @Published private(set) var buffering = false
    @Published private(set) var ended = false
    @Published private(set) var failure: String?
    @Published private(set) var audioTracks: [TrackOption] = []
    @Published private(set) var subtitleTracks: [TrackOption] = []
    @Published private(set) var selectedAudio: Int?
    @Published private(set) var selectedSubtitle: Int?
    @Published private(set) var volume: Double = 1
    @Published private(set) var muted = false

    private var loader: TorrentResourceLoader?
    private var timeObserver: Any?
    private var observers: [NSObjectProtocol] = []
    private var statusObservation: NSKeyValueObservation?
    private var rateObservation: NSKeyValueObservation?
    private var audioGroup: AVMediaSelectionGroup?
    private var legibleGroup: AVMediaSelectionGroup?
    private var audioOptions: [Int: AVMediaSelectionOption] = [:]
    private var legibleOptions: [Int: AVMediaSelectionOption] = [:]
}

extension PlayerEngine {
    func load(
        uri: String,
        startAt: Double,
        headers: [String: String],
        loader: TorrentResourceLoader?,
        preferences: JSONValue
    ) {
        self.loader = loader
        failure = nil
        ended = false

        let item: AVPlayerItem
        if let loader, let url = URL(string: uri) {
            // A custom scheme is only playable through a resource-loader delegate.
            let asset = AVURLAsset(url: url)
            asset.resourceLoader.setDelegate(loader, queue: .global(qos: .userInitiated))
            item = AVPlayerItem(asset: asset)
        } else if let url = URL(string: uri) {
            var options: [String: Any] = [:]
            if !headers.isEmpty {
                options["AVURLAssetHTTPHeaderFieldsKey"] = headers
            }
            item = AVPlayerItem(asset: AVURLAsset(url: url, options: options))
        } else {
            failure = "This source does not have a playable address."
            return
        }

        player.replaceCurrentItem(with: item)
        observe(item)
        if startAt > 0 {
            player.seek(to: CMTime(seconds: startAt, preferredTimescale: 600))
        }
        player.play()
        playing = true
        Task { await loadTracks(for: item, preferences: preferences) }
    }

    func stop() {
        if let timeObserver { player.removeTimeObserver(timeObserver) }
        timeObserver = nil
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
        statusObservation = nil
        rateObservation = nil
        player.pause()
        player.replaceCurrentItem(with: nil)
        // Release the torrent reader only once the item no longer holds the asset.
        loader = nil
        playing = false
    }

    func play() { player.play(); playing = true }
    func pause() { player.pause(); playing = false }
    func togglePlay() { playing ? pause() : play() }

    func seek(to seconds: Double) {
        player.seek(to: CMTime(seconds: max(seconds, 0), preferredTimescale: 600))
    }

    func seek(by delta: Double) {
        seek(to: position + delta)
    }

    func setSpeed(_ value: Double) {
        // `defaultRate` keeps the chosen speed across pauses on iOS 16 and later.
        player.defaultRate = Float(value)
        if player.rate != 0 { player.rate = Float(value) }
    }

    func select(audio: Int?) {
        selectedAudio = audio
        guard let group = audioGroup else { return }
        if let audio, let option = audioOptions[audio] {
            player.currentItem?.select(option, in: group)
        } else {
            player.currentItem?.selectMediaOptionAutomatically(in: group)
        }
    }

    func select(subtitle: Int?) {
        selectedSubtitle = subtitle
        guard let group = legibleGroup else { return }
        if let subtitle, let option = legibleOptions[subtitle] {
            player.currentItem?.select(option, in: group)
        } else {
            player.currentItem?.select(nil, in: group)
        }
    }

    func setVolume(_ value: Double) {
        volume = value
        player.volume = Float(value)
        if value > 0, muted { toggleMute() }
    }

    func toggleMute() {
        muted.toggle()
        player.isMuted = muted
    }

    /// The core records progress in milliseconds, while the player's own clock is in
    /// seconds. Converting here keeps the two units apart at the call sites; passing
    /// seconds as milliseconds is what left Continue watching and resume stuck near the
    /// start of every title.
    var positionMs: Double { position * 1000 }
    var durationMs: Double { duration * 1000 }
}

private extension PlayerEngine {
    func observe(_ item: AVPlayerItem) {
        if let timeObserver { player.removeTimeObserver(timeObserver) }
        // AVFoundation delivers these on the queue given, but the closure is declared
        // `@Sendable`, so the compiler cannot see that it is already on the main actor.
        // `assumeIsolated` states that, rather than hopping through a task per tick.
        timeObserver = player.addPeriodicTimeObserver(
            forInterval: CMTime(seconds: 0.4, preferredTimescale: 600), queue: .main
        ) { [weak self] time in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.position = time.seconds.isFinite ? time.seconds : 0
                let total = item.duration.seconds
                self.duration = total.isFinite ? total : 0
                self.buffering = item.isPlaybackBufferEmpty
            }
        }
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = [
            NotificationCenter.default.addObserver(
                forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.ended = true
                    self?.playing = false
                }
            },
            NotificationCenter.default.addObserver(
                forName: .AVPlayerItemPlaybackStalled, object: item, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.buffering = true }
            },
        ]
        // Key-value observations fire on whichever thread changed the value, so these
        // hop to the main actor instead of assuming it.
        statusObservation = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard item.status == .failed else { return }
            // Describing the error is pure, so it stays off the actor and only the
            // resulting string crosses back to it.
            let message = Self.describe(item.error)
            Task { @MainActor [weak self] in self?.failure = message }
        }
        rateObservation = player.observe(\.rate, options: [.new]) { [weak self] player, _ in
            let playing = player.rate != 0
            Task { @MainActor [weak self] in self?.playing = playing }
        }
    }

    /// Builds the track list from what AVFoundation offers, then applies the
    /// profile's preferences through the core's own adapter, so the same language
    /// priority and SDH/forced/commentary rules apply as on the TV client.
    func loadTracks(for item: AVPlayerItem, preferences: JSONValue) async {
        audioGroup = try? await item.asset.loadMediaSelectionGroup(for: .audible)
        legibleGroup = try? await item.asset.loadMediaSelectionGroup(for: .legible)

        var tracks: [JSONValue] = []
        (audioGroup?.options ?? []).enumerated().forEach { index, option in
            audioOptions[index] = option
            tracks.append(Self.describe(option, id: index, kind: "audio"))
        }
        let audioCount = audioOptions.count
        (legibleGroup?.options ?? []).enumerated().forEach { index, option in
            legibleOptions[index] = option
            tracks.append(Self.describe(option, id: index + audioCount, kind: "sub"))
        }
        audioTracks = audioOptions
            .sorted { $0.key < $1.key }
            .map { TrackOption(id: $0.key, label: $0.value.displayName, language: Self.language(of: $0.value)) }
        subtitleTracks = legibleOptions
            .sorted { $0.key < $1.key }
            .map { TrackOption(id: $0.key, label: $0.value.displayName, language: Self.language(of: $0.value)) }

        guard !tracks.isEmpty, let chosen = try? await NativeCore.shared.call("preferred_tracks", [
            "tracks": .array(tracks), "preferences": preferences,
        ]) else { return }

        if let id = chosen["audio"]?.intValue, audioOptions[id] != nil {
            select(audio: id)
        }
        let subtitlesEnabled = preferences["subtitles_enabled"]?.boolValue ?? true
        if !subtitlesEnabled {
            select(subtitle: nil)
        } else if let id = chosen["sub"]?.intValue, legibleOptions[id] != nil {
            select(subtitle: id)
        } else {
            select(subtitle: nil)
        }
    }

    static func describe(_ option: AVMediaSelectionOption, id: Int, kind: String) -> JSONValue {
        [
            "id": .number(Double(id)),
            "kind": .string(kind),
            "language": .string(language(of: option)),
            "selected": .bool(false),
            // AVFoundation exposes accessibility as media characteristics rather
            // than Media3's role flags; the core matches on the same field names.
            "hearing_impaired": .bool(option.hasMediaCharacteristic(.describesMusicAndSoundForAccessibility)),
            "visual_impaired": .bool(option.hasMediaCharacteristic(.describesVideoForAccessibility)),
            "commentary": .bool(option.hasMediaCharacteristic(.containsOnlyForcedSubtitles) == false
                                && option.hasMediaCharacteristic(.isAuxiliaryContent)),
            "forced": .bool(option.hasMediaCharacteristic(.containsOnlyForcedSubtitles)),
        ]
    }

    static func language(of option: AVMediaSelectionOption) -> String {
        option.extendedLanguageTag ?? option.locale?.identifier ?? ""
    }

    /// `nonisolated` so the key-value observation callback can describe an error
    /// without hopping to the main actor just for the string.
    nonisolated static func describe(_ error: Error?) -> String {
        guard let error else { return "This source could not play. Try another source." }
        return "Playback failed. \((error as NSError).localizedDescription)"
    }
}

// MARK: - Video surface

/// The video surface. `AVPlayerLayer` is used directly so picture size can offer the
/// same Fit/Zoom/Stretch choices as the TV client.
private struct PlayerSurface: UIViewRepresentable {
    let player: AVPlayer
    let pictureSize: Int

    func makeUIView(context: Context) -> PlayerLayerView {
        let view = PlayerLayerView()
        view.backgroundColor = .black
        view.playerLayer.player = player
        return view
    }

    func updateUIView(_ view: PlayerLayerView, context: Context) {
        view.playerLayer.player = player
        view.playerLayer.videoGravity = switch pictureSize {
        case 1: .resizeAspectFill  // Zoom
        case 2: .resize            // Stretch
        default: .resizeAspect     // Fit
        }
    }
}

final class PlayerLayerView: UIView {
    override static var layerClass: AnyClass { AVPlayerLayer.self }
    var playerLayer: AVPlayerLayer { layer as! AVPlayerLayer }
}

// MARK: - Overlays

struct TorrentBadge: View {
    let stats: JSONValue

    var body: some View {
        let total = stats.number("total")
        let percent = total > 0 ? Int(stats.number("downloaded") * 100 / total) : 0
        HStack(spacing: 7) {
            GlyphIcon(glyph: .torrent, size: 12)
            Text("\(stats.text("state")) · \(percent)% · \(stats.integer("peers")) peers · ↓ \(Int(stats.number("download_speed") / 1024)) KB/s · ↑ \(Int(stats.number("upload_speed") / 1024)) KB/s")
                .font(MadariFont.regular(11))
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(.black.opacity(0.7), in: Capsule())
    }
}

private struct RecoveryPanel: View {
    let ended: Bool
    let message: String?
    let hasNext: Bool
    let onRetry: () -> Void
    let onNext: () -> Void
    let onUseMpv: (() -> Void)?
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Text(ended ? "You've reached the end" : "Playback interrupted").madariHeading()
            if let message, !ended {
                Text(message)
                    .madariBody()
                    .foregroundStyle(MadariColors.muted)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: 10) {
                Button(ended ? "Play again" : "Retry", action: onRetry).buttonStyle(ProminentButton())
                if hasNext {
                    Button("Next episode", action: onNext).buttonStyle(QuietButton())
                }
                if let onUseMpv, !ended {
                    Button("Play with libmpv", action: onUseMpv).buttonStyle(QuietButton())
                }
                Button("Back to sources", action: onClose).buttonStyle(QuietButton())
            }
        }
        .frame(maxWidth: 520)
        .padding(26)
        .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 14))
    }
}

// MARK: - Options sheet

/// The options sheet: audio, subtitles, speed, picture size, episodes and sources,
/// mirroring the TV client's player dialog.
struct PlayerOptionsSheet<Engine: PlayerOptionsModel>: View {
    let playback: Playback
    @ObservedObject var engine: Engine
    @Binding var speed: Double
    @Binding var pictureSize: Int
    let sources: [Source]
    let onSwitchSource: (Source) -> Void
    let onSwitchEpisode: (String?) -> Void

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    private enum Page: String, CaseIterable, Identifiable {
        case menu = "Options"
        case audio = "Audio"
        case subtitles = "Subtitles"
        case speed = "Playback speed"
        case picture = "Picture size"
        case volume = "Volume"
        case episodes = "Episodes"
        case sources = "Change source"

        var id: String { rawValue }
    }

    @State private var page: Page = .menu

    var body: some View {
        NavigationStack {
            List {
                switch page {
                case .menu:
                    ForEach([Page.audio, .subtitles, .speed, .picture, .volume, .episodes, .sources]) { option in
                        Button {
                            page = option
                        } label: {
                            HStack {
                                Text(option.rawValue).madariBody()
                                Spacer()
                                Image(systemName: "chevron.right")
                                    .font(.system(size: 12, weight: .semibold))
                                    .foregroundStyle(MadariColors.muted)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                case .audio:
                    trackList(engine.audioTracks) { engine.select(audio: $0) }
                case .subtitles:
                    trackList(engine.subtitleTracks) { engine.select(subtitle: $0) }
                    // Only the libmpv backend can attach these; AVFoundation cannot add a
                    // subtitle file to an item it did not demux.
                    if engine.canAddSubtitles, !playback.subtitles.isEmpty {
                        ForEach(Array(playback.subtitles.enumerated()), id: \.offset) { _, subtitle in
                            Button {
                                engine.addSubtitle(url: subtitle.url)
                            } label: {
                                HStack {
                                    Text("Add \(subtitle.language.isEmpty ? "subtitle" : subtitle.language)").madariBody()
                                    Spacer()
                                    Image(systemName: "plus")
                                        .font(.system(size: 12, weight: .semibold))
                                        .foregroundStyle(MadariColors.muted)
                                }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                case .speed:
                    ForEach([0.5, 0.75, 1, 1.25, 1.5, 2], id: \.self) { value in
                        Button {
                            speed = value
                            engine.setSpeed(value)
                            model.setPlayerPreference("player_speed", value)
                            page = .menu
                        } label: {
                            HStack {
                                Text(Self.speedLabel(value)).madariBody()
                                Spacer()
                                if abs(speed - value) < 0.001 { Image(systemName: "checkmark") }
                            }
                        }
                        .buttonStyle(.plain)
                    }
                case .picture:
                    ForEach([(0, "Fit"), (1, "Zoom"), (2, "Stretch")], id: \.0) { value, label in
                        Button {
                            pictureSize = value
                            model.setPlayerPreference("player_resize", value)
                            page = .menu
                        } label: {
                            HStack {
                                Text(label).madariBody()
                                Spacer()
                                if pictureSize == value { Image(systemName: "checkmark") }
                            }
                        }
                        .buttonStyle(.plain)
                    }
                case .volume:
                    HStack(spacing: 10) {
                        GlyphIcon(glyph: .audio, size: 14)
                            .foregroundStyle(MadariColors.muted)
                        Slider(
                            value: Binding(get: { engine.volume }, set: { engine.setVolume($0) }),
                            in: 0...1
                        )
                    }
                    Button {
                        engine.toggleMute()
                    } label: {
                        HStack {
                            Text(engine.muted ? "Unmute" : "Mute").madariBody()
                            Spacer()
                            if engine.muted { Image(systemName: "checkmark") }
                        }
                    }
                    .buttonStyle(.plain)
                case .episodes:
                    if playback.title.videos.isEmpty {
                        Text("This title has no episodes.").madariLabel().foregroundStyle(MadariColors.muted)
                    }
                    ForEach(playback.title.videos, id: \.itemID) { episode in
                        Button {
                            dismiss()
                            onSwitchEpisode(episode.text("id"))
                        } label: {
                            HStack {
                                Text("S\(episode.integer("season")) E\(episode.integer("episode")) · \(episode.text("title"))")
                                    .madariBody()
                                Spacer()
                                if episode.text("id") == playback.videoId { Image(systemName: "checkmark") }
                            }
                        }
                        .buttonStyle(.plain)
                    }
                case .sources:
                    if sources.isEmpty {
                        Text("No sources available for this episode.")
                            .madariLabel().foregroundStyle(MadariColors.muted)
                    }
                    ForEach(Array(sources.enumerated()), id: \.offset) { _, source in
                        Button {
                            dismiss()
                            onSwitchSource(source)
                        } label: {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(source.displayName).madariBody().lineLimit(2)
                                Text(source.name).madariLabel().foregroundStyle(MadariColors.muted)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .navigationTitle(page.rawValue)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    if page == .menu {
                        Button("Close") { dismiss() }
                    } else {
                        Button("Back") { page = .menu }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func trackList(_ tracks: [TrackOption], select: @escaping (Int?) -> Void) -> some View {
        let isAudio = page == .audio
        Button {
            select(nil)
            page = .menu
        } label: {
            HStack {
                Text(isAudio ? "Automatic" : "Off").madariBody()
                Spacer()
                let current = isAudio ? engine.selectedAudio : engine.selectedSubtitle
                if current == nil { Image(systemName: "checkmark") }
            }
        }
        .buttonStyle(.plain)
        ForEach(tracks) { track in
            Button {
                select(track.id)
                page = .menu
            } label: {
                HStack {
                    Text(track.display).madariBody()
                    Spacer()
                    let current = isAudio ? engine.selectedAudio : engine.selectedSubtitle
                    if current == track.id { Image(systemName: "checkmark") }
                }
            }
            .buttonStyle(.plain)
        }
        if tracks.isEmpty {
            Text("No \(isAudio ? "audio" : "subtitle") tracks reported yet.")
                .madariLabel().foregroundStyle(MadariColors.muted)
        }
    }

    private static func speedLabel(_ value: Double) -> String {
        value == value.rounded() ? "\(Int(value))×" : "\(value)×"
    }
}
