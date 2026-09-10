import SwiftUI

/// Settings root.
///
/// Everything used to be stacked on one scrolling page, which buried the few things a
/// user changes often under the many they do not. Each area is now its own destination,
/// so the root is a short list of groups and each group opens onto its own screen.
///
/// Locked until the core authorizes the profile. A profile with no PIN authorizes
/// itself with an empty PIN, exactly as the Linux client does, so Settings never asks
/// for a PIN that was never set.
struct SettingsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var pin = ""

    private var state: AppState { model.state }

    var body: some View {
        MadariScreen {
            if state.settingsUnlocked {
                SettingsList()
            } else if state.settingsNeedPin {
                UnlockSettingsView(pin: $pin)
            } else {
                // Authorizing takes a moment; an empty list would flicker.
                ProgressView()
                    .tint(MadariColors.accent)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        // Registered here, not on the `List` below: a `navigationDestination` inside a
        // lazy container is invisible to the stack, so links highlight and never push.
        .navigationDestination(for: SettingsRoute.self) { route in
            switch route {
            case .profile: ProfileSettingsView()
            case .playback: PlaybackSettingsView()
            case .player: PlayerDisplaySettingsView()
            case .trakt: TraktSettingsView()
            case .addons: AddonsSettingsView()
            case .web: WebSettingsView()
            }
        }
        .navigationDestination(for: LanguageRoute.self) { route in
            LanguagePickerView(route: route)
        }
        .task(id: state.settingsNeedPin) {
            await authorizeIfUnprotected()
        }
    }

    /// An unprotected profile needs no PIN, so it is authorized without asking for one.
    private func authorizeIfUnprotected() async {
        guard !state.settingsUnlocked, !state.settingsNeedPin else { return }
        await model.authorize(pin: "")
    }
}

/// The grouped root list.
private struct SettingsList: View {
    @EnvironmentObject private var model: AppModel

    private var state: AppState { model.state }

    var body: some View {
        List {
            Section {
                NavigationLink(value: SettingsRoute.profile) {
                    SettingsRow(
                        glyph: .profile,
                        title: state.profileName,
                        detail: state.isKidsProfile ? "Kids profile" : "Profile and switching"
                    )
                }
            }

            Section("Playback") {
                NavigationLink(value: SettingsRoute.playback) {
                    SettingsRow(
                        glyph: .subtitles,
                        title: "Tracks and subtitles",
                        detail: trackSummary
                    )
                }
                NavigationLink(value: SettingsRoute.player) {
                    SettingsRow(
                        glyph: .screen,
                        title: "Player display",
                        detail: "Subtitle size, picture size, speed"
                    )
                }
            }

            Section("Accounts") {
                NavigationLink(value: SettingsRoute.trakt) {
                    SettingsRow(
                        glyph: .check,
                        title: "Trakt",
                        detail: state.trakt.boolean("connected")
                            ? "Connected as \(state.trakt.text("username").nilIfEmpty ?? "your account")"
                            : "Not connected"
                    )
                }
            }

            Section("Content") {
                NavigationLink(value: SettingsRoute.addons) {
                    SettingsRow(
                        glyph: .source,
                        title: "Addons",
                        detail: addonSummary
                    )
                }
            }

            Section {
                NavigationLink(value: SettingsRoute.web) {
                    SettingsRow(
                        glyph: .globe,
                        title: "Manage from a browser",
                        detail: state.web.boolean("running") ? state.webAddress : "Not running"
                    )
                }
            } footer: {
                Text("Madari for iOS · powered by the same core as the Linux and TV clients.")
                    .madariLabel()
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(MadariColors.background)
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .task { await model.refreshTrakt() }
    }

    private var trackSummary: String {
        let preferences = state.preferences
        let languages = preferences.strings("audio_languages").count + preferences.strings("subtitle_languages").count
        let subtitles = (preferences["subtitles_enabled"]?.boolValue ?? true) ? "Subtitles on" : "Subtitles off"
        return languages == 0 ? subtitles : "\(subtitles) · \(languages) language preference\(languages == 1 ? "" : "s")"
    }

    private var addonSummary: String {
        let all = state.allAddons
        let enabled = state.addons.count
        if all.isEmpty { return "None installed" }
        return "\(enabled) of \(all.count) enabled"
    }
}

/// The destinations reachable from the settings root.
enum SettingsRoute: Hashable {
    case profile
    case playback
    case player
    case trakt
    case addons
    case web
}

/// One row in the settings list: glyph, title, and a short state summary.
private struct SettingsRow: View {
    let glyph: Glyph
    let title: String
    var detail: String?

    var body: some View {
        HStack(spacing: 13) {
            GlyphIcon(glyph: glyph, size: 16)
                .foregroundStyle(MadariColors.accent)
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).madariBody()
                if let detail, !detail.isEmpty {
                    Text(detail)
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                        .lineLimit(1)
                }
            }
        }
        .padding(.vertical, 3)
    }
}

// MARK: - Unlock

/// Shown only for a profile that actually has a PIN, or a kids profile.
private struct UnlockSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Binding var pin: String

    var body: some View {
        VStack(spacing: 18) {
            LockedHeader(
                title: "Settings are locked",
                message: model.state.isKidsProfile
                    ? "Enter the guardian's PIN to change this kids profile."
                    : "Enter this profile's PIN to change its addons and preferences."
            )
            .padding(.horizontal, 24)
            Spacer()
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 12) {
                MadariField(
                    label: model.state.isKidsProfile ? "Guardian PIN" : "PIN",
                    prompt: "PIN",
                    text: $pin,
                    secret: true,
                    keyboard: .numberPad
                )
                Button("Unlock settings") {
                    let value = pin
                    Task { await model.authorize(pin: value) }
                }
                .buttonStyle(ProminentButton())
                .disabled(pin.isEmpty)
            }
            .padding(20)
            .background(.ultraThinMaterial)
        }
    }
}

private struct LockedHeader: View {
    let title: String
    let message: String

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            GlyphIcon(glyph: .lock, size: 26)
                .foregroundStyle(MadariColors.accent)
            Text(title).madariHeading()
            Text(message).madariBody().foregroundStyle(MadariColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, 40)
    }
}

// MARK: - Profile

private struct ProfileSettingsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var guardianPin = ""

    var body: some View {
        SettingsForm {
            Section("Current profile") {
                LabeledContent("Name") {
                    Text(model.state.profileName).foregroundStyle(MadariColors.muted)
                }
                LabeledContent("Type") {
                    Text(model.state.isKidsProfile ? "Kids" : "Regular")
                        .foregroundStyle(MadariColors.muted)
                }
                LabeledContent("PIN") {
                    Text(model.state.settingsNeedPin ? "Set" : "Not set")
                        .foregroundStyle(MadariColors.muted)
                }
            }

            Section {
                if model.state.isKidsProfile {
                    MadariField(
                        label: "Guardian PIN",
                        prompt: "Required to switch",
                        text: $guardianPin,
                        secret: true,
                        keyboard: .numberPad
                    )
                }
                Button("Switch profile") {
                    let value = guardianPin
                    guardianPin = ""
                    Task { await model.leave(pin: value) }
                }
                .disabled(model.state.isKidsProfile && guardianPin.isEmpty)
            } footer: {
                Text("Profiles are created, renamed and deleted from the profile picker.")
                    .madariLabel()
            }
        }
        .navigationTitle("Profile")
    }
}

// MARK: - Playback preferences

/// Per-profile track preferences, from the core's snapshot.
private struct PlaybackSettingsView: View {
    @EnvironmentObject private var model: AppModel

    private static let preferences: [(key: String, label: String, detail: String)] = [
        ("subtitle_sdh", "SDH subtitles", "Dialogue and sound descriptions"),
        ("subtitle_forced", "Forced subtitles", "Translated foreign dialogue"),
        ("audio_description", "Audio description", "Narrated visual action"),
        ("audio_commentary", "Audio commentary", ""),
    ]

    private var current: JSONValue { model.state.preferences }

    var body: some View {
        SettingsForm {
            Section {
                Toggle(isOn: Binding(
                    get: { current["subtitles_enabled"]?.boolValue ?? true },
                    set: { value in
                        Task { await model.savePreferences { $0["subtitles_enabled"] = .bool(value) } }
                    }
                )) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Subtitles by default").madariBody()
                        Text("Applied when a video opens; tracks can still be changed while watching.")
                            .madariLabel()
                            .foregroundStyle(MadariColors.muted)
                    }
                }
                .tint(MadariColors.accent)
            }

            Section("Track preferences") {
                ForEach(Self.preferences, id: \.key) { entry in
                    Picker(selection: Binding(
                        get: { current.text(entry.key).nilIfEmpty ?? "any" },
                        set: { value in
                            Task { await model.savePreferences { $0[entry.key] = .string(value) } }
                        }
                    )) {
                        Text("No preference").tag("any")
                        Text("Prefer").tag("prefer")
                        Text("Avoid when possible").tag("avoid")
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(entry.label).madariBody()
                            if !entry.detail.isEmpty {
                                Text(entry.detail).madariLabel().foregroundStyle(MadariColors.muted)
                            }
                        }
                    }
                }
            }

            Section {
                LanguageLink(title: "Audio languages", key: "audio_languages")
                LanguageLink(title: "Subtitle languages", key: "subtitle_languages")
            } footer: {
                Text("The first available language in each list wins.")
                    .madariLabel()
            }
        }
        .navigationTitle("Tracks and subtitles")
    }
}

/// One language-priority destination.
private struct LanguageLink: View {
    @EnvironmentObject private var model: AppModel
    let title: String
    let key: String

    var body: some View {
        NavigationLink(value: LanguageRoute(key: key, title: title)) {
            LabeledContent(title) {
                Text(summary).foregroundStyle(MadariColors.muted).lineLimit(1)
            }
        }
    }

    private var summary: String {
        let languages = model.state.preferences.strings(key)
        if languages.isEmpty { return "No preference" }
        return languages.map(Language.name).joined(separator: ", ")
    }
}

/// The ordered language editor.
struct LanguageRoute: Hashable {
    let key: String
    let title: String
}

struct LanguagePickerView: View {
    @EnvironmentObject private var model: AppModel
    let route: LanguageRoute

    @State private var list: [String] = []
    @State private var loaded = false

    var body: some View {
        List {
            Section {
                if list.isEmpty {
                    Text("No preference. The video's default is used.")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                }
                ForEach(Array(list.enumerated()), id: \.offset) { index, code in
                    HStack {
                        Text(Language.name(code)).madariBody()
                        Spacer()
                        Button { move(index, by: -1) } label: { Image(systemName: "arrow.up") }
                            .disabled(index == 0)
                        Button { move(index, by: 1) } label: { Image(systemName: "arrow.down") }
                            .disabled(index == list.count - 1)
                    }
                }
                .onMove { from, to in list.move(fromOffsets: from, toOffset: to) }
                .onDelete { offsets in list.remove(atOffsets: offsets) }
            } header: {
                Text("Priority")
            } footer: {
                Text("Drag to reorder. The first language a track offers wins.")
                    .madariLabel()
            }

            Section("Add a language") {
                ForEach(Language.common.filter { !list.contains($0) }, id: \.self) { code in
                    Button {
                        list.append(code)
                    } label: {
                        HStack {
                            Text(Language.name(code)).madariBody()
                            Spacer()
                            Image(systemName: "plus").foregroundStyle(MadariColors.accent)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(MadariColors.background)
        .navigationTitle(route.title)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { EditButton() }
        }
        .task {
            if !loaded {
                list = model.state.preferences.strings(route.key)
                loaded = true
            }
        }
        .onChange(of: list) { _, next in
            // Saved on every change: this screen has no explicit commit step.
            Task { await model.savePreferences { $0[route.key] = .array(next.map { .string($0) }) } }
        }
    }

    private func move(_ index: Int, by delta: Int) {
        let target = index + delta
        guard list.indices.contains(target) else { return }
        list.swapAt(index, target)
    }
}

// MARK: - Player display

/// Device-wide playback display defaults, matching the TV client.
private struct PlayerDisplaySettingsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var subtitleSize = "medium"
    @State private var picture = 0
    @State private var speed = 1.0

    private static let speeds: [Double] = [0.5, 0.75, 1, 1.25, 1.5, 2]

    var body: some View {
        SettingsForm {
            Section("Subtitle size") {
                Picker("Size", selection: $subtitleSize) {
                    Text("Small").tag("small")
                    Text("Medium").tag("medium")
                    Text("Large").tag("large")
                }
                .pickerStyle(.segmented)
                .onChange(of: subtitleSize) { _, value in
                    model.setPlayerPreference("player_subtitle_size", value)
                }
            }

            Section("Picture size") {
                Picker("Picture", selection: $picture) {
                    Text("Fit").tag(0)
                    Text("Zoom").tag(1)
                    Text("Stretch").tag(2)
                }
                .pickerStyle(.segmented)
                .onChange(of: picture) { _, value in
                    model.setPlayerPreference("player_resize", value)
                }
            }

            Section("Default speed") {
                Picker("Speed", selection: $speed) {
                    ForEach(Self.speeds, id: \.self) { value in
                        Text(Self.label(value)).tag(value)
                    }
                }
                .pickerStyle(.menu)
                .onChange(of: speed) { _, value in
                    model.setPlayerPreference("player_speed", value)
                }
            }

            Section {
                EmptyView()
            } footer: {
                Text("Device-wide defaults. They can still be changed while watching.")
                    .madariLabel()
            }
        }
        .navigationTitle("Player display")
        .task {
            subtitleSize = model.subtitleSize
            picture = model.pictureSize
            speed = model.defaultSpeed
        }
    }

    private static func label(_ value: Double) -> String {
        value == value.rounded() ? "\(Int(value))×" : "\(value)×"
    }
}

// MARK: - Trakt

/// Per-profile Trakt connection. Credentials live in this app's defaults; the tokens
/// themselves live in the core's profile database.
private struct TraktSettingsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var setup = false
    @State private var clientId = ""
    @State private var clientSecret = ""
    @State private var redirect = ""

    var body: some View {
        let trakt = model.state.trakt
        let device = model.state.traktDevice
        SettingsForm {
            if !device.text("user_code").isEmpty {
                Section("Authorize") {
                    Text("Enter this code on Trakt. It expires in \(max(device.number("expires_in") / 60, 1)) minutes.")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                    Text(device.text("user_code"))
                        .font(MadariFont.bold(28))
                        .foregroundStyle(MadariColors.accent)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .textSelection(.enabled)
                    Button("Open Trakt") {
                        if let url = URL(string: device.text("verification_url")) {
                            UIApplication.shared.open(url)
                        }
                    }
                    Button("Cancel", role: .destructive) { model.traktCancel() }
                }
            } else if trakt.boolean("connected") {
                Section("Connected") {
                    LabeledContent("Account") {
                        Text(trakt.text("username").nilIfEmpty ?? "your account")
                            .foregroundStyle(MadariColors.muted)
                    }
                    LabeledContent("Lists") {
                        Text("\(trakt.integer("lists"))").foregroundStyle(MadariColors.muted)
                    }
                    Button("Sync now") { Task { await model.traktSync() } }
                    Button("Disconnect", role: .destructive) { Task { await model.traktDisconnect() } }
                }
            } else {
                Section {
                    Button("Connect Trakt") {
                        let saved = model.traktCredentials
                        clientId = saved.clientId
                        clientSecret = saved.clientSecret
                        redirect = saved.redirect
                        setup = true
                    }
                } footer: {
                    Text("Import your watchlist, watched history and personal lists. Playback reports progress to Trakt.")
                        .madariLabel()
                }
            }
        }
        .navigationTitle("Trakt")
        .task { await model.refreshTrakt() }
        .sheet(isPresented: $setup) {
            NavigationStack {
                Form {
                    Section {
                        Text("Create a Trakt application at trakt.tv/oauth/applications, then paste its credentials. They are stored only on this device.")
                            .madariLabel()
                            .foregroundStyle(MadariColors.muted)
                    }
                    Section {
                        MadariField(label: "Client ID", prompt: "Client ID", text: $clientId)
                        MadariField(label: "Client secret", prompt: "Client secret", text: $clientSecret, secret: true)
                        MadariField(label: "Redirect URI", prompt: "urn:ietf:wg:oauth:2.0:oob", text: $redirect)
                    }
                }
                .navigationTitle("Connect Trakt")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { setup = false }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Continue") {
                            model.saveTraktCredentials(clientId: clientId, clientSecret: clientSecret, redirect: redirect)
                            setup = false
                            Task { await model.traktConnect() }
                        }
                        .disabled(clientId.trimmingCharacters(in: .whitespaces).isEmpty
                                  || clientSecret.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
        }
    }
}

// MARK: - Addons

private struct AddonsSettingsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var recommended: [JSONValue] = []
    @State private var showInstall = false
    @State private var removing: JSONValue?
    @State private var configuring: JSONValue?
    @State private var sharing: JSONValue?

    private var addons: [JSONValue] { model.state.allAddons }

    var body: some View {
        List {
            recommendedSection
            installedSection
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(MadariColors.background)
        .navigationTitle("Addons")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showInstall = true
                } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel("Install an addon from a URL")
            }
            ToolbarItem(placement: .topBarTrailing) { EditButton() }
        }
        .task { await loadRecommended() }
        .sheet(isPresented: $showInstall) {
            InstallAddonSheet { url, allowLocal in
                Task { await install(url, allowLocal: allowLocal) }
            }
            .environmentObject(model)
        }
        .sheet(item: Binding(get: { removing.map(SettingsTarget.init) }, set: { removing = $0?.value })) { target in
            ConfirmationSheet(
                title: "Remove addon?",
                message: "Remove \(target.value["manifest"]?.text("name") ?? "this addon") from your profile?",
                confirmTitle: "Remove",
                onConfirm: {
                    let addon = target.value
                    removing = nil
                    Task { await model.removeAddon(addon) }
                },
                onCancel: { removing = nil }
            )
        }
        .sheet(item: Binding(get: { configuring.map(SettingsTarget.init) }, set: { configuring = $0?.value })) { target in
            ConfigureAddonSheet(addon: target.value) { configuring = nil }
                .environmentObject(model)
        }
        .sheet(item: Binding(get: { sharing.map(SettingsTarget.init) }, set: { sharing = $0?.value })) { target in
            ShareAddonSheet(addon: target.value) { sharing = nil }
                .environmentObject(model)
        }
    }

    private var recommendedSection: some View {
        Section {
            if recommended.isEmpty {
                Text("Checking the recommended addons…")
                    .madariLabel()
                    .foregroundStyle(MadariColors.muted)
            }
            ForEach(recommended, id: \.entryURL) { entry in
                RecommendedAddonRow(entry: entry) {
                    Task { await install(entry.entryURL) }
                }
            }
        } header: {
            Text("Recommended")
        } footer: {
            Text(Self.recommendedFooter).madariLabel()
        }
    }

    private var installedSection: some View {
        Section {
            if addons.isEmpty {
                Text("Nothing installed yet.")
                    .madariLabel()
                    .foregroundStyle(MadariColors.muted)
            }
            ForEach(addons, id: \.installationID) { addon in
                AddonRow(addon: addon) {
                    Task { await model.toggleAddon(addon) }
                }
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    Button(role: .destructive) { removing = addon } label: {
                        Label("Remove", systemImage: "trash")
                    }
                    Button { configuring = addon } label: {
                        Label("Configure", systemImage: "slider.horizontal.3")
                    }
                    Button { sharing = addon } label: {
                        Label("Share", systemImage: "person.badge.plus")
                    }
                }
            }
            .onMove { from, to in
                var ids = addons.map(\.installationID)
                ids.move(fromOffsets: from, toOffset: to)
                Task { await model.reorderAddons(ids) }
            }
        } header: {
            Text("Installed")
        } footer: {
            Text(addons.isEmpty ? Self.emptyInstalledFooter : Self.reorderFooter).madariLabel()
        }
    }

    private static let recommendedFooter =
        "Public addons that need no account. Installing fetches the manifest and adds it to this profile."
    private static let emptyInstalledFooter =
        "Addons supply every catalog, stream and subtitle."
    private static let reorderFooter =
        "Order decides which addon is asked first. Drag to reorder."

    /// The curated list comes from the core, so the URLs are not duplicated here.
    private func loadRecommended() async {
        if let catalog = try? await NativeCore.shared.call("addon_catalog") {
            recommended = catalog.objects("recommended")
        }
    }

    private func install(_ url: String, allowLocal: Bool = false) async {
        await model.install(url: url, allowLocal: allowLocal)
        // Reflect the new installed state on the recommended rows.
        await loadRecommended()
    }
}

/// A curated addon: name, what it does, and Install or Installed.
private struct RecommendedAddonRow: View {
    let entry: JSONValue
    let onInstall: () -> Void

    private var installed: Bool { entry.boolean("installed") }

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(entry.text("name").nilIfEmpty ?? entry.text("url"))
                    .madariBody()
                Text(entry.text("description"))
                    .madariLabel()
                    .foregroundStyle(MadariColors.muted)
                    .lineLimit(2)
            }
            Spacer(minLength: 8)
            if installed {
                Label("Installed", systemImage: "checkmark")
                    .labelStyle(.titleAndIcon)
                    .font(MadariFont.medium(12))
                    .foregroundStyle(MadariColors.accent)
            } else {
                Button("Install", action: onInstall)
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .tint(MadariColors.accent)
            }
        }
        .padding(.vertical, 2)
    }
}

/// Installs an addon from a manifest URL, for anything not in the curated list.
private struct InstallAddonSheet: View {
    let onInstall: (String, Bool) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var url = ""
    @State private var allowLocal = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    MadariField(
                        label: "Manifest URL",
                        prompt: "https://example.com/manifest.json",
                        text: $url,
                        keyboard: .URL
                    )
                    Toggle("Allow local network", isOn: $allowLocal)
                        .tint(MadariColors.accent)
                } footer: {
                    Text("Every addon is a manifest.json URL. Addons that need configuring must be configured by their provider first.")
                        .madariLabel()
                }
            }
            .navigationTitle("Install addon")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Install") {
                        let value = url
                        let local = allowLocal
                        dismiss()
                        onInstall(value, local)
                    }
                    .disabled(url.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
        .presentationDetents([.medium])
    }
}

/// One installed addon: name, options and an inline enable switch.
private struct AddonRow: View {
    let addon: JSONValue
    let onToggle: () -> Void

    private var manifest: JSONValue { addon["manifest"] ?? .object([:]) }
    private var enabled: Bool { addon.boolean("enabled") }

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(manifest.text("name").nilIfEmpty ?? "Addon")
                    .madariBody()
                let detail = [manifest.text("version"), manifest.strings("types").joined(separator: ", ")]
                    .filter { !$0.isEmpty }
                    .joined(separator: " · ")
                if !detail.isEmpty {
                    Text(detail).madariLabel().foregroundStyle(MadariColors.muted).lineLimit(1)
                }
            }
            Spacer()
            Toggle("", isOn: Binding(get: { enabled }, set: { _ in onToggle() }))
                .labelsHidden()
                .tint(MadariColors.accent)
        }
        .padding(.vertical, 2)
    }
}

/// A sheet payload; addon installation ids are unique within a profile.
struct SettingsTarget: Identifiable {
    let value: JSONValue
    var id: String { value.text("installation_id") }
}

/// Reconfiguring a shared installation affects every linked profile.
private struct ConfigureAddonSheet: View {
    let addon: JSONValue
    let onDismiss: () -> Void

    @EnvironmentObject private var model: AppModel
    @State private var url = ""
    @State private var allowLocal = false
    @State private var linked: [String] = []

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(linked.isEmpty
                         ? "Paste the complete configured manifest URL."
                         : "Changes apply to every linked profile: \(linked.joined(separator: ", ")).")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                }
                Section {
                    MadariField(label: "Manifest URL", prompt: "https://…", text: $url, keyboard: .URL)
                    Toggle("Allow local network", isOn: $allowLocal)
                        .tint(MadariColors.accent)
                }
            }
            .navigationTitle("Configure \(addon["manifest"]?.text("name") ?? "addon")")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { onDismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        let value = url
                        let local = allowLocal
                        onDismiss()
                        Task {
                            await model.configureAddon(
                                id: addon.text("installation_id"), url: value, allowLocal: local
                            )
                        }
                    }
                    .disabled(url.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .task { linked = await model.linkedProfiles(addon) }
        }
    }
}

/// Links one installation to another profile; its PIN, or a guardian's, is required.
private struct ShareAddonSheet: View {
    let addon: JSONValue
    let onDismiss: () -> Void

    @EnvironmentObject private var model: AppModel
    @State private var target = ""
    @State private var pin = ""

    var body: some View {
        let targets = model.state.profiles.filter { $0.text("id") != model.state.profile?.text("id") }
        NavigationStack {
            Form {
                if targets.isEmpty {
                    Section {
                        Text("Create another profile before sharing an addon.")
                            .madariLabel()
                            .foregroundStyle(MadariColors.muted)
                    }
                } else {
                    Section {
                        Text("This links one installation. Future configuration changes affect every linked profile. Library and progress stay separate.")
                            .madariLabel()
                            .foregroundStyle(MadariColors.muted)
                    }
                    Section("Share with") {
                        ForEach(targets, id: \.itemID) { profile in
                            Button {
                                target = profile.text("id")
                            } label: {
                                HStack {
                                    Text(profile.text("name")).madariBody()
                                    Spacer()
                                    if target == profile.text("id") {
                                        Image(systemName: "checkmark").foregroundStyle(MadariColors.accent)
                                    }
                                }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    Section {
                        MadariField(
                            label: "Recipient or guardian PIN, if required",
                            prompt: "PIN", text: $pin, secret: true, keyboard: .numberPad
                        )
                    }
                }
            }
            .navigationTitle("Share \(addon["manifest"]?.text("name") ?? "addon")")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { onDismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Share") {
                        let chosen = target
                        let value = pin
                        onDismiss()
                        Task { await model.shareAddon(addon, targetId: chosen, pin: value) }
                    }
                    .disabled(target.isEmpty)
                }
            }
        }
    }
}

// MARK: - Web settings

/// The LAN settings server the core hosts, for managing this installation from a
/// browser on the same network.
private struct WebSettingsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        let web = model.state.web
        SettingsForm {
            Section {
                if web.boolean("running") {
                    LabeledContent("Address") {
                        Text(model.state.webAddress)
                            .foregroundStyle(MadariColors.accent)
                            .textSelection(.enabled)
                    }
                    LabeledContent("Pairing code") {
                        Text(web.text("code"))
                            .font(MadariFont.semibold(17))
                            .foregroundStyle(MadariColors.accent)
                            .textSelection(.enabled)
                    }
                } else {
                    Text("Start web settings to manage profiles, addon URLs and playback preferences in a browser on this network.")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                }
                Button(web.boolean("running") ? "Stop web settings" : "Start web settings") {
                    Task { await model.toggleWeb() }
                }
            } footer: {
                Text("iOS asks for local-network access the first time the server starts.")
                    .madariLabel()
            }
        }
        .navigationTitle("Manage from a browser")
    }
}

// MARK: - Shared chrome

/// A grouped settings form on the app's own background.
private struct SettingsForm<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        Form { content }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(MadariColors.background)
    }
}

/// A yes/no confirmation, sized to its content.
struct ConfirmationSheet: View {
    let title: String
    let message: String
    let confirmTitle: String
    let onConfirm: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(title).madariHeading()
            Text(message).madariBody()
            HStack(spacing: 10) {
                Button(confirmTitle, role: .destructive, action: onConfirm)
                    .buttonStyle(ProminentButton())
                Button("Cancel", action: onCancel)
                    .buttonStyle(QuietButton())
            }
        }
        .padding(24)
        .presentationDetents([.height(210)])
    }
}

/// Language tags and their display names.
enum Language {
    static let common = [
        "en", "hi", "ta", "te", "ml", "kn", "mr", "bn", "gu", "pa", "ur", "as", "or", "ne", "si",
        "fr", "de", "es", "pt", "it", "nl", "pl", "ru", "uk", "tr", "sv", "da", "fi", "no", "el",
        "ja", "ko", "zh", "ar", "he", "fa", "th", "vi", "id", "ms", "tl",
    ]

    private static let aliases: [String: String] = [
        "eng": "en", "hin": "hi", "tam": "ta", "tel": "te", "mal": "ml", "kan": "kn",
        "mar": "mr", "ben": "bn", "guj": "gu", "pan": "pa", "urd": "ur", "asm": "as",
        "ori": "or", "nep": "ne", "sin": "si", "fra": "fr", "fre": "fr", "deu": "de",
        "ger": "de", "spa": "es", "por": "pt", "ita": "it", "nld": "nl", "dut": "nl",
        "pol": "pl", "rus": "ru", "ukr": "uk", "tur": "tr", "swe": "sv", "dan": "da",
        "fin": "fi", "nor": "no", "ell": "el", "gre": "el", "jpn": "ja", "kor": "ko",
        "zho": "zh", "chi": "zh", "ara": "ar", "heb": "he", "iw": "he", "fas": "fa",
        "per": "fa", "tha": "th", "vie": "vi", "ind": "id", "msa": "ms", "may": "ms",
        "tgl": "tl", "fil": "tl",
    ]

    /// Two-letter base of a tag in either ISO 639-1 or 639-2 form.
    static func base(_ code: String) -> String {
        let normalized = code.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "_", with: "-").lowercased()
        let head = normalized.split(separator: "-").first.map(String.init) ?? ""
        return aliases[head] ?? head
    }

    /// The name the Linux client shows, so both clients read the same.
    static func name(_ code: String) -> String {
        let normalized = code.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "_", with: "-").lowercased()
        guard !normalized.isEmpty else { return "" }
        let tags = normalized.split(separator: "-").map(String.init)
        let head = tags.first ?? ""
        let english = Locale(identifier: "en_US").localizedString(forLanguageCode: aliases[head] ?? head) ?? ""
        let name = english.isEmpty ? code.trimmingCharacters(in: .whitespaces) : english
        let qualifiers = tags.dropFirst().map { tag -> String in
            switch tag {
            case "us": "United States"
            case "gb": "United Kingdom"
            case "br": "Brazil"
            case "pt": "Portugal"
            case "hans": "Simplified"
            case "hant": "Traditional"
            default: tag.uppercased()
            }
        }
        return qualifiers.isEmpty ? name : "\(name) (\(qualifiers.joined(separator: ", ")))"
    }
}
