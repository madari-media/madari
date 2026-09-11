import SwiftUI

/// Home: hero, Continue watching and one row per addon catalog.
///
/// The TV client kept the hero fixed once chosen so a late catalog response could not
/// swap it. The same rule applies here: the hero is captured from the first shelf and
/// only replaced while it is still empty.
struct HomeView: View {
    @EnvironmentObject private var model: AppModel

    @State private var hero: Title?
    @State private var profileSheet = false

    private var state: AppState { model.state }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    if let hero {
                        HeroCard(title: hero) { model.push(.details(hero)) }
                    } else if state.busy {
                        HeroPlaceholder()
                    } else {
                        WelcomePanel(
                            hasAddons: !state.addons.isEmpty,
                            onManage: { model.tab = .settings },
                            onRefresh: { Task { await model.refresh() } }
                        )
                    }

                    TorrentsRow { model.push(.torrents) }

                    if !continueList.isEmpty {
                        ContinueSection(titles: continueList)
                    }

                    ForEach(state.homeShelves.filter { !$0.titles.isEmpty }) { shelf in
                        PosterRow(
                            shelf: shelf,
                            onOpen: { model.push(.details($0)) },
                            isSaved: { model.isSaved($0) },
                            onToggleSaved: { title in Task { await model.toggleSaved(title) } },
                            onMore: { Task { await model.loadMore(shelf, into: .home) } }
                        )
                    }

                    if !state.notices.isEmpty {
                        NoticesView(notices: state.notices) { Task { await model.refresh() } }
                    }
                }
                .padding(.bottom, 30)
            }
            .refreshable { await model.refresh() }
            .toolbar {
                ToolbarItem(placement: .madariLeading) {
                    Button {
                        profileSheet = true
                    } label: {
                        HStack(spacing: 7) {
                            GlyphIcon(glyph: .profile, size: 15)
                            Text(state.profileName).madariLabel()
                        }
                    }
                }
                ToolbarItem(placement: .madariTrailing) {
                    Button {
                        Task { await model.refresh() }
                    } label: {
                        GlyphIcon(glyph: .refresh)
                    }
                }
            }
            .sheet(isPresented: $profileSheet) {
                ProfileSwitcher()
                    .environmentObject(model)
            }
            .task { await captureHero() }
            .task(id: heroIdentity) { await captureHero() }
            .task(id: continueIdentity) {
                await model.resolveContinue(continueList)
            }
        }
    }

    private var heroIdentity: String {
        state.homeShelves.first?.titles.first?.identity ?? ""
    }

    private var continueList: [Title] { continueTitles(state.snapshot) }

    private var continueIdentity: String {
        continueList.map(\.identity).joined(separator: "|")
    }

    private func captureHero() async {
        if hero == nil {
            hero = state.homeShelves.first?.titles.first ?? continueList.first
        }
        if hero == nil, state.homeShelves.isEmpty, state.busy {
            return
        }
    }
}

private struct HeroPlaceholder: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Your next story awaits").madariHeading()
            Text("Loading your catalogs…").madariBody().foregroundStyle(MadariColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(24)
    }
}

private struct WelcomePanel: View {
    let hasAddons: Bool
    let onManage: () -> Void
    let onRefresh: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("A world of stories. Your way.").madariHeading()
            Text(hasAddons
                 ? "Your catalogs will appear here. Check enabled addons or try refreshing."
                 : "Install an addon to bring your movies and series to Madari.")
                .madariBody()
                .foregroundStyle(MadariColors.muted)
            HStack(spacing: 12) {
                Button("Manage addons", action: onManage).buttonStyle(ProminentButton())
                Button("Refresh", action: onRefresh).buttonStyle(QuietButton())
            }
            .frame(maxWidth: 420)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(24)
    }
}

/// Continue watching. Metadata is resolved once per snapshot in a single cached
/// batch, and a series with no next episode is finished and stays hidden.
private struct ContinueSection: View {
    @EnvironmentObject private var model: AppModel
    let titles: [Title]

    var body: some View {
        let visible = titles.filter { model.state.continueEntries[$0.identity]?.isFinished != true }
        if !visible.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                Text("Continue watching").madariRowHeading().padding(.horizontal, 20)
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHStack(spacing: 14) {
                        ForEach(visible) { title in
                            if let entry = model.state.continueEntries[title.identity] {
                                ContinueCard(entry: entry, snapshot: model.state.snapshot) {
                                    Task { await model.resumeContinue(title, knownVideoId: entry.episode?.text("id")) }
                                }
                            } else {
                                ContinuePlaceholder(title: title)
                            }
                        }
                    }
                    .padding(.horizontal, 20)
                }
            }
        }
    }
}

private struct ContinuePlaceholder: View {
    let title: Title

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Artwork(url: title.background)
                .frame(width: 240, height: 135)
                .clipShape(RoundedRectangle(cornerRadius: 9))
            Text(title.name).madariLabel().foregroundStyle(MadariColors.muted).lineLimit(1)
        }
        .frame(width: 240)
    }
}

/// Profile actions available while watching, including switching profiles.
struct ProfileSwitcher: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    @State private var pin = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(model.state.profileName).madariHeading()
                    Text(model.state.isKidsProfile
                         ? "Kids profile · protected by the guardian's PIN."
                         : "Create and edit profiles from the profile picker.")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                }
                if model.state.isKidsProfile {
                    Section("Guardian PIN") {
                        MadariField(
                            label: "Guardian PIN", prompt: "Required to switch",
                            text: $pin, secret: true, keyboard: .numberPad
                        )
                    }
                }
                Section {
                    Button("Switch profile") {
                        dismiss()
                        Task { await model.leave(pin: pin) }
                    }
                    .buttonStyle(ProminentButton())
                }
            }
            .navigationTitle("Profile")
            .inlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
        }
    }
}
