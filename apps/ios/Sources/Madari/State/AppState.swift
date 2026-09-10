import Foundation

/// The destinations that can be pushed onto a tab's navigation stack.
///
/// The TV client kept `detail`/`sources`/`catalog` in one global state machine and
/// reset them on every tab change. iOS has real navigation, so each tab keeps its
/// own stack and returning to a tab restores where the user was.
enum Route: Hashable {
    case details(Title)
    case sources(Title, videoId: String)
    case catalog(Catalog)
    /// The TV client has Calendar as its own rail destination. It reports on saved
    /// titles, so on iOS it is pushed from My list instead of taking a tab.
    case calendar
    /// The managed torrents. The TV client has no equivalent: it plays whatever the
    /// selected source points at and never shows what is already on disk.
    case torrents
}

/// The tabs. The TV rail has six destinations; Settings gets its own tab because it
/// holds addons, Trakt and playback preferences, and Calendar moves into My List,
/// where the saved titles it reports on already live.
enum Tab: String, CaseIterable, Identifiable {
    case home = "Home"
    case search = "Search"
    case explore = "Explore"
    case library = "My list"
    case settings = "Settings"

    var id: String { rawValue }

    var glyph: Glyph {
        switch self {
        case .home: .home
        case .search: .search
        case .explore: .explore
        case .library: .library
        case .settings: .settings
        }
    }
}

/// Which shelf list an action applies to.
enum ShelfList {
    case home
    case search
}

/// Everything the screens render. One value, replaced wholesale on change, matching
/// the Android client's `TvState`.
struct AppState {
    var busy = false
    /// Set while a Continue watching card is resolving, so Back can cancel it.
    var resumingTitle: String?
    var error: String?

    var profiles: [JSONValue] = []
    var profileAvatars: [JSONValue] = []
    var activeKids = ""
    var profile: JSONValue?
    var snapshot: JSONValue = .object([:])

    var homeShelves: [Shelf] = []
    var searchShelves: [Shelf] = []
    /// Resolved Continue watching cards, keyed by title identity.
    var continueEntries: [String: ContinueEntry] = [:]

    /// The resolved metadata for the title currently being viewed.
    var detail: Title?
    var videoId: String?
    var sources: [Source]?
    var sourcesLoading = false

    var playback: Playback?

    var settingsUnlocked = false
    var web: JSONValue = .object([:])
    var webAddress = ""
    var calendar: JSONValue = .object([:])
    var query = ""
    var notices: [String] = []
    var trakt: JSONValue = .object([:])
    var traktDevice: JSONValue = .object([:])

    /// The active profile's playback preferences, as stored by the core.
    var preferences: JSONValue { snapshot["playback_preferences"] ?? .object([:]) }
    var addons: [JSONValue] {
        snapshot.objects("addons").filter { $0.boolean("enabled") }
    }
    var allAddons: [JSONValue] { snapshot.objects("addons") }

    var isKidsProfile: Bool { profile?.boolean("kids") == true }
    var profileName: String { profile?.text("name") ?? "" }

    /// A profile is protected when it has a PIN, and kids profiles need the
    /// guardian's. Unprotected profiles unlock settings with an empty PIN.
    var settingsNeedPin: Bool {
        guard let profile else { return true }
        return profile.boolean("pin_protected") || profile.boolean("kids")
    }
}
