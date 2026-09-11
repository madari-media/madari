// swift-tools-version: 6.0
import PackageDescription

/// Builds the macOS app.
///
/// This is the macOS counterpart of the builder package xtool generates for iOS: an
/// executable target that depends on the app's library product and has no sources of its
/// own, so the entry point is the library's `@main` App. xtool only targets iOS, which is
/// why this package exists rather than a flag on the existing one.
///
/// scripts/build-macos.sh builds this and assembles the .app around it.
let package = Package(
    name: "Madari-MacOS-Builder",
    platforms: [.macOS("14.0")],
    dependencies: [
        .package(name: "RootPackage", path: "../ios"),
    ],
    targets: [
        .executableTarget(
            name: "Madari-App",
            dependencies: [
                .product(name: "Madari", package: "RootPackage"),
            ],
            linkerSettings: [
                // The vendored libmpv frameworks are embedded into Contents/Frameworks,
                // which is what this points at. Their install names are @rpath/…, so the
                // rpath is what lets them resolve at launch. xtool adds the iOS equivalent
                // (@executable_path/Frameworks) itself, because on iOS the app directory
                // is the Frameworks directory.
                .unsafeFlags([
                    "-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks",
                ]),
            ]
        )
    ]
)
