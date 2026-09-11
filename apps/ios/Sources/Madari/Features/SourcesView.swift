import SwiftUI

/// Source selection, grouped by addon with a provenance filter.
///
/// The core keeps per-addon results separate so one failing addon does not hide the
/// others; this screen surfaces that grouping instead of flattening it.
struct SourcesView: View {
    @EnvironmentObject private var model: AppModel
    let title: Title
    let videoId: String

    @State private var filter: String?
    @State private var failed: String?

    private var state: AppState { model.state }

    private var sources: [Source] { state.sources ?? [] }
    private var visible: [Source] {
        guard let filter else { return sources }
        return sources.filter { $0.provider == filter }
    }
    private var groups: [(provider: String, name: String, count: Int)] {
        var order: [String] = []
        var names: [String: String] = [:]
        var counts: [String: Int] = [:]
        for source in sources {
            if counts[source.provider] == nil { order.append(source.provider) }
            names[source.provider] = source.name
            counts[source.provider, default: 0] += 1
        }
        return order.map { (provider: $0, name: names[$0] ?? "Addon", count: counts[$0] ?? 0) }
    }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text("Choose a source").madariHeading()
                        Text(title.name).madariBody().foregroundStyle(MadariColors.muted).lineLimit(1)
                    }
                    .padding(.horizontal, 20)

                    if !groups.isEmpty {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 8) {
                                FilterChip(label: "All addons · \(sources.count)", selected: filter == nil) { filter = nil }
                                ForEach(groups, id: \.provider) { group in
                                    FilterChip(label: "\(group.name) · \(group.count)", selected: filter == group.provider) {
                                        filter = group.provider
                                    }
                                }
                            }
                            .padding(.horizontal, 20)
                        }
                    }

                    Text(state.sourcesLoading
                         ? "Finding sources…"
                         : "\(visible.count) source\(visible.count == 1 ? "" : "s") available")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                        .padding(.horizontal, 20)

                    if let message = failed {
                        ErrorCard(message: message) {
                            failed = nil
                            Task { await model.loadSources(title, videoId: videoId) }
                        }
                        .padding(.horizontal, 20)
                    }

                    if state.sourcesLoading {
                        ProgressView().tint(MadariColors.accent)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 36)
                    } else if visible.isEmpty {
                        EmptyStateView(
                            title: "No sources found",
                            message: "Try another addon, or search again.",
                            actionTitle: "Try again",
                            action: { Task { await model.loadSources(title, videoId: videoId) } }
                        )
                    }

                    ForEach(Array(visible.enumerated()), id: \.offset) { _, source in
                        SourceRow(source: source) {
                            Task {
                                let ok = await model.play(title, videoId: videoId, source: source)
                                if !ok { failed = model.state.error }
                            }
                        }
                        .padding(.horizontal, 20)
                    }

                    if !state.notices.isEmpty {
                        NoticesView(notices: state.notices)
                    }
                }
                .padding(.vertical, 16)
                .padding(.bottom, 30)
            }
            .navigationTitle("Sources")
            .inlineNavigationTitle()
            .task { await model.loadSources(title, videoId: videoId) }
        }
    }
}

private struct FilterChip: View {
    let label: String
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .madariLabel()
                .padding(.horizontal, 13)
                .padding(.vertical, 8)
                .background(selected ? MadariColors.accent : MadariColors.panel, in: Capsule())
                .foregroundStyle(selected ? .white : .primary)
        }
        .buttonStyle(.plain)
    }
}

private struct SourceRow: View {
    let source: Source
    let onPlay: () -> Void

    var body: some View {
        Button(action: onPlay) {
            HStack(alignment: .top, spacing: 14) {
                GlyphIcon(glyph: .play, size: 18)
                    .foregroundStyle(MadariColors.accent)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 5) {
                    Text(source.displayName)
                        .madariBody()
                        .lineLimit(2)
                    if !source.detail.isEmpty {
                        Text(source.detail)
                            .madariLabel()
                            .foregroundStyle(MadariColors.muted)
                            .lineLimit(3)
                    }
                    Text(source.name).madariLabel().foregroundStyle(MadariColors.muted)
                }
                Spacer(minLength: 0)
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 10))
        }
        .buttonStyle(.plain)
    }
}
