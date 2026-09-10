import SwiftUI

/// Search across every enabled catalog that declares a search extra.
struct SearchView: View {
    @EnvironmentObject private var model: AppModel

    @State private var query = ""
    @FocusState private var focused: Bool

    private var state: AppState { model.state }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    HStack(spacing: 10) {
                        HStack(spacing: 8) {
                            GlyphIcon(glyph: .search, size: 15).foregroundStyle(MadariColors.muted)
                            TextField("Search movies and series", text: $query)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .submitLabel(.search)
                                .focused($focused)
                                .onSubmit { submit() }
                            if !query.isEmpty {
                                Button {
                                    query = ""
                                } label: {
                                    GlyphIcon(glyph: .close, size: 13)
                                }
                                .buttonStyle(.plain)
                                .foregroundStyle(MadariColors.muted)
                            }
                        }
                        .font(MadariFont.regular(16))
                        .padding(11)
                        .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 10))

                        Button("Search", action: submit)
                            .buttonStyle(ProminentButton())
                            .frame(width: 110)
                            .disabled(query.trimmingCharacters(in: .whitespaces).isEmpty || state.busy)
                    }
                    .padding(.horizontal, 20)

                    if state.query.isEmpty {
                        EmptyStateView(
                            title: "Find your next favourite",
                            message: "Search across your enabled addons. Only catalogs that declare a search extra are queried."
                        )
                    } else if !state.busy && state.searchShelves.allSatisfy({ $0.titles.isEmpty }) {
                        EmptyStateView(
                            title: "No results",
                            message: "Try another title, or check that your addons declare searchable catalogs."
                        )
                    }

                    ForEach(state.searchShelves.filter { !$0.titles.isEmpty }) { shelf in
                        PosterRow(
                            shelf: shelf,
                            onOpen: { model.push(.details($0)) },
                            isSaved: { model.isSaved($0) },
                            onToggleSaved: { title in Task { await model.toggleSaved(title) } },
                            onMore: { Task { await model.loadMore(shelf, into: .search) } }
                        )
                    }

                    if !state.notices.isEmpty {
                        NoticesView(notices: state.notices)
                    }
                }
                .padding(.vertical, 16)
                .padding(.bottom, 30)
            }
        }
        .task(id: state.query) {
            // The model already ran the search; this only re-runs when a query was
            // restored from a previous visit.
            if query.isEmpty, !state.query.isEmpty { query = state.query }
        }
    }

    private func submit() {
        focused = false
        let value = query
        Task { await model.search(value) }
    }
}

/// Explore: every catalog declared by an enabled addon, with its declared filters.
struct ExploreView: View {
    @EnvironmentObject private var model: AppModel

    private var state: AppState { model.state }

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    Text("Explore").madariHeading().padding(.horizontal, 20)
                    Text("Browse your catalogs")
                        .madariBody().foregroundStyle(MadariColors.muted).padding(.horizontal, 20)

                    if model.catalogs().isEmpty {
                        EmptyStateView(
                            title: "No catalogs yet",
                            message: "Addons supply every catalog. Install one to start browsing.",
                            actionTitle: "Install an addon",
                            action: { model.tab = .settings }
                        )
                    }

                    ForEach(model.catalogs(), id: \.self) { catalog in
                        Button {
                            model.push(.catalog(catalog))
                        } label: {
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(catalog.name).madariBody()
                                    Text("\(catalog.providerName) · \(catalog.type)")
                                        .madariLabel().foregroundStyle(MadariColors.muted)
                                }
                                Spacer()
                                if !catalog.formFields.isEmpty {
                                    Text("\(catalog.formFields.count) filters")
                                        .madariLabel().foregroundStyle(MadariColors.muted)
                                }
                                Image(systemName: "chevron.right")
                                    .font(.system(size: 12, weight: .semibold))
                                    .foregroundStyle(MadariColors.muted)
                            }
                            .padding(16)
                            .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 10))
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal, 20)
                    }
                }
                .padding(.vertical, 16)
                .padding(.bottom, 30)
            }
        }
    }
}

/// One catalog: the declared filter form plus a grid of results.
struct CatalogView: View {
    @EnvironmentObject private var model: AppModel
    let catalog: Catalog

    @State private var fields: [String: String] = [:]
    @State private var shelf: Shelf?
    @State private var loading = false
    @State private var error: String?

    private let columns = [GridItem(.adaptive(minimum: 128), spacing: 16)]

    var body: some View {
        MadariScreen {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text(catalog.name).madariHeading()
                        Text(catalog.providerName).madariLabel().foregroundStyle(MadariColors.muted)
                    }
                    .padding(.horizontal, 20)

                    if !catalog.formFields.isEmpty {
                        FilterForm(catalog: catalog, fields: $fields) { Task { await load() } }
                            .padding(.horizontal, 20)
                    }

                    if let error {
                        ErrorCard(message: error) { Task { await load() } }
                            .padding(.horizontal, 20)
                    }

                    if loading && shelf == nil {
                        ProgressView().tint(MadariColors.accent)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 40)
                    } else if let shelf {
                        if shelf.titles.isEmpty {
                            EmptyStateView(title: "No titles found", message: "Adjust the filters and try again.")
                        } else {
                            LazyVGrid(columns: columns, alignment: .leading, spacing: 20) {
                                ForEach(shelf.titles) { title in
                                    PosterCard(
                                        title: title,
                                        onOpen: { model.push(.details(title)) },
                                        onToggleSaved: { Task { await model.toggleSaved(title) } }
                                    )
                                }
                            }
                            .padding(.horizontal, 20)

                            if shelf.more {
                                Button("Load more") { Task { await loadMore(shelf) } }
                                    .buttonStyle(QuietButton())
                                    .frame(width: 160)
                                    .padding(.horizontal, 20)
                            }
                        }
                    }
                }
                .padding(.vertical, 16)
                .padding(.bottom, 30)
            }
            .navigationTitle(catalog.name)
            .navigationBarTitleDisplayMode(.inline)
            .task { if shelf == nil { await load() } }
        }
    }

    private func load() async {
        loading = true
        error = nil
        do {
            shelf = try await model.query(catalog, extras: fields.filter { !$0.value.isEmpty })
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }

    private func loadMore(_ current: Shelf) async {
        guard let next = try? await model.query(
            catalog,
            extras: current.extras,
            skip: current.skip + 100
        ) else { return }
        let merged = (current.titles + next.titles).reduce(into: [Title]()) { unique, title in
            if !unique.contains(where: { $0.identity == title.identity }) { unique.append(title) }
        }
        shelf = Shelf(
            id: current.id, name: current.name, titles: merged, catalog: catalog,
            skip: next.skip,
            more: next.more && merged.count > current.titles.count,
            extras: current.extras
        )
    }
}

/// The filter fields an addon declares, with its option lists as quick choices.
private struct FilterForm: View {
    let catalog: Catalog
    @Binding var fields: [String: String]
    let onOpen: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ForEach(catalog.formFields, id: \.self) { field in
                VStack(alignment: .leading, spacing: 8) {
                    let required = catalog.required.contains(field)
                    MadariField(
                        label: field + (required ? " (required)" : " (optional)"),
                        prompt: required ? "Required by this catalog" : "Any",
                        text: Binding(
                            get: { fields[field] ?? "" },
                            set: { fields[field] = $0 }
                        ),
                        keyboard: catalog.isNumeric(field) ? .numberPad : .default
                    )
                    let options = catalog.options(for: field)
                    if !options.isEmpty {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 8) {
                                ForEach(options, id: \.self) { option in
                                    Button {
                                        fields[field] = option
                                    } label: {
                                        Text(option)
                                            .madariLabel()
                                            .padding(.horizontal, 11)
                                            .padding(.vertical, 7)
                                            .background(
                                                fields[field] == option ? MadariColors.accent : MadariColors.panel,
                                                in: Capsule()
                                            )
                                            .foregroundStyle(fields[field] == option ? .white : .primary)
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                            .padding(.vertical, 2)
                        }
                    }
                }
            }
            let missing = catalog.required.filter { (fields[$0] ?? "").isEmpty }
            VStack(alignment: .leading, spacing: 8) {
                Button("Open catalog", action: onOpen)
                    .buttonStyle(ProminentButton())
                    .frame(width: 180)
                    .disabled(!missing.isEmpty)
                if !missing.isEmpty {
                    Text("Required: \(missing.joined(separator: ", "))")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                }
            }
        }
        .padding(16)
        .background(MadariColors.panel.opacity(0.5), in: RoundedRectangle(cornerRadius: 12))
    }
}
