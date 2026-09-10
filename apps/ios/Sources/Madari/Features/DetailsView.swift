import SwiftUI

/// A title's overview: backdrop, actions, progress and the season/episode browser.
///
/// The TV client had to split this into a fixed viewport plus dialogs. With a
/// scrolling screen the episodes and the synopsis live inline, and only the full
/// credits stay behind a sheet.
struct DetailsView: View {
    @EnvironmentObject private var model: AppModel
    let title: Title

    @State private var season: Int?
    @State private var showCredits = false

    private var state: AppState { model.state }

    /// The resolved metadata once the core has answered, otherwise the preview the
    /// catalog supplied. Rendering the preview immediately keeps the screen
    /// populated while `metadata` resolves.
    private var detail: Title {
        if let resolved = state.detail, resolved.identity == title.identity { return resolved }
        return title
    }

    private var videos: [JSONValue] { detail.videos }
    private var seasons: [Int] {
        videos.map { $0.integer("season") }.reduce(into: [Int]()) {
            if !$0.contains($1) { $0.append($1) }
        }
    }
    private var selectedSeason: Int {
        season ?? currentVideo?.integer("season") ?? seasons.first ?? 1
    }
    private var currentVideo: JSONValue? {
        videos.first { $0.text("id") == (state.videoId ?? detail.contentId) }
    }
    private var progressHistory: [JSONValue] {
        state.snapshot.objects("progress").filter { sameKey($0["key"], detail.key) }
    }
    private var watching: JSONValue? {
        progressHistory.last {
            $0.text("video_id") == (state.videoId ?? detail.contentId)
                && !$0.boolean("completed")
                && $0.number("position_ms") > 0
        }
    }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 20) {
                    header
                    actions
                    if let watching {
                        ProgressRow(position: watching.number("position_ms"), duration: watching.number("duration_ms"))
                    }
                    if !videos.isEmpty {
                        episodeBrowser
                    }
                    synopsis
                }
                .padding(.bottom, 32)
            }
            .navigationTitle(detail.name)
            .navigationBarTitleDisplayMode(.inline)
            .task { await model.open(title) }
            .sheet(isPresented: $showCredits) {
                CreditsSheet(title: detail)
            }
        }
    }

    private var header: some View {
        ZStack(alignment: .bottomLeading) {
            Artwork(url: detail.background)
                .frame(height: 240)
                .frame(maxWidth: .infinity)
                .clipped()
            LinearGradient(
                colors: [.clear, MadariColors.background.opacity(0.9), MadariColors.background],
                startPoint: .top, endPoint: .bottom
            )
            VStack(alignment: .leading, spacing: 8) {
                Text(detail.isSeries ? "SERIES" : "MOVIE")
                    .madariLabel().tracking(3).foregroundStyle(.white.opacity(0.75))
                Wordmark(title: detail)
                if !metadataLine.isEmpty {
                    Text(metadataLine).madariLabel().foregroundStyle(.white.opacity(0.85))
                }
            }
            .padding(20)
        }
    }

    private var metadataLine: String {
        [
            detail.releaseInfo,
            detail.runtime,
            detail.imdbRating.isEmpty ? "" : "IMDb \(detail.imdbRating)",
            seasons.isEmpty ? "" : "\(seasons.count) season\(seasons.count == 1 ? "" : "s")",
        ]
        .filter { !$0.isEmpty }
        .joined(separator: "   ·   ")
    }

    private var actions: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let label = episodeLabel {
                Text(label).madariLabel().foregroundStyle(MadariColors.muted)
            }
            HStack(spacing: 10) {
                Button {
                    guard !state.busy else { return }
                    model.push(.sources(detail, videoId: state.videoId ?? detail.contentId))
                } label: {
                    HStack(spacing: 8) {
                        GlyphIcon(glyph: .play, size: 14)
                        Text(state.busy ? "Loading title…" : (watching != nil ? "Resume watching" : "Watch now"))
                            .font(MadariFont.semibold(14))
                    }
                }
                .buttonStyle(ProminentButton())
                .disabled(state.busy)

                Button {
                    Task { await model.toggleSaved(detail) }
                } label: {
                    HStack(spacing: 7) {
                        GlyphIcon(glyph: model.isSaved(detail) ? .check : .plus, size: 13)
                        Text(model.isSaved(detail) ? "Saved" : "My list").font(MadariFont.medium(14))
                    }
                }
                .buttonStyle(QuietButton())

                Button {
                    showCredits = true
                } label: {
                    HStack(spacing: 7) {
                        GlyphIcon(glyph: .info, size: 13)
                        Text("Details").font(MadariFont.medium(14))
                    }
                }
                .buttonStyle(QuietButton())
            }
            .frame(maxWidth: 560)
        }
        .padding(.horizontal, 20)
    }

    private var episodeLabel: String? {
        guard let currentVideo else { return nil }
        return "Season \(currentVideo.integer("season")) · Episode \(currentVideo.integer("episode"))"
    }

    private var episodeBrowser: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Episodes & seasons").madariRowHeading().padding(.horizontal, 20)

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(seasons, id: \.self) { number in
                        Button {
                            season = number
                        } label: {
                            Text(number == 0 ? "Specials" : "Season \(number)")
                                .madariLabel()
                                .padding(.horizontal, 13)
                                .padding(.vertical, 8)
                                .background(
                                    number == selectedSeason ? MadariColors.accent : MadariColors.panel,
                                    in: Capsule()
                                )
                                .foregroundStyle(number == selectedSeason ? .white : .primary)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 20)
            }

            ForEach(videos.filter { $0.integer("season") == selectedSeason }, id: \.itemID) { episode in
                Button {
                    model.push(.sources(detail, videoId: episode.text("id")))
                } label: {
                    EpisodeRow(
                        episode: episode,
                        fallbackArtwork: detail.background,
                        progress: progressHistory.last { $0.text("video_id") == episode.text("id") }
                    )
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 20)
            }
        }
    }

    private var synopsis: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("About").madariRowHeading()
            Text(detail.description.isEmpty ? "No synopsis available." : detail.description)
                .madariBody()
                .foregroundStyle(MadariColors.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 20)
    }
}

/// The title wordmark: the addon's logo when it has one, the name otherwise.
///
/// Addon logos are frequently SVG, which UIKit cannot decode, so the name stays
/// visible until an image has actually decoded.
struct Wordmark: View {
    let title: Title

    @State private var logo: UIImage?

    var body: some View {
        ZStack(alignment: .leading) {
            Text(title.name)
                .font(MadariFont.bold(26))
                .foregroundStyle(.white)
                .lineLimit(2)
                .opacity(logo == nil ? 1 : 0)
            if let logo {
                Image(uiImage: logo)
                    .resizable()
                    .scaledToFit()
                    .frame(height: 60)
                    .frame(maxWidth: 300, alignment: .leading)
            }
        }
        .task(id: title.logo) {
            logo = await ImageCache.load(title.logo)
        }
    }
}

private struct ProgressRow: View {
    let position: Double
    let duration: Double

    var body: some View {
        HStack(spacing: 12) {
            ProgressBar(fraction: duration > 0 ? position / duration : 0)
                .frame(width: 160)
            Text("\(Int((max(duration - position, 0) + 59_999) / 60_000)) min left")
                .madariLabel()
                .foregroundStyle(MadariColors.muted)
        }
        .padding(.horizontal, 20)
    }
}

private struct EpisodeRow: View {
    let episode: JSONValue
    let fallbackArtwork: String
    let progress: JSONValue?

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            ZStack(alignment: .bottom) {
                Artwork(url: episode.text("thumbnail").nilIfEmpty ?? fallbackArtwork)
                    .frame(width: 150, height: 85)
                    .clipShape(RoundedRectangle(cornerRadius: 7))
                    .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 7))
                if let progress {
                    ProgressBar(fraction: progress.boolean("completed") ? 1 : progressFraction(progress))
                        .padding(.horizontal, 8)
                        .padding(.bottom, 6)
                }
            }
            VStack(alignment: .leading, spacing: 5) {
                Text("\(episode.integer("episode")). \(episodeTitle)")
                    .madariBody()
                    .lineLimit(2)
                let overview = episode.text("overview").nilIfEmpty
                    ?? episode.text("description").nilIfEmpty
                if let overview, !overview.isEmpty {
                    Text(overview)
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                        .lineLimit(3)
                }
                if let progress {
                    Text(progress.boolean("completed") ? "Watched" : "Continue watching")
                        .font(MadariFont.medium(12))
                        .foregroundStyle(progress.boolean("completed") ? MadariColors.muted : MadariColors.accent)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .background(MadariColors.panel.opacity(0.55), in: RoundedRectangle(cornerRadius: 10))
    }

    private var episodeTitle: String {
        for key in ["title", "name"] {
            if let value = episode.text(key).nilIfEmpty { return value }
        }
        return "Episode \(episode.integer("episode"))"
    }
}

/// Full synopsis and credits, which the TV client showed in a dialog.
private struct CreditsSheet: View {
    let title: Title

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(title.description.isEmpty ? "No synopsis available." : title.description)
                        .madariBody()
                    ForEach([("Cast", "cast"), ("Director", "director"), ("Genres", "genres")], id: \.0) { label, key in
                        let values = title.raw.strings(key)
                        if !values.isEmpty {
                            VStack(alignment: .leading, spacing: 5) {
                                Text(label).madariHeading()
                                Text(values.joined(separator: " · "))
                                    .madariBody()
                                    .foregroundStyle(MadariColors.muted)
                            }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(20)
            }
            .navigationTitle(title.name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Close") { dismiss() }
                }
            }
        }
    }
}
