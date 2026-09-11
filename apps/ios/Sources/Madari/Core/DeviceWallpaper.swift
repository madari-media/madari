#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// The profile wallpaper, chosen for the device actually in use.
///
/// The CDN publishes one image per device profile:
///
/// ```text
/// https://downloads.madari.media/backgrounds/{format}/{device}.{ext}
/// ```
///
/// Serving one desktop image to every client was the direct cause of a layout bug on
/// iOS: `desktop_fhd` is 3840×2160, so aspect-filling it into a portrait phone screen
/// scaled it ~4× wider than the screen, and every sibling view was pushed off it.
/// Choosing a portrait image for the device keeps the crop small and the decode cheap.
///
/// `@MainActor` because it reads the device idiom and the screen, both of which are
/// main-actor isolated on iOS and macOS alike.
@MainActor
enum DeviceWallpaper {
    private struct Device {
        let name: String
        /// Output pixels. The generator's scale factor is already applied, which is why
        /// the three desktop entries all land at 3840×2160.
        let width: Int
        let height: Int
    }

    private static let phones = [
        Device(name: "iphone_14_pro_max", width: 1290, height: 2796),
        Device(name: "iphone_14", width: 1170, height: 2532),
    ]

    private static let tablets = [
        Device(name: "ipad_pro_12.9", width: 2732, height: 2048),
        Device(name: "ipad_air", width: 2360, height: 1640),
        Device(name: "ipad_mini", width: 2266, height: 1488),
    ]

    private static let desktops = [
        Device(name: "desktop_4k", width: 3840, height: 2160),
        Device(name: "laptop", width: 3415, height: 1920),
    ]

    /// JPEG rather than WebP: every published device has one, and it removes any doubt
    /// about decoder support for a full-screen image. The profile avatars, which are
    /// WebP only, go through the same loader.
    private static let format = "jpeg"

    /// The closest published wallpaper for this device.
    static var url: String {
        "https://downloads.madari.media/backgrounds/\(format)/\(device).jpg"
    }

    /// The published name nearest the screen, matched within the device's own family.
    static var device: String {
        let candidates = family
        guard let pixels = screenPixels else { return candidates[0].name }
        // Compared as (long side, short side) so a rotation does not change the answer.
        let target = (max(pixels.width, pixels.height), min(pixels.width, pixels.height))
        return candidates
            .min { left, right in
                distance(left, to: target) < distance(right, to: target)
            }?
            .name ?? candidates[0].name
    }

    /// The family to match within. A Mac has no `userInterfaceIdiom`, and the landscape
    /// desktop art is exactly what its screen corresponds to.
    private static var family: [Device] {
        #if os(iOS)
        switch UIDevice.current.userInterfaceIdiom {
        case .pad: tablets
        case .phone: phones
        // A TV runs the same landscape art as a desktop.
        default: desktops
        }
        #else
        desktops
        #endif
    }

    private static func distance(_ device: Device, to target: (Int, Int)) -> Int {
        abs(max(device.width, device.height) - target.0)
            + abs(min(device.width, device.height) - target.1)
    }

    /// Native pixel size, which is what the published sizes are quoted in.
    private static var screenPixels: (width: Int, height: Int)? {
        #if os(iOS)
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let screen = scenes.first { $0.activationState == .foregroundActive }?.screen
            ?? scenes.first?.screen
        guard let screen else { return nil }
        let bounds = screen.nativeBounds
        return (Int(bounds.width), Int(bounds.height))
        #else
        // `NSScreen.frame` is in points, so the backing scale is what brings it to the
        // pixels the published sizes are quoted in.
        guard let screen = NSScreen.main else { return nil }
        let scale = screen.backingScaleFactor
        return (Int(screen.frame.width * scale), Int(screen.frame.height * scale))
        #endif
    }
}
