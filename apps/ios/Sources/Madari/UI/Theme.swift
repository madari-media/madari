import CoreText
import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// The Madari palette, shared with the Linux and TV clients.
enum MadariColors {
    static let background = Color(red: 0x0B / 255, green: 0x0C / 255, blue: 0x0F / 255)
    static let panel = Color(red: 0x1C / 255, green: 0x1E / 255, blue: 0x23 / 255)
    static let accent = Color(red: 0xF0 / 255, green: 0x5B / 255, blue: 0x64 / 255)
    static let muted = Color(red: 0xAA / 255, green: 0xAD / 255, blue: 0xB5 / 255)
}

/// DM Sans, the bundled brand typeface.
///
/// The file is a variable font. Its name table reports family `DM Sans 9pt`,
/// PostScript `DMSans-9ptRegular`, and typographic family `DM Sans`, with the
/// weights as named instances of that family. `Font.custom` accepts a family or a
/// PostScript name and falls back to the system font *silently* when neither
/// matches, so each face is resolved through the platform font API and checked
/// before use. `register()` must run before the first view is built.
enum MadariFont {
    private static let family = "DM Sans"
    /// The file's own face, which always exists and is the last resort.
    private static let postScript = "DMSans-9ptRegular"
    private static let instanceFaces: [String: String] = [
        "regular": "Regular",
        "medium": "Medium",
        "semibold": "SemiBold",
        "bold": "Bold",
    ]

    private static let registration: Void = {
        guard let url = AppResources.url(forResource: "DMSans", withExtension: "ttf") else {
            DebugLog.write("DMSans.ttf missing from the resource bundle")
            return
        }
        var error: Unmanaged<CFError>?
        let registered = CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error)
        // Reported once at launch so a silent system-font fallback is visible in the
        // device log instead of only being noticeable by eye.
        let resolved = PlatformFont(name: postScript, size: 12)
        DebugLog.write("font registered=\(registered) face=\(resolved?.fontName ?? "none") family=\(resolved?.familyName ?? "none")")
    }()

    /// Registers the bundled face. A lazy `static let` keeps this a one-shot without
    /// mutable global state, which strict concurrency rejects.
    static func register() {
        _ = registration
    }

    static func regular(_ size: CGFloat) -> Font { font("regular", size, .regular) }
    static func medium(_ size: CGFloat) -> Font { font("medium", size, .medium) }
    static func semibold(_ size: CGFloat) -> Font { font("semibold", size, .semibold) }
    static func bold(_ size: CGFloat) -> Font { font("bold", size, .bold) }

    private static func font(_ face: String, _ size: CGFloat, _ weight: Font.Weight) -> Font {
        // A named instance is the exact cut the designer drew.
        if let instance = instanceFaces[face],
           let descriptor = PlatformFontDescriptor(fontAttributes: [
               .family: family,
               .face: instance,
           ]) as PlatformFontDescriptor? {
            // Optional on macOS and non-optional on iOS, so it is declared as an optional
            // here and checked once for both.
            let candidate: PlatformFont? = PlatformFont(descriptor: descriptor, size: size)
            // `PlatformFont(descriptor:size:)` substitutes instead of failing, so the
            // resolved name is what distinguishes success from a fallback.
            if let candidate, candidate.fontName.hasPrefix("DMSans") {
                return Font(candidate)
            }
        }
        // Otherwise interpolate the variable axis from the one face we know exists.
        if let base = PlatformFont(name: postScript, size: size) {
            return Font(base).weight(weight)
        }
        if let byFamily = PlatformFont(name: family, size: size) {
            return Font(byFamily).weight(weight)
        }
        return .system(size: size, weight: weight)
    }
}

/// Semantic text styles, so screens do not each pick their own sizes.
extension Text {
    func madariTitle() -> Text { font(MadariFont.semibold(22)) }
    func madariHeading() -> Text { font(MadariFont.medium(18)) }
    func madariRowHeading() -> Text { font(MadariFont.semibold(17)) }
    func madariBody() -> Text { font(MadariFont.regular(15)) }
    func madariLabel() -> Text { font(MadariFont.regular(13)) }
}

/// Shared badge for type/episode/quality metadata.
struct MetaLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .madariLabel()
            .foregroundStyle(MadariColors.muted)
            .lineLimit(1)
    }
}

/// The app-wide surface: every screen sits on this background.
struct MadariScreen<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        ZStack {
            MadariColors.background.ignoresSafeArea()
            content
        }
        .tint(MadariColors.accent)
    }
}

/// A failure the user should read, with an optional retry.
struct ErrorCard: View {
    let message: String
    var retry: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Something went wrong").madariHeading()
            Text(message)
                .madariBody()
                .foregroundStyle(MadariColors.muted)
                .fixedSize(horizontal: false, vertical: true)
            if let retry {
                Button("Try again", action: retry)
                    .buttonStyle(ProminentButton())
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 12))
    }
}

/// The primary action button. Touch replaces the TV's focus ring.
struct ProminentButton: ButtonStyle {
    var tinted = true

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(MadariFont.semibold(14))
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .background(
                configuration.isPressed
                    ? MadariColors.accent.opacity(0.75)
                    : (tinted ? MadariColors.accent : MadariColors.panel),
                in: RoundedRectangle(cornerRadius: 9)
            )
            .foregroundStyle(tinted ? Color.white : Color.primary)
    }
}

/// The secondary action button used beside `ProminentButton`.
struct QuietButton: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(MadariFont.medium(14))
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .background(
                configuration.isPressed ? MadariColors.panel.opacity(0.6) : MadariColors.panel,
                in: RoundedRectangle(cornerRadius: 9)
            )
            .foregroundStyle(Color.primary)
    }
}

/// Labelled text field with the shared panel treatment.
struct MadariField: View {
    let label: String
    let prompt: String
    @Binding var text: String
    var secret = false
    var keyboard: MadariKeyboard = .standard

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(label).madariLabel().foregroundStyle(MadariColors.muted)
            Group {
                if secret {
                    SecureField(prompt, text: $text)
                } else {
                    TextField(prompt, text: $text)
                }
            }
            .noAutocapitalization()
            .autocorrectionDisabled()
            .keyboardType(keyboard)
            .font(MadariFont.regular(16))
            .padding(12)
            .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 9))
        }
    }
}

/// A labelled switch row, used by the playback and player preference screens.
struct MadariToggle: View {
    let label: String
    var detail: String?
    @Binding var isOn: Bool

    var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: 3) {
                Text(label).madariBody()
                if let detail {
                    Text(detail).madariLabel().foregroundStyle(MadariColors.muted)
                }
            }
        }
        .tint(MadariColors.accent)
    }
}
