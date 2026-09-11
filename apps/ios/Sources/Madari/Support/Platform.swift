import Foundation
import SwiftUI

#if os(iOS)
import UIKit
/// The platform's bitmap image type. The two platforms name theirs differently and
/// SwiftUI takes a different initialiser per platform, so the app talks about
/// `PlatformImage` and the two shims below bridge to SwiftUI.
typealias PlatformImage = UIImage
typealias PlatformFont = UIFont
typealias PlatformFontDescriptor = UIFontDescriptor
#else
import AppKit
typealias PlatformImage = NSImage
typealias PlatformFont = NSFont
typealias PlatformFontDescriptor = NSFontDescriptor
#endif

extension Image {
    /// `Image(uiImage:)` and `Image(nsImage:)` are separate initialisers, and only one
    /// of them exists per platform, so a `#if` cannot be avoided. It is confined here
    /// rather than repeated at every call site.
    init(platformImage: PlatformImage) {
        #if os(iOS)
        self.init(uiImage: platformImage)
        #else
        self.init(nsImage: platformImage)
        #endif
    }
}

extension View {
    /// `navigationBarTitleDisplayMode` only exists on iOS. A Mac window has a title bar
    /// and no inline mode, so the modifier is a no-op there.
    func inlineNavigationTitle() -> some View {
        #if os(iOS)
        navigationBarTitleDisplayMode(.inline)
        #else
        self
        #endif
    }
}

/// Opens a URL outside the app: a browser for links, System Settings for the app's own
/// settings page. iOS routes this through `UIApplication`, macOS through `NSWorkspace`.
@MainActor
func openExternally(_ url: URL) {
    #if os(iOS)
    UIApplication.shared.open(url)
    #else
    NSWorkspace.shared.open(url)
    #endif
}

#if os(macOS)
/// iOS re-decodes on rotation; a Mac window can resize at any time, so images are
/// re-rendered from the same source instead.
extension PlatformImage {
    /// `NSImage.size` is in points and can be zero for an image whose representation
    /// has not been decoded yet, which would collapse a layout sized from it.
    var hasUsableSize: Bool { size.width > 0 && size.height > 0 }
}
#endif
// MARK: - SwiftUI API that exists on only one platform

extension ToolbarItemPlacement {
    /// iOS splits a navigation bar into leading and trailing slots. A Mac toolbar is a
    /// single row, so both of these map onto it.
    static var madariLeading: ToolbarItemPlacement {
        #if os(iOS)
        .topBarLeading
        #else
        .navigation
        #endif
    }

    static var madariTrailing: ToolbarItemPlacement {
        #if os(iOS)
        .topBarTrailing
        #else
        .primaryAction
        #endif
    }
}

/// Which keyboard to start with. iOS has `UIKeyboardType`; a Mac has a physical keyboard
/// and no equivalent, so this exists only to keep the call sites shared.
enum MadariKeyboard {
    case standard
    case numberPad
    case URL

    #if os(iOS)
    var uiKeyboardType: UIKeyboardType {
        switch self {
        case .standard: .default
        case .numberPad: .numberPad
        case .URL: .URL
        }
    }
    #endif
}

/// The sheet heights iOS can be asked for. A Mac sheet is a window, so there is nothing to
/// constrain.
enum SheetDetent {
    case medium
    case large
    case height(CGFloat)
}

extension View {
    /// `textInputAutocapitalization` is iOS only: a Mac never autocapitalises.
    func noAutocapitalization() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.never)
        #else
        self
        #endif
    }

    func keyboardType(_ keyboard: MadariKeyboard) -> some View {
        #if os(iOS)
        self.keyboardType(keyboard.uiKeyboardType)
        #else
        self
        #endif
    }

    /// `statusBarHidden` is iOS only; a Mac window has no status bar.
    func hideStatusBar() -> some View {
        #if os(iOS)
        statusBarHidden()
        #else
        self
        #endif
    }

    func sheetDetents(_ detents: [SheetDetent]) -> some View {
        #if os(iOS)
        presentationDetents(Set(detents.map { detent in
            switch detent {
            case .medium: .medium
            case .large: .large
            case .height(let height): .height(height)
            }
        }))
        #else
        self
        #endif
    }

    /// A full-screen modal has no meaning in a windowed app, so macOS presents a sheet.
    func madariFullScreenCover<Item: Identifiable, Content: View>(
        item: Binding<Item?>,
        @ViewBuilder content: @escaping (Item) -> Content
    ) -> some View {
        #if os(iOS)
        fullScreenCover(item: item, content: content)
        #else
        sheet(item: item, content: content)
        #endif
    }
}

/// `EditButton` is iOS only, and a Mac list has no edit mode: rows are edited in place
/// through a context menu. Declaring it here shadows SwiftUI's on iOS, so the call sites
/// read the same on both platforms while the iOS one still uses SwiftUI's own button.
struct EditButton: View {
    var body: some View {
        #if os(iOS)
        SwiftUI.EditButton()
        #else
        EmptyView()
        #endif
    }
}

extension View {
    /// iOS places grouped rows in inset cards. A Mac list is inset already and has no
    /// `insetGrouped`, so the equivalent treatment there is `.inset`.
    func madariGroupedList() -> some View {
        #if os(iOS)
        listStyle(.insetGrouped)
        #else
        listStyle(.inset)
        #endif
    }
}
