import SwiftUI

/// My list: everything the user saved, from the core's snapshot.
struct LibraryView: View {
    @EnvironmentObject private var model: AppModel

    @State private var path: [Route] = []
    private let columns = [GridItem(.adaptive(minimum: 128), spacing: 16)]

    private var titles: [Title] { savedTitles(model.state.snapshot) }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text("My list").madariHeading()
                        Text("The stories you're saving for later")
                            .madariBody().foregroundStyle(MadariColors.muted)
                    }
                    .padding(.horizontal, 20)

                    if titles.isEmpty {
                        EmptyStateView(
                            title: "Nothing saved yet",
                            message: "Open a title and choose Add to My list, or long press a poster anywhere in the app."
                        )
                    } else {
                        LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
                            ForEach(titles) { title in
                                PosterCard(
                                    title: title,
                                    progress: progress(for: title),
                                    onOpen: { model.push(.details(title)) },
                                    onToggleSaved: { Task { await model.toggleSaved(title) } }
                                )
                            }
                        }
                        .padding(.horizontal, 20)
                    }
                }
                .padding(.vertical, 16)
                .padding(.bottom, 30)
            }
            .refreshable { await model.refresh() }
            .toolbar {
                ToolbarItem(placement: .madariTrailing) {
                    // The TV client has Calendar as its own destination. It reports on
                    // saved titles, so it is reached from here.
                    Button {
                        model.push(.calendar)
                    } label: {
                        GlyphIcon(glyph: .calendar)
                    }
                }
            }
        }
    }

    /// Resume position for the video the core picked, if any.
    private func progress(for title: Title) -> Double {
        let history = model.state.snapshot.objects("progress").filter { sameKey($0["key"], title.key) }
        guard let latest = history.last else { return 0 }
        return latest.boolean("completed") ? 1 : progressFraction(latest)
    }
}

/// Calendar: releases this month for saved titles, using the addon-declared
/// calendar support the core reports.
struct CalendarView: View {
    @EnvironmentObject private var model: AppModel

    @State private var month = CalendarView.currentMonth

    private static var currentMonth: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM"
        formatter.locale = Locale(identifier: "en_US_POSIX")
        return formatter.string(from: Date())
    }

    private var state: AppState { model.state }

    /// Episodes whose release date falls in the shown month.
    private var entries: [(title: Title, episode: JSONValue)] {
        state.calendar.objects("titles").flatMap { entry -> [(Title, JSONValue)] in
            guard let key = entry["key"], let meta = entry["meta"] else { return [] }
            let title = Title(provider: key.text("installation_id"), raw: meta)
            return title.videos
                .filter { $0.text("released").hasPrefix(month) }
                .map { (title, $0) }
        }
        .sorted { $0.1.text("released") < $1.1.text("released") }
    }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    Text("Calendar").madariHeading().padding(.horizontal, 20)

                    HStack(spacing: 12) {
                        Button {
                            shift(-1)
                        } label: {
                            GlyphIcon(glyph: .previous, size: 14)
                        }
                        .buttonStyle(QuietButton())
                        .frame(width: 52)

                        Text(monthLabel)
                            .madariBody()
                            .frame(width: 170)

                        Button {
                            shift(1)
                        } label: {
                            GlyphIcon(glyph: .next, size: 14)
                        }
                        .buttonStyle(QuietButton())
                        .frame(width: 52)

                        Button("Today") { month = Self.currentMonth }
                            .buttonStyle(QuietButton())
                            .frame(width: 90)
                    }
                    .padding(.horizontal, 20)

                    if !state.calendar.boolean("supported") {
                        EmptyStateView(
                            title: "No calendar support",
                            message: "Install an addon that declares calendar support, then save series to My list."
                        )
                    } else if entries.isEmpty {
                        EmptyStateView(
                            title: "Nothing this month",
                            message: "No releases for your saved titles in \(monthLabel)."
                        )
                    }

                    ForEach(entries, id: \.episode.itemID) { entry in
                        Button {
                            model.push(.details(entry.title))
                        } label: {
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text("\(entry.title.name) · S\(entry.episode.integer("season")) E\(entry.episode.integer("episode"))")
                                        .madariBody()
                                    Text(entry.episode.text("title").nilIfEmpty ?? entry.episode.text("id"))
                                        .madariLabel().foregroundStyle(MadariColors.muted)
                                }
                                Spacer()
                                Text(String(entry.episode.text("released").prefix(10)))
                                    .madariLabel().foregroundStyle(MadariColors.muted)
                            }
                            .padding(14)
                            .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 10))
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal, 20)
                    }
                }
                .padding(.vertical, 16)
                .padding(.bottom, 30)
            }
            .navigationTitle("Calendar")
            .inlineNavigationTitle()
            .task { await model.loadCalendar() }
            .refreshable { await model.loadCalendar() }
        }
    }

    private var monthLabel: String {
        let parser = DateFormatter()
        parser.dateFormat = "yyyy-MM"
        parser.locale = Locale(identifier: "en_US_POSIX")
        guard let date = parser.date(from: month) else { return month }
        let display = DateFormatter()
        display.dateFormat = "MMMM yyyy"
        return display.string(from: date)
    }

    private func shift(_ delta: Int) {
        let parser = DateFormatter()
        parser.dateFormat = "yyyy-MM"
        parser.locale = Locale(identifier: "en_US_POSIX")
        guard let date = parser.date(from: month),
              let shifted = Calendar(identifier: .gregorian).date(byAdding: .month, value: delta, to: date) else { return }
        month = parser.string(from: shifted)
    }
}
