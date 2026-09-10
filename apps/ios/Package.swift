// swift-tools-version: 6.0
import Foundation
import PackageDescription

// xtool builds this package as a dependency of a generated builder package, and
// that builder performs the final link from its own directory. Linker paths must
// therefore be absolute; a relative `-Lnative` resolves against the builder.
let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path

/// The deployment target. Also passed to the linker below, so the two cannot drift.
let minimumIOSVersion = "17.0"

/// Tells the linker which SDK this app was built against.
///
/// iOS decides between the current design (Liquid Glass) and the pre-26
/// compatibility appearance from the SDK version recorded in the Mach-O's
/// `LC_BUILD_VERSION`. clang passes `-platform_version` itself on macOS; on this
/// Linux cross toolchain it does not, and ld64.lld then falls back to recording the
/// deployment target as the SDK version, which silently opts the app out of the
/// current design. Reading the version from the SDK keeps it honest when the
/// installed SDK changes.
func iosPlatformVersionFlags() -> [LinkerSetting] {
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
let mpvTargets: [Target] = mpvLibraries.map {
    .binaryTarget(name: $0, path: "native/mpv/\($0).xcframework")
}

let package = Package(
    name: "Madari",
    platforms: [.iOS(minimumIOSVersion)],
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
                .unsafeFlags(["-L\(packageDirectory)/native"]),
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
                // libmpv's render API is OpenGL ES, which Apple deprecated in iOS 12.
                // It is still the API this build of mpv targets, and the alternative
                // would be a Metal/libplacebo build that upstream does not support, so
                // the deprecation is expected rather than a problem to fix.
                .unsafeFlags(["-Xcc", "-DGLES_SILENCE_DEPRECATION"])
            ],
            linkerSettings: iosPlatformVersionFlags()
        )
    ]
)
