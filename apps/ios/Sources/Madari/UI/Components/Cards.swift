import SwiftUI

/// Artwork, through the shared loader described in `RemoteImage.swift`.
///
/// Addons sometimes publish SVG logos, which the platform image decoder cannot decode; those keep the
/// panel placeholder rather than a broken frame, which is also what the TV client
/// does when artwork fails.
struct Artwork: View {
    let url: String
    var contentMode: ContentMode = .fill
    /// Spinner size relative to the default; posters and backdrops pass 1.
    var progressScale: CGFloat = 1

    var body: some View {
        RemoteImage(url: url, contentMode: contentMode, progressScale: progressScale)
    }
}

/// A title card. Tapping opens the title; the long press adds or removes it from
/// My list, which is how the TV client's "hold OK to edit" behaves on a phone.
struct PosterCard: View {
    let title: Title
    var progress: Double = 0
    let onOpen: () -> Void
    var onToggleSaved: (() -> Void)?

    var body: some View {
        Button(action: onOpen) {
            VStack(alignment: .leading, spacing: 8) {
                ZStack(alignment: .bottom) {
                    Artwork(url: title.poster)
                        .frame(width: 128, height: 188)
                        .clipShape(RoundedRectangle(cornerRadius: 9))
                        .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 9))
                    if progress > 0 {
                        ProgressBar(fraction: progress)
                            .padding(.horizontal, 8)
                            .padding(.bottom, 7)
                    }
                }
                Text(title.name)
                    .madariLabel()
                    .foregroundStyle(MadariColors.muted)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .frame(width: 128, alignment: .leading)
            }
        }
        .buttonStyle(.plain)
        .contextMenu {
            if let onToggleSaved {
                Button {
                    onToggleSaved()
                } label: {
                    Label("My list", systemImage: Glyph.library.rawValue)
                }
            }
        }
        .accessibilityLabel(title.name)
    }
}

/// A thin progress indicator shared by posters and episode rows.
struct ProgressBar: View {
    let fraction: Double

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.white.opacity(0.25))
                Capsule()
                    .fill(MadariColors.accent)
                    .frame(width: geometry.size.width * min(max(fraction, 0), 1))
            }
        }
        .frame(height: 3)
    }
}

/// A horizontally scrolling catalog row.
struct PosterRow: View {
    let shelf: Shelf
    let onOpen: (Title) -> Void
    var isSaved: ((Title) -> Bool)?
    var onToggleSaved: ((Title) -> Void)?
    var onMore: (() -> Void)?
    /// Progress fractions by title identity, for Continue watching and My list.
    var progress: [String: Double] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(shelf.heading)
                .madariRowHeading()
                .padding(.horizontal, 20)
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: 14) {
                    ForEach(shelf.titles) { title in
                        PosterCard(
                            title: title,
                            progress: progress[title.identity] ?? 0,
                            onOpen: { onOpen(title) },
                            onToggleSaved: onToggleSaved.map { toggle in { toggle(title) } }
                        )
                    }
                    if shelf.more, let onMore {
                        Button(action: onMore) {
                            VStack(spacing: 8) {
                                GlyphIcon(glyph: .plus, size: 24)
                                Text("More").madariLabel()
                            }
                            .frame(width: 100, height: 188)
                            .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 9))
                            .foregroundStyle(MadariColors.muted)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 2)
            }
        }
    }
}

/// The Home hero. The TV client used a fixed 290dp card with a focus ring; on iOS it
/// is a full-width backdrop that opens the title on tap.
struct HeroCard: View {
    let title: Title
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            ZStack(alignment: .bottomLeading) {
                Artwork(url: title.background)
                    .frame(height: 260)
                    .frame(maxWidth: .infinity)
                    .clipped()
                LinearGradient(
                    colors: [.clear, MadariColors.background.opacity(0.85), MadariColors.background],
                    startPoint: .top, endPoint: .bottom
                )
                LinearGradient(
                    colors: [MadariColors.background.opacity(0.7), .clear],
                    startPoint: .leading, endPoint: .trailing
                )
                VStack(alignment: .leading, spacing: 9) {
                    Text(title.isSeries ? "SERIES" : "MOVIE")
                        .madariLabel()
                        .tracking(3)
                        .foregroundStyle(.white.opacity(0.75))
                    Text(title.name)
                        .font(MadariFont.bold(28))
                        .foregroundStyle(.white)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                    let meta = [title.releaseInfo, title.genres.prefix(2).joined(separator: " · ")]
                        .filter { !$0.isEmpty }
                        .joined(separator: "   •   ")
                    if !meta.isEmpty {
                        Text(meta).madariLabel().foregroundStyle(.white.opacity(0.8))
                    }
                }
                .padding(20)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title.name)
    }
}

/// A Continue watching card: artwork, the next episode the core chose and a bar for
/// the position of *that* video.
struct ContinueCard: View {
    let entry: ContinueEntry
    let snapshot: JSONValue
    let onPlay: () -> Void

    var body: some View {
        let episode = entry.episode
        let artwork = episode?.text("thumbnail").nilIfEmpty
            ?? entry.meta.text("background").nilIfEmpty
            ?? entry.title.background
        let target = episode?.text("id") ?? history.last?.text("video_id")
        let progress = history.last { $0.text("video_id") == target }
        let completed = progress?.boolean("completed") ?? false

        Button(action: onPlay) {
            VStack(alignment: .leading, spacing: 10) {
                ZStack(alignment: .bottomLeading) {
                    Artwork(url: artwork)
                        .frame(width: 240, height: 135)
                        .clipShape(RoundedRectangle(cornerRadius: 9))
                        .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 9))
                    LinearGradient(
                        colors: [.clear, .black.opacity(0.85)],
                        startPoint: .center, endPoint: .bottom
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 9))
                    VStack(alignment: .leading, spacing: 3) {
                        Text(entry.title.name)
                            .madariLabel()
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if let episode {
                            Text("S\(episode.integer("season")) · E\(episode.integer("episode"))  \(episode.text("title"))")
                                .font(MadariFont.regular(11))
                                .foregroundStyle(.white.opacity(0.8))
                                .lineLimit(1)
                        }
                        Text(status(progress: progress, completed: completed))
                            .font(MadariFont.medium(11))
                            .foregroundStyle(.white)
                    }
                    .padding(10)
                }
                .overlay(alignment: .bottom) {
                    if let progress, !completed {
                        ProgressBar(fraction: progressFraction(progress))
                            .padding(.horizontal, 10)
                            .padding(.bottom, 6)
                    }
                }
            }
            .frame(width: 240)
        }
        .buttonStyle(.plain)
    }

    private var history: [JSONValue] {
        snapshot.objects("progress").filter { sameKey($0["key"], entry.title.key) }
    }

    private func status(progress: JSONValue?, completed: Bool) -> String {
        if let progress, !completed, progress.number("position_ms") > 0 {
            return "Resume · \(minutesLeft(progress)) min left"
        }
        if entry.title.isSeries, entry.episode != nil || completed { return "Play next episode" }
        return entry.title.isSeries ? "Continue series" : "Play movie"
    }
}

/// The startup surface. The TV client holds focus here until real catalog items
/// arrive; on iOS it simply occupies the screen while the first snapshot loads.
struct LoadingView: View {
    var message = "Loading your home"

    @State private var sweep = false

    var body: some View {
        VStack(spacing: 22) {
            BrandLogo(size: 76)
            Text("MADARI")
                .font(MadariFont.medium(26))
                .tracking(7)
                .foregroundStyle(.white)
            ZStack(alignment: .leading) {
                Capsule().fill(Color.white.opacity(0.09))
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: [.clear, .white.opacity(0.85), .clear],
                            startPoint: .leading, endPoint: .trailing
                        )
                    )
                    .frame(width: 58)
                    .offset(x: sweep ? 86 : -58)
            }
            .frame(width: 144, height: 2)
            .clipped()
            Text(message).madariLabel().foregroundStyle(MadariColors.muted)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(MadariColors.background)
        .onAppear {
            withAnimation(.linear(duration: 1.8).repeatForever(autoreverses: false)) { sweep = true }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Loading")
    }
}

/// Notices from addons that failed while others succeeded.
struct NoticesView: View {
    let notices: [String]
    var retry: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(notices, id: \.self) { notice in
                Text(notice).madariLabel().foregroundStyle(MadariColors.muted)
            }
            if let retry {
                Button("Retry", action: retry)
                    .buttonStyle(QuietButton())
                    .frame(width: 140)
            }
        }
        .padding(.horizontal, 20)
    }
}

/// Empty-state block used by the catalog, search and library screens.
struct EmptyStateView: View {
    let title: String
    let message: String
    var actionTitle: String?
    var action: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).madariHeading()
            Text(message).madariBody().foregroundStyle(MadariColors.muted)
            if let actionTitle, let action {
                Button(actionTitle, action: action)
                    .buttonStyle(ProminentButton())
                    .frame(width: 200)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
    }
}
