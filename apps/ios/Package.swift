// swift-tools-version: 6.0
import Foundation
import PackageDescription

// xtool builds this package as a dependency of a generated builder package, and
// that builder performs the final link from its own directory. Linker paths must
// therefore be absolute; a relative `-Lnative` resolves against the builder.
let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path

/// The deployment target. Also passed to the linker below, so the two cannot drift.
let minimumIOSVersion = "17.0"

/// The Mac app. 14 is the oldest release with the SwiftUI APIs these screens already use.
let minimumMacOSVersion = "14.0"

/// Which app is being built. Set by the build scripts, because a manifest cannot ask
/// SwiftPM for the target platform: CI builds the iOS app on a macOS runner, and the Mac app
/// is built from Linux, so the host says nothing useful. iOS is the default, so a plain
/// `swift build` or `xtool dev build` behaves exactly as it did before.
let targetPlatform = ProcessInfo.processInfo.environment["MADARI_TARGET_PLATFORM"] ?? "ios"

/// Tells the linker which SDK this app was built against.
///
/// iOS decides between the current design (Liquid Glass) and the pre-26
/// compatibility appearance from the SDK version recorded in the Mach-O's
/// `LC_BUILD_VERSION`. clang passes `-platform_version` itself on macOS; on this
/// Linux cross toolchain it does not, and ld64.lld then falls back to recording the
/// deployment target as the SDK version, which silently opts the app out of the
/// current design. Reading the version from the SDK keeps it honest when the
/// installed SDK changes.
///
/// Getting the platform wrong here is what "using sysroot for 'iPhoneOS' but targeting
/// 'MacOSX'" means: iOS flags passed to a macOS build.
func platformVersionFlags() -> [LinkerSetting] {
    guard targetPlatform == "ios" else {
        // The Mac app records its own platform version through the toolchain.
        return []
    }
    #if os(macOS)
    // Xcode and the macOS toolchain supply this, and a duplicate is an error.
    return []
    #else
    let environment = ProcessInfo.processInfo.environment
    let bundle = environment["MADARI_DARWIN_BUNDLE"]
        ?? "\(environment["HOME"] ?? "")/.config/swiftpm/swift-sdks/darwin.artifactbundle"
    let settings = "\(bundle)/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk/SDKSettings.plist"
    var sdkVersion = "26.0"
    if let data = FileManager.default.contents(atPath: settings),
       let plist = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any],
       let version = plist["Version"] as? String {
        sdkVersion = version
    }
    // SwiftPM forwards a dependency's linker settings to the final link verbatim,
    // so the driver needs explicit `-Xlinker` forwarding for this one.
    return [.unsafeFlags([
        "-Xlinker", "-platform_version",
        "-Xlinker", "ios",
        "-Xlinker", minimumIOSVersion,
        "-Xlinker", sdkVersion,
    ])]
    #endif
}

/// The libmpv stack, vendored as prebuilt XCFrameworks by scripts/fetch-ios-mpv.sh
/// (see docs/ios.md). AVFoundation cannot open Matroska, WebM or AVI, so these
/// provide the fallback playback backend for everything AVPlayer refuses.
///
/// They are dynamic frameworks, so xtool embeds and signs all of them into the app
/// bundle and adds the `@executable_path/Frameworks` rpath itself. The list is the
/// transitive closure of `Mpv.framework`, and every entry has to be a dependency or
/// the linker will not find its symbols.
let mpvLibraries = [
    "Ass",
    "Avcodec",
    "Avfilter",
    "Avformat",
    "Avutil",
    "Dav1d",
    "Freetype",
    "Fribidi",
    "Harfbuzz",
    "Mbedcrypto",
    "Mbedtls",
    "Mbedx509",
    "Mpv",
    "Png16",
    "Swresample",
    "Swscale",
    "Uchardet",
    "Xml2",
]

/// Absent until scripts/fetch-ios-mpv.sh has run, exactly like native/libmadari_ios.a.
///
/// Only one platform's set is declared, not both. SwiftPM validates every binary target in
/// the package whether or not anything depends on it, so declaring the other platform's
/// frameworks fails the build with "does not contain a binary artifact" whenever they have
/// not been downloaded. Which set that is comes from the same variable as the platform
/// flags above.
let mpvTargets: [Target] = mpvLibraries.map {
    let directory = targetPlatform == "macos" ? "native/mpv-macos" : "native/mpv"
    return .binaryTarget(name: $0, path: "\(directory)/\($0).xcframework")
}

let package = Package(
    name: "Madari",
    platforms: [.iOS(minimumIOSVersion), .macOS(minimumMacOSVersion)],
    products: [
        // An xtool project contains exactly one library product: the app itself.
        .library(name: "Madari", targets: ["Madari"])
    ],
    targets: mpvTargets + [
        // Raw symbols from libmadari_ios.a, as emitted by `uniffi-bindgen`.
        .target(
            name: "madari_iosFFI",
            path: "Sources/madari_iosFFI",
            publicHeadersPath: "."
        ),
        // Generated Swift bindings. Committed; refresh with scripts/build-ios-native.sh.
        .target(
            name: "MadariCore",
            dependencies: ["madari_iosFFI"],
            path: "Sources/MadariCore",
            linkerSettings: [
                // The Rust core is a static library built per platform; the iOS one is the
                // cross build, the macOS one is a universal binary from the same script.
                .unsafeFlags(["-L\(packageDirectory)/native"], .when(platforms: [.iOS])),
                .unsafeFlags(["-L\(packageDirectory)/native/macos"], .when(platforms: [.macOS])),
                .linkedLibrary("madari_ios"),
                // Linked by crates the core pulls in, not by Swift:
                // rustls-platform-verifier verifies through Security.framework,
                // rusqlite and librqbit use CoreFoundation and SystemConfiguration,
                // aws-lc-sys is C++ and needs the runtime, and librqbit uses zlib.
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("SystemConfiguration"),
                .linkedLibrary("c++"),
                .linkedLibrary("z")
            ]
        ),
        .target(
            name: "Madari",
            dependencies: ["MadariCore"] + mpvLibraries.map { .target(name: $0) },
            path: "Sources/Madari",
            resources: [.process("Resources")],
            swiftSettings: [
                // libmpv's render API is OpenGL: ES on iOS, desktop GL on macOS. Apple has
                // deprecated both, and both are still the API this build of mpv targets;
                // the alternative would be a Metal/libplacebo build that upstream does not
                // support. Each define is inert on the platform it does not name, so both
                // are set rather than trying to guess the target from the host.
                .unsafeFlags([
                    "-Xcc", "-DGLES_SILENCE_DEPRECATION",
                    "-Xcc", "-DGL_SILENCE_DEPRECATION",
                ])
            ],
            linkerSettings: platformVersionFlags()
        )
    ]
)
