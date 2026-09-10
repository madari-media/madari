import SwiftUI

/// The libmpv player.
///
/// AVFoundation only decodes MP4/MOV/M4V/HLS, so everything in
/// `MediaFormat.mpvOnlyExtensions` — Matroska, WebM, AVI and the rest — is played here
/// instead. It is also where playback lands when AVPlayer fails on a source it was
/// supposed to handle. The interface is `PlayerChrome`, shared with the AVPlayer screen,
/// so the two backends differ only in which decoder produces the frames.
struct MpvPlayerView: View {
    let playback: Playback
    /// Offered when the source is one AVPlayer could decode, so the user can switch back
    /// if libmpv has trouble with it.
    var onUseAVPlayer: (() -> Void)?

    @EnvironmentObject private var model: AppModel
    @StateObject private var engine: MpvEngine

    @State private var speed = 1.0
    @State private var pictureSize = 0
    @State private var allSources: [Source] = []
    @State private var torrentStats: JSONValue?

    init(playback: Playback, onUseAVPlayer: (() -> Void)? = nil) {
        self.playback = playback
        self.onUseAVPlayer = onUseAVPlayer
        _engine = StateObject(wrappedValue: MpvEngine(
            uri: playback.uri,
            headers: playback.requestHeaders,
            startSeconds: Double(playback.resumeMs) / 1000
        ))
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            MpvSurface(engine: engine)
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
                otherBackend: onUseAVPlayer.map { ("Use the iOS player", $0) }
            )
        }
        .statusBarHidden()
        .task {
            // Claims playback, so the AVPlayer backend stands down if it was playing.
            PlaybackCoordinator.shared.activate(engine)
            speed = model.defaultSpeed
            pictureSize = model.pictureSize
            engine.start()
            engine.setSpeed(speed)
            engine.setPictureSize(pictureSize)
        }
        .task { await trackTorrent() }
        .task { allSources = await model.playerSources(playback.title, videoId: playback.videoId) }
        .onChange(of: pictureSize) { _, value in
            engine.setPictureSize(value)
        }
        .onDisappear {
            PlaybackCoordinator.shared.deactivate(engine)
            engine.stop()
            Task {
                await model.saveProgress(
                    playback,
                    positionMs: engine.positionMs,
                    durationMs: engine.durationMs,
                    completed: engine.ended
                )
            }
        }
    }

    // MARK: - Actions

    private func trackTorrent() async {
        guard playback.isTorrent else { return }
        while !Task.isCancelled {
            torrentStats = await model.torrentStats(playback.token)
            try? await Task.sleep(for: .milliseconds(1500))
        }
    }

    private func restart() {
        engine.seek(to: 0)
        engine.play()
    }

    private func switchSource(_ source: Source) {
        let position = engine.positionMs
        let duration = engine.durationMs
        engine.stop()
        Task {
            await model.replacePlayback(
                playback,
                videoId: playback.videoId,
                source: source,
                positionMs: position,
                durationMs: duration,
                completed: false
            )
        }
    }

    private func switchEpisode(_ videoId: String?) {
        guard let videoId else { return }
        let position = engine.positionMs
        let duration = engine.durationMs
        let completed = engine.ended
        engine.stop()
        Task {
            await model.replacePlayback(
                playback,
                videoId: videoId,
                source: playback.source,
                positionMs: position,
                durationMs: duration,
                completed: completed
            )
        }
    }

    private func close() {
        let completed = engine.ended
        let positionMs = engine.positionMs
        let durationMs = engine.durationMs
        engine.stop()
        model.state.playback = nil
        Task {
            await model.saveProgress(playback, positionMs: positionMs, durationMs: durationMs, completed: completed)
            await model.closePlayer(next: false)
        }
    }
}
