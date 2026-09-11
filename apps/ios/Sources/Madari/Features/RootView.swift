import SwiftUI

/// Root routing: the profile picker until a profile is unlocked, then the tabs.
///
/// The TV client also gated on the player and a full-screen loading surface; here the
/// player is a `fullScreenCover` and the first-load surface is per screen.
struct RootView: View {
    @EnvironmentObject private var model: AppModel

    /// Set only when the user moves a source to the other backend by hand. Cleared when
    /// playback closes, so the next source is routed by its container again.
    @State private var forcedBackend: PlayerBackend?

    var body: some View {
        ZStack {
            MadariColors.background.ignoresSafeArea()
            if model.state.profile == nil {
                ProfilesView()
            } else {
                TabsView()
            }
            if model.state.resumingTitle != nil {
                ResumingBanner()
            }
        }
        .madariFullScreenCover(item: playbackBinding) { playback in
            player(for: playback)
                .environmentObject(model)
        }
        .onChange(of: model.state.playback?.id) { _, id in
            if id == nil { forcedBackend = nil }
        }
        .alert(
            "Something went wrong",
            isPresented: Binding(
                get: { model.state.error != nil },
                set: { if !$0 { model.dismissError() } }
            )
        ) {
            Button("OK") { model.dismissError() }
        } message: {
            Text(model.state.error ?? "")
        }
        .task { await model.start() }
    }

    private var playbackBinding: Binding<Playback?> {
        Binding(get: { model.state.playback }, set: { model.state.playback = $0 })
    }

    /// AVPlayer handles what AVFoundation can decode and libmpv handles the rest, but
    /// either one can hand the same source to the other when it cannot cope with it.
    @ViewBuilder
    private func player(for playback: Playback) -> some View {
        let mpvOnly = MediaFormat.requiresMpv(playback)
        if forcedBackend == .mpv || (forcedBackend == nil && mpvOnly) {
            MpvPlayerView(
                playback: playback,
                // Pointless for a container AVPlayer cannot open in the first place.
                onUseAVPlayer: mpvOnly ? nil : { forcedBackend = .avPlayer }
            )
        } else {
            PlayerView(playback: playback, onUseMpv: { forcedBackend = .mpv })
        }
    }
}

/// Which decoder is driving playback.
private enum PlayerBackend {
    case avPlayer
    case mpv
}

/// The five tab roots. Each owns a navigation stack so returning to a tab restores
/// where the user was, which the single-state TV client could not do.
struct TabsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        TabView(selection: $model.tab) {
            ForEach(Tab.allCases) { tab in
                NavigationStack(path: model.path(tab)) {
                    root(for: tab)
                        .navigationDestination(for: Route.self) { route in
                            destination(route)
                        }
                }
                .tabItem { Label(tab.rawValue, systemImage: tab.glyph.rawValue) }
                .tag(tab)
            }
        }
        // One indicator for every asynchronous action — catalog queries, addon
        // installs, saves, Trakt — so no screen has to invent its own.
        .overlay(alignment: .bottom) {
            if model.state.busy {
                BusyIndicator()
                    .padding(.bottom, 74)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: model.state.busy)
    }

    @ViewBuilder
    private func root(for tab: Tab) -> some View {
        switch tab {
        case .home: HomeView()
        case .search: SearchView()
        case .explore: ExploreView()
        case .library: LibraryView()
        case .settings: SettingsView()
        }
    }

    @ViewBuilder
    private func destination(_ route: Route) -> some View {
        switch route {
        case .details(let title):
            DetailsView(title: title)
        case .sources(let title, let videoId):
            SourcesView(title: title, videoId: videoId)
        case .catalog(let catalog):
            CatalogView(catalog: catalog)
        case .calendar:
            CalendarView()
        case .torrents:
            TorrentsView()
        }
    }
}

/// Shown while a Continue watching card resolves its next episode.
struct ResumingBanner: View {
    var body: some View {
        VStack {
            Spacer()
            BusyIndicator(label: "Resuming…")
                .padding(.bottom, 28)
        }
        .transition(.opacity)
    }
}

/// The shared progress chip for any operation that takes long enough to notice.
struct BusyIndicator: View {
    var label = "Loading…"

    var body: some View {
        HStack(spacing: 10) {
            ProgressView().tint(.white)
            Text(label).madariBody()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .background(MadariColors.panel, in: Capsule())
        .overlay(Capsule().stroke(Color.white.opacity(0.08), lineWidth: 1))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(label)
    }
}
