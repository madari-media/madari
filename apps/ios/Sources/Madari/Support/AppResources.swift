import Foundation

/// Finds the resources this app ships with, without SwiftPM's `Bundle.module`.
///
/// `Bundle.module` is a generated accessor that calls `fatalError` when it cannot find the
/// resource bundle it was compiled to expect, and it does so inside its own initialiser —
/// before any caller can react or recover. On iOS the accessor finds its bundle beside the
/// executable. On macOS it does not: `Bundle(url:)` refuses a directory that contains only
/// resources, and the bundle SwiftPM produces has no `Info.plist`. The result was the whole
/// app dying with SIGTRAP on the main thread inside `App.init()`, the first time the font was
/// looked up, on a build that had already launched past dyld.
///
/// This looks in the places that exist on both platforms, and returns nil rather than
/// trapping, so a missing resource degrades to the system font instead of taking the app down.
enum AppResources {
    /// `Bundle(for:)` needs a class to identify the image, and this one exists only for that.
    private final class Finder {}

    static func url(forResource name: String, withExtension ext: String) -> URL? {
        // Loose in the resource directory first: that is where the macOS build puts these
        // files, so the common case needs no bundle parsing at all.
        let directories = [
            Bundle.main.resourceURL,
            Bundle.main.bundleURL,
            Bundle(for: Finder.self).resourceURL,
            Bundle(for: Finder.self).bundleURL,
        ]
        for directory in directories {
            guard let directory else { continue }
            let direct = directory.appendingPathComponent("\(name).\(ext)")
            if FileManager.default.fileExists(atPath: direct.path) { return direct }
            // Otherwise inside the SwiftPM resource bundle, which is where they are when
            // SwiftPM embeds them for iOS.
            for nested in ["Madari_Madari.bundle", "Madari.bundle"] {
                guard let bundle = Bundle(url: directory.appendingPathComponent(nested)),
                      let found = bundle.url(forResource: name, withExtension: ext)
                else { continue }
                return found
            }
        }
        return nil
    }
}
