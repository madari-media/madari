import SwiftUI

/// The playback state and transport the shared chrome drives.
///
/// `PlayerEngine` (AVFoundation) and `MpvEngine` (libmpv) both publish this already, so
/// the two backends differ only in which decoder produces the frames — not in how the
/// player looks or behaves.
@MainActor
protocol PlayerBackendModel: PlayerOptionsModel {
    var position: Double { get }
    var duration: Double { get }
    var playing: Bool { get }
    var buffering: Bool { get }
    var ended: Bool { get }
    var failure: String? { get }
    var positionMs: Double { get }
    var durationMs: Double { get }

    func play()
    func pause()
    func seek(to seconds: Double)
}

extension PlayerBackendModel {
    /// AVFoundation and libmpv spell the relative seek and the play/pause toggle
    /// differently; the chrome should not have to care which engine it is driving.
    func seek(by delta: Double) {
        seek(to: max(0, position + delta))
    }

    func togglePlay() {
        playing ? pause() : play()
    }
}

extension PlayerEngine: PlayerBackendModel {}
extension MpvEngine: PlayerBackendModel {}

/// Everything the player draws around the video surface.
///
/// Both backends render this, so the AVPlayer and libmpv screens are the same UI driven
/// by different engines: the same top bar, the same scrubber and transport, the same
/// options sheet and the same end-of-playback panel.
struct PlayerChrome<Engine: PlayerBackendModel>: View {
    let playback: Playback
    @ObservedObject var engine: Engine
    let sources: [Source]
    let torrentStats: JSONValue?
    @Binding var speed: Double
    @Binding var pictureSize: Int

    var onClose: () -> Void
    var onSwitchSource: (Source) -> Void
    var onSwitchEpisode: (String?) -> Void
    var onRestart: () -> Void
    /// The other backend, when it could play this source too, so a source can be handed
    /// across without leaving the player.
    var otherBackend: (label: String, action: () -> Void)?

    @EnvironmentObject private var model: AppModel
    @State private var controlsVisible = true
    @State private var scrubbing = false
    @State private var scrubTarget: Double = 0
    @State private var optionsOpen = false
    @State private var hideControls: Task<Void, Never>?

    var body: some View {
        ZStack {
            // Always present, so the controls can be brought back after they hide
            // themselves. Attaching the tap to the controls meant the gesture disappeared
            // along with them, leaving no way to reopen the HUD.
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { toggleControls() }

            if engine.failure != nil || engine.ended {
                recoveryPanel
            } else if controlsVisible {
                controls
            }

            if controlsVisible, let torrentStats, engine.failure == nil, !engine.ended {
                TorrentBadge(stats: torrentStats)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    // The close button occupies the top-trailing corner too, and this badge is
                    // drawn after the controls, so without clearing the top bar it sat over
                    // the button and swallowed the taps.
                    .padding(.top, 64)
                    .padding(.trailing, 16)
                    .safeAreaPadding(.top)
                    .allowsHitTesting(false)
            }

            // Before the first frame there is nothing to see, so show that something is
            // happening instead of a dead black screen.
            if engine.failure == nil, !engine.ended, engine.position <= 0, engine.duration <= 0 {
                VStack(spacing: 12) {
                    ProgressView().tint(.white).scaleEffect(1.2)
                    Text(engine.buffering ? "Buffering…" : "Opening…")
                        .madariLabel()
                        .foregroundStyle(.white.opacity(0.8))
                }
            }
        }
        // The chrome itself covers the whole screen, so its tap layer and background bands
        // reach the edges instead of stopping at the safe area. The bars below put their
        // content back inside the safe area, so nothing hides under the notch or the home
        // indicator.
        .ignoresSafeArea()
        .animation(.easeInOut(duration: 0.2), value: controlsVisible)
        .onAppear { showControls() }
        .sheet(isPresented: $optionsOpen) {
            PlayerOptionsSheet(
                playback: playback,
                engine: engine,
                speed: $speed,
                pictureSize: $pictureSize,
                sources: sources,
                onSwitchSource: onSwitchSource,
                onSwitchEpisode: onSwitchEpisode
            )
            .environmentObject(model)
        }
    }

    // MARK: - Controls

    private var controls: some View {
        VStack {
            topBar
            Spacer()
            transport
        }
        .allowsHitTesting(true)
    }

    private var topBar: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 4) {
                Text(playback.title.name)
                    .font(MadariFont.semibold(17))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(episodeCaption)
                    .madariLabel()
                    .foregroundStyle(.white.opacity(0.75))
                    .lineLimit(1)
            }
            Spacer()
            Button(action: onClose) {
                GlyphIcon(glyph: .close, size: 16)
                    .padding(9)
                    .background(.black.opacity(0.45), in: Circle())
                    .foregroundStyle(.white)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Close player")
        }
        .padding(18)
        .safeAreaPadding(.top)
        .background(
            LinearGradient(colors: [.black.opacity(0.8), .clear], startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea(edges: .top)
                .allowsHitTesting(false)
        )
    }

    private var transport: some View {
        VStack(spacing: 10) {
            HStack(spacing: 6) {
                Text(Self.timeLabel(scrubbing ? scrubTarget : engine.position))
                    .madariLabel()
                    .foregroundStyle(.white)
                Text("/").madariLabel().foregroundStyle(.white.opacity(0.5))
                Text(Self.timeLabel(engine.duration)).madariLabel().foregroundStyle(.white.opacity(0.6))
                Spacer()
                if engine.buffering {
                    ProgressView().tint(.white).scaleEffect(0.7)
                }
                Text(Self.speedLabel(speed)).madariLabel().foregroundStyle(.white.opacity(0.6))
            }

            Slider(
                value: Binding(
                    get: { scrubbing ? scrubTarget : engine.position },
                    set: { scrubTarget = $0 }
                ),
                in: 0...max(engine.duration, 1),
                onEditingChanged: { editing in
                    if editing {
                        scrubbing = true
                        scrubTarget = engine.position
                    } else {
                        engine.seek(to: scrubTarget)
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
                .accessibilityLabel("Back 10 seconds")

                Button {
                    engine.togglePlay()
                    showControls()
                } label: {
                    GlyphIcon(glyph: engine.playing ? .pause : .play, size: 26)
                }
                .accessibilityLabel(engine.playing ? "Pause" : "Play")

                Button {
                    engine.seek(by: 10)
                    showControls()
                } label: {
                    GlyphIcon(glyph: .forward, size: 20)
                }
                .accessibilityLabel("Forward 10 seconds")

                Button {
                    engine.toggleMute()
                    showControls()
                } label: {
                    GlyphIcon(glyph: .audio, size: 20)
                        .foregroundStyle(engine.muted ? MadariColors.accent : .white)
                }
                .accessibilityLabel(engine.muted ? "Unmute" : "Mute")

                Spacer()

                if playback.previousVideo != nil {
                    Button("Previous") { onSwitchEpisode(playback.previousVideo) }
                        .buttonStyle(.plain)
                        .font(MadariFont.regular(13))
                }
                if let next = playback.nextVideo {
                    Button("Next") { onSwitchEpisode(next) }
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
        .safeAreaPadding(.bottom)
        .background(
            LinearGradient(colors: [.clear, .black.opacity(0.9)], startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea(edges: .bottom)
                .allowsHitTesting(false)
        )
    }

    private var recoveryPanel: some View {
        VStack(spacing: 16) {
            Text(engine.ended ? "You've reached the end" : "Playback interrupted")
                .madariHeading()
            if let failure = engine.failure, !engine.ended {
                Text(failure)
                    .madariBody()
                    .foregroundStyle(MadariColors.muted)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: 10) {
                Button(engine.ended ? "Play again" : "Retry", action: onRestart)
                    .buttonStyle(ProminentButton())
                if engine.ended, let next = playback.nextVideo {
                    Button("Next episode") { onSwitchEpisode(next) }.buttonStyle(QuietButton())
                }
                if let otherBackend {
                    Button(otherBackend.label, action: otherBackend.action).buttonStyle(QuietButton())
                }
                Button("Back to sources", action: onClose).buttonStyle(QuietButton())
            }
        }
        .padding(24)
        .background(MadariColors.panel.opacity(0.95), in: RoundedRectangle(cornerRadius: 18))
        .padding(.horizontal, 28)
    }

    // MARK: - Helpers

    private var episodeCaption: String {
        guard let episode = playback.episode else { return playback.source.displayName }
        let title = episode.text("title").nilIfEmpty ?? ""
        let prefix = "Season \(episode.integer("season")) · Episode \(episode.integer("episode"))"
        return title.isEmpty ? prefix : "\(prefix)  \(title)"
    }

    private func toggleControls() {
        controlsVisible.toggle()
        guard controlsVisible else { return }
        showControls()
    }

    /// Keeps the controls up while paused, and never hides them mid-scrub or with the
    /// options sheet open, matching the TV client.
    private func showControls() {
        controlsVisible = true
        hideControls?.cancel()
        hideControls = Task {
            try? await Task.sleep(for: .seconds(5))
            // Only "has playback started" is required here. Waiting for a stream to stop
            // buffering never happened on a torrent, and every state change used to re-arm
            // this timer, so the controls stayed up for good.
            guard !Task.isCancelled, engine.playing, engine.position > 0,
                  !optionsOpen, !scrubbing,
                  engine.failure == nil, !engine.ended else { return }
            controlsVisible = false
        }
    }

    private static func timeLabel(_ seconds: Double) -> String {
        guard seconds.isFinite, seconds >= 0 else { return "0:00" }
        let total = Int(seconds.rounded())
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        let secs = total % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, secs)
            : String(format: "%d:%02d", minutes, secs)
    }

    private static func speedLabel(_ value: Double) -> String {
        value == 1 ? "1×" : String(format: "%g×", value)
    }
}
