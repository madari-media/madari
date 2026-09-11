import SwiftUI

/// The profile picker.
///
/// Touching a tile opens the profile (asking for a PIN when it has one); the context
/// menu offers edit. The TV client used "hold OK to edit", which has no touch
/// equivalent, so editing moved to a long-press menu.
struct ProfilesView: View {
    @EnvironmentObject private var model: AppModel

    @State private var pin = ""
    /// The single presented dialog. Only one can be up at a time, so an enum is both
    /// simpler and more reliable than several `sheet` modifiers.
    @State private var presented: ProfileSheet?

    var body: some View {
        ZStack {
            // The poster wall for this device. A desktop image here is 3840x2160, and
            // filling that into a portrait screen is what used to push the picker off it.
            Artwork(url: DeviceWallpaper.url)
                .ignoresSafeArea()
            // Scrims for legibility over an unknown photograph: darker at the top and
            // bottom where the chrome sits, and a soft vertical wash behind the text.
            LinearGradient(
                colors: [
                    MadariColors.background.opacity(0.94),
                    MadariColors.background.opacity(0.55),
                    MadariColors.background.opacity(0.72),
                    MadariColors.background.opacity(0.99),
                ],
                startPoint: .top, endPoint: .bottom
            )
            .ignoresSafeArea()
            // A vignette centred on the content column: the scrim alone is not enough
            // to keep white text legible over an unknown photograph.
            RadialGradient(
                colors: [MadariColors.background.opacity(0.78), .clear],
                center: .center,
                startRadius: 20,
                endRadius: 330
            )
            .ignoresSafeArea()

            GeometryReader { proxy in
                // The padding below is 24 a side; the grid is capped at 560 wide.
                let perRow = columnsPerRow(for: min(proxy.size.width, 560) - 48)
                ScrollView {
                    VStack(spacing: 0) {
                        header
                        Spacer(minLength: 28)
                        titles
                        profileGrid(perRow: perRow).padding(.top, 28)
                        Spacer(minLength: 28)
                        footer
                    }
                    .padding(.horizontal, 24)
                    .padding(.vertical, 20)
                    // Lets the spacers centre the block while the page still scrolls
                    // when the list outgrows the screen.
                    .frame(minHeight: proxy.size.height, alignment: .top)
                    .frame(maxWidth: 560)
                    .frame(maxWidth: .infinity)
                }
                .scrollBounceBehavior(.basedOnSize)
            }
        }
        // One sheet modifier, driven by an enum. Three stacked `.sheet` modifiers on
        // the same view are unreliable in SwiftUI, and here they made a tap present
        // the wrong dialog.
        .onAppear { DebugLog.write("picker appeared: profiles=\(model.state.profiles.count) visible=\(visibleProfiles.count)") }
        .onChange(of: model.state.profiles.count) { _, count in
            DebugLog.write("picker profiles changed: \(count) visible=\(visibleProfiles.count) activeKids=\(model.state.activeKids.isEmpty ? "none" : model.state.activeKids)")
        }
        .sheet(item: $presented) { sheet in
            switch sheet {
            case .pin(let profile):
                PinPrompt(profile: profile) { entered in
                    presented = nil
                    Task { await model.unlock(profile, pin: entered) }
                }
            case .create:
                ProfileEditor(mode: .create(guardian: guardian), avatars: model.state.profileAvatars)
                    .environmentObject(model)
            case .edit(let profile):
                ProfileEditor(mode: .edit(profile), avatars: model.state.profileAvatars)
                    .environmentObject(model)
            case .delete(let profile):
                DeleteProfilePrompt(profile: profile) { pin in
                    presented = nil
                    Task { await model.deleteProfile(profile, pin: pin) }
                } onCancel: {
                    presented = nil
                }
            }
        }
    }

    /// Brand lockup. Profile management lives on the tiles themselves, reached by
    /// long press, which is where a user would look for it.
    private var header: some View {
        HStack(spacing: 10) {
            BrandLogo(size: 28)
            Text("madari")
                .font(MadariFont.medium(18))
                .foregroundStyle(.white)
            Spacer()
        }
    }

    private var titles: some View {
        VStack(spacing: 9) {
            Text(startsEmpty ? "Make yourself at home" : "Who's watching?")
                .font(MadariFont.semibold(30))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)
                .shadow(color: .black.opacity(0.65), radius: 10, y: 2)
            Text(startsEmpty ? "Create a profile to get started." : "Pick a profile to continue.")
                .madariBody()
                .foregroundStyle(Color.white.opacity(0.82))
                .multilineTextAlignment(.center)
                .shadow(color: .black.opacity(0.6), radius: 8, y: 1)
        }
    }

    /// One row per line of tiles, centred.
    ///
    /// Not a `LazyVGrid`: flexible columns shrink as the count grows, but a tile is a
    /// fixed 104pt, so four columns on a phone overflowed their cells and the tiles
    /// overlapped. Rows are laid out from the width actually available instead.
    private func profileGrid(perRow: Int) -> some View {
        let items = profileItems
        let rows = stride(from: 0, to: items.count, by: perRow).map { start in
            Array(items[start..<min(start + perRow, items.count)])
        }
        return VStack(spacing: 24) {
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                HStack(alignment: .top, spacing: 18) {
                    ForEach(row) { item in
                        switch item {
                        case .profile(let profile):
                            ProfileTile(
                                profile: profile,
                                avatarURL: avatarURL(profile),
                                onOpen: { open(profile) },
                                onEdit: { presented = .edit(profile) },
                                onDelete: { presented = .delete(profile) }
                            )
                        case .add:
                            ProfileTile(
                                profile: nil,
                                avatarURL: nil,
                                onOpen: { presented = .create },
                                onEdit: nil,
                                onDelete: nil
                            )
                        }
                    }
                }
            }
        }
    }

    /// How many tiles fit on one line, capped by how many there are to show.
    private func columnsPerRow(for width: CGFloat) -> Int {
        let tile: CGFloat = 104
        let spacing: CGFloat = 18
        let fit = max(1, Int((width + spacing) / (tile + spacing)))
        return min(fit, max(1, profileItems.count))
    }

    /// The profiles to show, then the add tile.
    private var profileItems: [ProfileTileItem] {
        visibleProfiles.map { .profile($0) } + [.add]
    }

    private var footer: some View {
        VStack(spacing: 6) {
            if startsEmpty {
                Text("A little space for everything you love.")
                    .madariLabel()
                    .foregroundStyle(Color.white.opacity(0.55))
            } else {
                Text("Tap to open · long press to edit or delete")
                    .madariLabel()
                    .foregroundStyle(Color.white.opacity(0.55))
            }
        }
    }

    /// True only before the first profile exists, so the empty state does not flash
    /// while the store is still being read.
    private var startsEmpty: Bool {
        model.state.profiles.isEmpty && !model.state.busy
    }

    /// Kids devices only offer the kids profile, which the core reports as active.
    private var visibleProfiles: [JSONValue] {
        model.state.profiles.filter {
            model.state.activeKids.isEmpty || $0.text("id") == model.state.activeKids
        }
    }

    /// The adult that authorizes creating another profile: the first regular profile.
    private var guardian: JSONValue? {
        visibleProfiles.first { !$0.boolean("kids") }
    }

    private func avatarURL(_ profile: JSONValue) -> String? {
        model.state.profileAvatars
            .first { $0.text("id") == profile.text("avatar") }?
            .text("url")
            .nilIfEmpty
    }

    private func open(_ profile: JSONValue) {
        if profile.boolean("pin_protected") {
            pin = ""
            presented = .pin(profile)
        } else {
            Task { await model.unlock(profile, pin: "") }
        }
    }
}

/// One tile in the picker: a profile, or the add affordance.
private enum ProfileTileItem: Identifiable {
    case profile(JSONValue)
    case add

    var id: String {
        switch self {
        case .profile(let profile): profile.text("id")
        case .add: "__add_profile__"
        }
    }
}

/// Confirms deleting a profile.
///
/// Destructive and irreversible, so it names the profile, says what else goes with it,
/// and requires the profile's PIN (or a guardian's for a kids profile) before the
/// destructive button is enabled.
private struct DeleteProfilePrompt: View {
    let profile: JSONValue
    let onDelete: (String) -> Void
    let onCancel: () -> Void

    @State private var pin = ""

    private var isKids: Bool { profile.boolean("kids") }
    private var needsPin: Bool { profile.boolean("pin_protected") || isKids }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("“\(profile.text("name"))” will be removed from this device, along with its library, playback progress, addon links and any Trakt connection.")
                        .madariBody()
                    Text("This cannot be undone. Your other profiles are not affected.")
                        .madariLabel()
                        .foregroundStyle(MadariColors.muted)
                }
                if needsPin {
                    Section {
                        MadariField(
                            label: isKids ? "Guardian PIN" : "PIN",
                            prompt: isKids ? "The guardian's PIN" : "This profile's PIN",
                            text: $pin,
                            secret: true,
                            keyboard: .numberPad
                        )
                    } footer: {
                        Text(isKids
                             ? "A kids profile is deleted with its guardian's PIN."
                             : "Enter this profile's PIN to confirm.")
                            .madariLabel()
                    }
                }
                Section {
                    Button(role: .destructive) {
                        onDelete(pin)
                    } label: {
                        Text("Delete \(profile.text("name"))")
                            .font(MadariFont.semibold(16))
                            .frame(maxWidth: .infinity)
                    }
                    .disabled(needsPin && pin.isEmpty)
                }
            }
            .navigationTitle("Delete profile")
            .inlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", action: onCancel)
                }
            }
            .sheetDetents([.medium, .large])
        }
    }
}

/// The profile picker's dialogs.
enum ProfileSheet: Identifiable {
    case pin(JSONValue)
    case create
    case edit(JSONValue)
    case delete(JSONValue)

    var id: String {
        switch self {
        case .pin(let profile): "pin-\(profile.text("id"))"
        case .create: "create"
        case .edit(let profile): "edit-\(profile.text("id"))"
        case .delete(let profile): "delete-\(profile.text("id"))"
        }
    }
}

private struct ProfileTile: View {
    let profile: JSONValue?
    let avatarURL: String?
    let onOpen: () -> Void
    var onEdit: (() -> Void)?
    var onDelete: (() -> Void)?

    /// Deterministic tile colours, matching the TV picker's palette. Each is used as a
    /// gradient so an avatar-less tile still looks deliberate rather than flat.
    private static let palette: [(Color, Color)] = [
        (Color(red: 0x42 / 255, green: 0x6A / 255, blue: 0x88 / 255), Color(red: 0x28 / 255, green: 0x44 / 255, blue: 0x5C / 255)),
        (Color(red: 0x87 / 255, green: 0x63 / 255, blue: 0x4F / 255), Color(red: 0x5B / 255, green: 0x3F / 255, blue: 0x33 / 255)),
        (Color(red: 0x60 / 255, green: 0x5D / 255, blue: 0x88 / 255), Color(red: 0x3D / 255, green: 0x3B / 255, blue: 0x5C / 255)),
        (Color(red: 0x44 / 255, green: 0x79 / 255, blue: 0x6E / 255), Color(red: 0x2A / 255, green: 0x51 / 255, blue: 0x49 / 255)),
        (Color(red: 0x89 / 255, green: 0x57 / 255, blue: 0x70 / 255), Color(red: 0x5C / 255, green: 0x37 / 255, blue: 0x49 / 255)),
    ]

    private static let side: CGFloat = 104
    private static let corner: CGFloat = 20

    private var name: String { profile?.text("name") ?? "Add profile" }
    private var isAdd: Bool { profile == nil }
    private var isKids: Bool { profile?.boolean("kids") == true }
    private var isProtected: Bool { profile?.boolean("pin_protected") == true }

    var body: some View {
        Button(action: onOpen) {
            VStack(spacing: 11) {
                tile
                Text(name)
                    .font(MadariFont.medium(15))
                    .foregroundStyle(.white.opacity(isAdd ? 0.82 : 1))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .shadow(color: .black.opacity(0.6), radius: 6, y: 1)
                    .frame(maxWidth: Self.side)
            }
        }
        .buttonStyle(ProfileTileStyle())
        .contextMenu {
            if let onEdit, profile != nil {
                Button { onEdit() } label: { Label("Edit profile", systemImage: "pencil") }
            }
            if let onDelete, profile != nil {
                Button(role: .destructive) { onDelete() } label: {
                    Label("Delete profile", systemImage: "trash")
                }
            }
        }
        .accessibilityLabel(
            [name,
             isKids ? "Kids" : nil,
             isProtected ? "PIN protected" : nil]
                .compactMap { $0 }.joined(separator: ", ")
        )
    }

    private var tile: some View {
        ZStack {
            if isAdd {
                RoundedRectangle(cornerRadius: Self.corner, style: .continuous)
                    .fill(Color.white.opacity(0.06))
                RoundedRectangle(cornerRadius: Self.corner, style: .continuous)
                    .strokeBorder(
                        Color.white.opacity(0.28),
                        style: StrokeStyle(lineWidth: 1.5, dash: [6, 6])
                    )
                VStack(spacing: 6) {
                    Image(systemName: "plus")
                        .font(.system(size: 24, weight: .medium))
                    Text("Add").font(MadariFont.medium(12))
                }
                .foregroundStyle(.white.opacity(0.8))
            } else {
                RoundedRectangle(cornerRadius: Self.corner, style: .continuous)
                    .fill(LinearGradient(colors: [tint.0, tint.1], startPoint: .topLeading, endPoint: .bottomTrailing))
                // The avatar fills the tile; the initial shows until it decodes and stays
                // as the fallback if it never does.
                Text(String(name.prefix(1)).uppercased())
                    .font(MadariFont.medium(38))
                    .foregroundStyle(.white.opacity(0.92))
                if let avatarURL {
                    Artwork(url: avatarURL, progressScale: 0.7)
                        .clipShape(RoundedRectangle(cornerRadius: Self.corner, style: .continuous))
                }
                // A soft sheen so the tile reads as a surface, not a swatch.
                RoundedRectangle(cornerRadius: Self.corner, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [.white.opacity(0.14), .clear, .black.opacity(0.22)],
                            startPoint: .topLeading, endPoint: .bottomTrailing
                        )
                    )
                badges
            }
        }
        .frame(width: Self.side, height: Self.side)
        .clipShape(RoundedRectangle(cornerRadius: Self.corner, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: Self.corner, style: .continuous)
                .stroke(Color.white.opacity(isAdd ? 0 : 0.14), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.35), radius: 10, y: 5)
    }

    @ViewBuilder
    private var badges: some View {
        VStack {
            HStack {
                Spacer()
                if isKids {
                    Text("KIDS")
                        .font(MadariFont.semibold(9))
                        .tracking(0.8)
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 3)
                        .background(MadariColors.accent, in: Capsule())
                        .padding(7)
                }
            }
            Spacer()
            HStack {
                Spacer()
                if isProtected {
                    Image(systemName: "lock.fill")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.white)
                        .padding(6)
                        .background(.black.opacity(0.45), in: Circle())
                        .padding(7)
                }
            }
        }
    }

    private var tint: (Color, Color) {
        guard let profile else { return (MadariColors.panel, MadariColors.panel) }
        let hash = profile.text("id").unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0x7FFF_FFFF }
        return Self.palette[hash % Self.palette.count]
    }
}

/// Presses a tile without the default button dimming, which reads badly over artwork.
private struct ProfileTileStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .animation(.spring(response: 0.25, dampingFraction: 0.7), value: configuration.isPressed)
    }
}

/// PIN entry for a protected profile.
private struct PinPrompt: View {
    let profile: JSONValue
    let onSubmit: (String) -> Void

    @State private var pin = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    MadariField(label: "PIN", prompt: "Enter your PIN", text: $pin, secret: true, keyboard: .numberPad)
                }
            }
            .navigationTitle(profile.text("name"))
            .inlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Continue") { onSubmit(pin) }
                        .disabled(pin.isEmpty)
                }
            }
        }
    }
}

/// Creates or edits a profile, including the avatar picker.
private struct ProfileEditor: View {
    enum Mode {
        case create(guardian: JSONValue?)
        case edit(JSONValue)

        var title: String {
            switch self {
            case .create: "New profile"
            case .edit: "Edit profile"
            }
        }
    }

    let mode: Mode
    let avatars: [JSONValue]

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var pin = ""
    @State private var guardianPin = ""
    @State private var kids = false
    @State private var avatar = ""
    @State private var choosingImage = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    // The row shows the current choice; the grid lives in its own
                    // dialog, the way the TV client separates the field from its
                    // avatar picker.
                    AvatarField(
                        avatars: avatars,
                        selected: avatar,
                        name: name,
                        onChoose: { choosingImage = true }
                    )
                }
                if case .create(let guardian) = mode, let guardian, guardian.boolean("pin_protected") {
                    Section("Authorization") {
                        MadariField(
                            label: "PIN for \(guardian.text("name"))",
                            prompt: "Required to add a profile",
                            text: $guardianPin,
                            secret: true,
                            keyboard: .numberPad
                        )
                    }
                }
                Section("Profile") {
                    MadariField(label: "Name", prompt: "Profile name", text: $name)
                    MadariField(
                        label: isEditing ? "New PIN (leave empty to keep)" : "PIN (optional)",
                        prompt: isEditing ? "Leave empty to keep" : "4–8 digits",
                        text: $pin,
                        secret: true,
                        keyboard: .numberPad
                    )
                    if case .create(let guardian) = mode, guardian != nil {
                        MadariToggle(
                            label: "Kids profile",
                            detail: "Content comes from the addons you choose; there is no automatic age filter.",
                            isOn: $kids
                        )
                    }
                }
                if case .edit(let profile) = mode, profile.boolean("pin_protected") {
                    Section("Current PIN") {
                        MadariField(
                            label: "Current PIN", prompt: "Required to save changes",
                            text: $guardianPin, secret: true, keyboard: .numberPad
                        )
                    }
                }
            }
            .navigationTitle(mode.title)
            .inlineNavigationTitle()
            .sheet(isPresented: $choosingImage) {
                AvatarPicker(avatars: avatars, selected: avatar, name: name) { chosen in
                    avatar = chosen
                    choosingImage = false
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .task {
                if case .edit(let profile) = mode {
                    name = profile.text("name")
                    avatar = profile.text("avatar")
                } else if avatar.isEmpty {
                    avatar = avatars.first?.text("id") ?? ""
                }
            }
        }
    }

    private var isEditing: Bool {
        if case .edit = mode { return true }
        return false
    }

    private func save() {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        let chosen = avatar
        let pinValue = pin
        let guardianValue = guardianPin
        let kidsValue = kids
        let currentMode = mode
        dismiss()
        Task {
            switch currentMode {
            case .create(let guardian):
                if let guardian {
                    await model.addProfile(
                        adult: guardian, adultPin: guardianValue, name: trimmed,
                        pin: pinValue, kids: kidsValue, avatar: chosen
                    )
                } else {
                    await model.createProfile(name: trimmed, pin: pinValue, kids: false, avatar: chosen)
                }
            case .edit(let profile):
                await model.updateProfile(
                    profile: profile, currentPin: guardianValue,
                    name: trimmed, pin: pinValue, avatar: chosen
                )
            }
        }
    }
}

/// The avatar grid from `state.profileAvatars`.
///
/// A grid rather than a horizontal `ScrollView`: a horizontal scroll view reports no
/// usable height inside a `Form` row, which left the tiles invisible.
/// The current profile image, with a button that opens the picker dialog.
///
/// The TV client shows a preview here and a separate `AvatarPicker` dialog; a grid of
/// 70-odd tiles inside a `Form` row is squashed and hard to scroll, so the same split
/// applies on iOS.
private struct AvatarField: View {
    let avatars: [JSONValue]
    let selected: String
    let name: String
    let onChoose: () -> Void

    private var selectedURL: String {
        avatars.first { $0.text("id") == selected }?.text("url") ?? ""
    }

    var body: some View {
        HStack(spacing: 14) {
            AvatarTile(url: selectedURL, letter: name, isSelected: false)
                .frame(width: 60, height: 60)
            VStack(alignment: .leading, spacing: 4) {
                Text(selected.isEmpty ? "No image chosen" : selected.replacingOccurrences(of: ".webp", with: ""))
                    .madariBody()
                Text(avatars.isEmpty
                     ? "The profile image catalog could not be loaded."
                     : "\(avatars.count) images available")
                    .madariLabel()
                    .foregroundStyle(MadariColors.muted)
            }
            Spacer()
            Button("Choose") { onChoose() }
                .buttonStyle(QuietButton())
                .frame(width: 100)
                .disabled(avatars.isEmpty)
        }
    }
}

/// The avatar grid, presented as its own dialog.
private struct AvatarPicker: View {
    let avatars: [JSONValue]
    let selected: String
    let name: String
    let onPick: (String) -> Void

    @Environment(\.dismiss) private var dismiss

    private let columns = [GridItem(.adaptive(minimum: 64), spacing: 12)]

    var body: some View {
        NavigationStack {
            Group {
                if avatars.isEmpty {
                    VStack(spacing: 10) {
                        Text("No profile images").madariHeading()
                        Text("The catalog is published by madari.media and could not be reached. Check the connection and try again.")
                            .madariBody()
                            .foregroundStyle(MadariColors.muted)
                            .multilineTextAlignment(.center)
                    }
                    .padding(28)
                } else {
                    ScrollView {
                        LazyVGrid(columns: columns, spacing: 12) {
                            ForEach(avatars, id: \.itemID) { entry in
                                let id = entry.text("id")
                                Button {
                                    onPick(id)
                                    dismiss()
                                } label: {
                                    AvatarTile(
                                        url: entry.text("url"),
                                        letter: name,
                                        isSelected: selected == id
                                    )
                                }
                                .buttonStyle(.plain)
                                .accessibilityLabel(entry.text("name").nilIfEmpty ?? id)
                            }
                        }
                        .padding(20)
                    }
                }
            }
            .navigationTitle("Profile image")
            .inlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}

/// One avatar, falling back to the profile initial while it decodes or when it cannot
/// be fetched at all.
private struct AvatarTile: View {
    let url: String
    let letter: String
    let isSelected: Bool

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10).fill(MadariColors.panel)
            Text(String(letter.prefix(1)).uppercased())
                .font(MadariFont.medium(20))
                .foregroundStyle(MadariColors.muted)
            // The shared loader, so a 72-tile grid does not open 72 connections.
            RemoteImage(url: url, contentMode: .fill, progressScale: 0.55)
                .frame(width: 54, height: 54)
                .clipShape(RoundedRectangle(cornerRadius: 10))
        }
        .frame(width: 54, height: 54)
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(isSelected ? MadariColors.accent : Color.white.opacity(0.12),
                        lineWidth: isSelected ? 2 : 1)
        )
        .clipped()
    }
}
