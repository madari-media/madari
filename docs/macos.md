# Madari for macOS

A native SwiftUI Mac app in `apps/macos`, built from the same Swift sources as the iOS
client. It talks to the same Rust core through the same UniFFI boundary, so profiles,
addons, catalogs, torrents and playback are the same code on every Apple platform.

The Mac app is for Apple Silicon and Intel: the released archive is a universal binary.

## Architecture

The app is one set of sources, not a fork. `apps/ios/Sources/Madari` holds every screen and
both players, and (outside a handful of platform shims) nothing in it knows which platform it
is running on.

What the Mac needs that iOS does not:

| Piece | iOS | macOS |
|---|---|---|
| Bundle assembly | `xtool dev build` | `scripts/build-macos.sh` |
| mpv video surface | `CAEAGLLayer` + OpenGL ES | `NSOpenGLView` + desktop GL |
| AVPlayer surface | `UIView` with an `AVPlayerLayer` | `NSView` with an `AVPlayerLayer` sublayer |
| Frameworks | `native/mpv` | `native/mpv-macos` |
| Rust target | `aarch64-apple-ios` | `{arm64,x86_64}-apple-darwin` |

Why the bundle is assembled by hand: xtool builds iOS apps and only iOS apps — its
subcommands are devices, install and dev, and it targets `arm64-apple-ios`. A Mac app needs
`Contents/MacOS/Madari`, the frameworks in `Contents/Frameworks`, and the SwiftPM resource
bundle in `Contents/Resources`, so `scripts/build-macos.sh` does that. The Swift side is
still built by SwiftPM, through a small builder package in `apps/macos` that mirrors the one
xtool generates for iOS: an executable target whose only source is a stub, because the entry
point is the library's `@main` App.

### Which platform is being built

`apps/ios/Package.swift` reads `MADARI_TARGET_PLATFORM`, defaulting to `ios`. It has to be
told rather than inferred from the host, because the host says nothing useful here: CI builds
the iOS app on a macOS runner, and the Mac app can be built from Linux.

That variable selects two things that would otherwise quietly break the build:

- **The libmpv frameworks.** Only the target platform's set is declared. SwiftPM validates
  every binary target in a package whether or not anything depends on it, so declaring both
  sets fails with "does not contain a binary artifact" as soon as one of them has not been
  downloaded.
- **The `-platform_version` linker flag.** It is iOS-specific and must not reach a macOS
  build; doing so produces `using sysroot for 'iPhoneOS' but targeting 'MacOSX'` and then
  `unable to load standard library for target 'arm64-apple-macosx14.0'`.

## Source layout

```text
apps/macos/
  Package.swift                     the builder package (executable target + rpath)
  Info.plist                        macOS bundle keys, version substituted at build time
  Sources/Madari-App/stub.c         the entry point comes from the library
apps/ios/
  Sources/Madari/                   all screens and both players, shared with iOS
  Sources/Madari/Support/Platform.swift
                                    the shims below, and nowhere else
  Sources/Madari/Player/MpvSurfaceMacOS.swift
                                    NSOpenGLView surface for libmpv
  native/mpv-macos/                 vendored macOS XCFrameworks (gitignored)
scripts/
  fetch-ios-mpv.sh                  vendors both platforms' frameworks
  build-macos.sh                    Rust core, Swift app, .app assembly, ad-hoc signing
  typecheck-macos.sh                checks the sources from Linux
  linux-shims/codesign              a no-op codesign so a Linux cross build can link
```

## Deliberate platform translations

`Support/Platform.swift` is the only file that knows about the differences:

| iOS | macOS | Why |
|---|---|---|
| `Bundle.module` fonts, `UIImage` | `NSImage` via `PlatformImage` | different bitmap types |
| `UIKeyboardType` | `MadariKeyboard`, ignored | a Mac has a physical keyboard |
| `.topBarLeading` / `.topBarTrailing` | `.navigation` / `.primaryAction` | a Mac toolbar is one row |
| `presentationDetents` | ignored | a Mac sheet is a window, nothing to constrain |
| `statusBarHidden` | ignored | no status bar |
| `EditButton` | renders nothing | a Mac list edits in place, there is no edit mode |
| `fullScreenCover` | `sheet` | a full-screen modal has no meaning in a windowed app |
| `.listStyle(.insetGrouped)` | `.listStyle(.inset)` | no grouped style on macOS |
| `UIDevice` / window scene | `NSScreen` + `backingScaleFactor` | wallpaper size matching |

The mpv system option `ao` is per platform for the same reason the frameworks are: the iOS
build compiles out every audio output except AudioUnit, and neither build can use the other's
setting.

## Build

Fetch the frameworks, then build. Both scripts work on a Mac; the first two also work from
Linux.

```bash
MADARI_MPV_PLATFORMS="ios macos" bash scripts/fetch-ios-mpv.sh
bash scripts/build-macos.sh
```

`build-macos.sh` cross-compiles the Rust core for `arm64` and `x86_64`, combines them with
`lipo`, builds the Swift app once per architecture for the same reason, assembles
`apps/macos/build/Madari.app`, and ad-hoc signs it. Overrides: `MADARI_MACOS_ARCHS`,
`MADARI_MACOS_MIN`, `MADARI_MACOS_CONFIG`, `MADARI_SKIP_WEB_BUILD=1`.

The UniFFI bindings are shared with iOS and committed, so they are not regenerated here. If
they are missing, run `scripts/build-ios-native.sh` once.

### Building from Linux

The Rust half cross-compiles cleanly. The Swift half is not reliable from Linux: xtool's
Darwin bundle ships both `info.json` (the legacy artifact-bundle form) and `swift-sdk.json`
(the newer Swift SDK form), SwiftPM registers it twice under the same id, and then picks
between the two arbitrarily — when it picks the legacy parse it applies an iOS or simulator
sysroot and the build dies before compiling anything. `build-macos.sh` retries only that
specific failure, and says so rather than pretending it is a code error.

For a dependable check from Linux, use:

```bash
bash scripts/typecheck-macos.sh
```

It drives `swiftc` directly with the macOS SDK and stdlib from the bundle, which is
deterministic, and type-checks every source file for `arm64-apple-macosx14.0`. It does not
link or assemble, so it catches porting errors and not bundle mistakes.

The real build runs on a macOS runner in CI (`.github/workflows/macos.yml`), where no Swift
SDK is involved at all and the toolchain is native.

## Validation

What has been checked, on the released archive:

- `lipo -info` reports `x86_64 arm64` — a universal binary.
- All 18 libmpv frameworks are embedded in `Contents/Frameworks`, and the executable has 18
  `@rpath` dependencies matching them.
- The executable carries an `@executable_path/../Frameworks` rpath, which is what makes those
  resolve, plus the Swift runtime rpath.
- `Contents/Resources/Madari_Madari.bundle` is present — the fonts and the logo live in it.
- `Info.plist` reports `media.madari.macos`, a minimum system version of 14.0, and the version
  from `version.txt`.
- `scripts/typecheck-macos.sh` is clean for the whole app.

What has not been checked: the app has not been launched. Nobody has seen it run, so the
first launch on a real Mac is the first real test of the render path.

## Current limits

- **The signature is ad-hoc.** Enough to launch locally, not enough to distribute without a
  warning: a downloaded `.app` is quarantined by Gatekeeper, so the first open needs
  right-click → Open, or a real Developer ID signature and notarisation. Treat this as
  required work before the Mac app is advertised, not as polish.
- **No Metal and no HDR on the mpv backend**, for the same reason as iOS: this libmpv build
  targets OpenGL, and the alternative is a libplacebo/Vulkan build upstream does not support.
  HDR still works through the AVPlayer backend.
- **`MPV_RENDER_PARAM_FLIP_Y` is set to 0** for the macOS surface, because the window's own
  framebuffer has OpenGL's bottom-left origin, unlike the iOS layer. That is reasoned rather
  than observed; if the picture comes out inverted, this is the value to flip.
- **No Mac-specific shell integration.** No menu bar commands, no window state restoration,
  no `Services` entries: the app runs, and the platform niceties are not written.
- **Video output is not verified on Intel.** The binary is universal, but only the
  architecture has been checked, not playback on an Intel Mac.
