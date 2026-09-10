# Madari for iOS

The iOS client is a native SwiftUI application in `apps/ios`. It uses the Linux app's
cinematic hero, poster rows, DM Sans, profile flows and addon-driven browsing as its
reference, and drives the same Rust core through a UniFFI boundary. There is no
WebView, no companion server requirement, and no duplicate Swift media core.

iOS is built and deployed **from Linux with [xtool](https://github.com/saagarjha/xtool)**
and a Darwin Swift SDK — no Xcode and no Mac. Deployment target is iOS 17.

## Architecture

```text
SwiftUI screens
    → AppModel (calls on a background queue)
        → NativeCore / MadariCore (UniFFI)
            → madari-ios (UniFFI boundary)
                → madari-tv (shared mobile boundary: Bridge and web settings)
                    → madari-core (catalogs, metadata, library, resume, episode policy)
                    → madari-native (profiles, PINs, HTTP, SQLite, internal media)
                    → madari-media (embedded torrent engine)

AVPlayer                                              libmpv (vendored, see below)
    → HTTP / HLS data sources                             → vo=libmpv rendered into an EAGL surface
    → TorrentResourceLoader → AVAssetResourceLoader       → mpv_stream_cb_add_ro → NativeCore.readMedia
      → NativeCore.readMedia
```

Playback has two backends. AVFoundation decodes MP4/MOV/M4V and HLS and nothing else,
so Matroska, WebM, AVI and MPEG-TS are played by libmpv instead. Both read torrent
bytes through the same Rust reader, and either backend can hand a source to the other.

`madari-tv` owns one Tokio runtime, the profile session and a checked registry of
media readers; it is a plain library. Each client adds a thin FFI crate of its own:
`madari-android` is the JNI `cdylib` and `madari-ios` the UniFFI `staticlib`. The
`cdylib` is deliberately not declared on the shared crate — cargo builds every
declared crate type of an in-workspace path dependency, and `ld64.lld` cannot produce
an iOS dylib.

The profile database and downloaded media live in the app's Application Support
directory. Profiles are local to this device and do not synchronize with a Linux
installation. PINs and kids-mode enforcement come from the existing native profile
implementation.

Torrent bytes stay in the process. `AVAssetResourceLoader` asks for byte ranges and
`TorrentResourceLoader` answers them from the Rust reader; nothing is served over
loopback HTTP.

## Source layout

`apps/ios/Sources/Madari` is organized by layer and feature:

```text
MadariApp.swift        app entry point, font registration
Core/                  JSONValue, NativeCore (UniFFI wrapper), Models, LocalNetwork
State/                 AppState, AppModel (the view model), Snapshot (derived views)
UI/                    Theme (palette, DM Sans, shared chrome), Glyph (SF Symbols)
UI/Components/         RemoteImage (shared loader), cards, rows, hero, loading states
Features/              one screen per area: profiles, home, search, explore, library,
                       calendar, details, sources, settings
Player/                AVPlayer host and controls, TorrentResourceLoader,
                       libmpv engine (MpvEngine), libmpv video output (MpvSurface)
Sources/madari_iosFFI/ generated C module (committed)
Sources/MadariCore/    generated Swift bindings (committed)
```

Dependencies point inward: `Features/*` and `UI/*` depend on `State` and `Core`, while
`Core` depends on nothing above it. Screens never call the generated bindings
directly; every native operation goes through `NativeCore`.

JSON never becomes a hundred mirrored Swift types. `JSONValue` is a `Sendable` enum
that preserves every addon extension field across the boundary, the same role
`JSONObject` plays in Kotlin.

## Implemented flows

- Create adult or kids profiles with a bundled avatar catalog; enter a protected
  profile; guardian authorization for settings and leaving kids mode. New kids
  profiles require a PIN-protected adult.
- Home hero, addon catalog rows, Continue watching, saved list and title details.
- Search declared searchable catalogs; browse catalogs and supply declared filters,
  including option lists; explicit pagination in 100-item increments with
  deduplication and repeated/short-page termination.
- Shared-core metadata resolution, season/episode lists, source provenance and
  partial-addon failure messages.
- Native HTTP and HLS playback through AVPlayer, pause/seek, playback speed, picture
  size, audio and subtitle selection, and embedded-torrent playback through
  `AVAssetResourceLoader` with live torrent statistics.
- Shared resume policy, position persistence, and manual next-episode selection using
  the core's availability and episode-order rules.
- Per-profile playback preferences (subtitles by default, ordered audio/subtitle
  language priority, SDH/forced/audio-description/commentary), applied through the
  core's own `preferred_tracks` adapter so the same rules as the TV client decide
  which AVFoundation track is selected.
- Per-profile Trakt device login with sync and disconnect.
- Create, rename and **delete** profiles. Deleting removes the profile's library,
  progress, addon links and Trakt connection; the core refuses a profile that still
  guards a kids profile and names them instead of cascading.
- Add/remove/enable/disable/reorder addons, with the curated addons offered as a
  recommended list, reconfiguring a shared installation, and linking an installation
  to another profile.
- Settings is grouped into destinations (profile, tracks and subtitles, player
  display, Trakt, addons, browser settings) rather than one long page.
- Calendar for saved titles, using the core's addon-declared calendar support.
- Device-wide player display defaults (subtitle size, picture size, default speed).
- The core's LAN web settings server, started and stopped from Settings, with the
  address and pairing code shown.

A new profile is offered two curated addons once: Cinemeta and OpenSubtitles. The
offer is recorded per profile, so a default removed afterwards stays removed. Anything
else is installed from **Settings → Addons**, which lists the recommended addons with
an Install button and takes a manifest URL for anything else; settings unlock
automatically when the profile has no PIN. Only content supplied by the user's addons
is shown.

## Deliberate platform translations

The TV client is a D-pad interface; iOS is touch, so some structure differs while the
feature set does not:

- **Six rail destinations become five tabs.** Settings is a tab (it holds addons,
  Trakt and playback preferences), and Calendar is pushed from My list, where the
  saved titles it reports on already live.
- **Each tab keeps its own navigation stack**, so returning to a tab restores where
  the user was. The TV client reset detail/source state on every tab change because a
  fixed viewport had nowhere to keep it.
- **Focus and D-pad affordances become touch ones**: the focus ring becomes a pressed
  state, "hold OK to edit" becomes a long-press context menu, and the player draws its
  own controls instead of relying on `AVPlayerViewController` so episode and source
  switching still exist.
- **`ObservableObject`, not `@Observable`.** The observation macro's compiler plugin
  does not build in this cross-compile setup (xtool #197).
- **A shared image loader replaces `AsyncImage`.** `AsyncImage` starts one unbounded
  request per view and never retries, so a 72-tile avatar grid or a wall of posters
  loses whatever iOS throttles and leaves permanent placeholders.
- **The profile wallpaper is chosen for the device.** The CDN publishes one image per
  device profile at `backgrounds/{format}/{device}.{ext}`; serving a single 3840×2160
  desktop image to a phone meant a ~4× overscale for an aspect-filled background, which
  is what pushed the picker's content off screen.
- **Each tab's path is a `NavigationPath`.** A typed `[Route]` can only hold one value
  type, so a settings sub-page pushed from a `NavigationLink` was silently rejected and
  only highlighted. `navigationDestination` also lives outside the `List`, because a
  destination registered inside a lazy container is invisible to the stack.

## Build

Prerequisites, all documented in `~/oad/BUILDING-APPS.md` and `~/oad/AGENTS.md`:

- Swift 6.3 with the toolchain clang in `/usr/lib/swift/bin`.
- `xtool`, an App Store Connect API key for signing, and the Darwin Swift SDK
  (installed with `xtool setup`).
- Rust 1.96 with the `aarch64-apple-ios` target.
- Node and pnpm only when the web settings UI needs rebuilding.

```bash
rustup target add aarch64-apple-ios
cd apps/ios
PATH=/usr/lib/swift/bin:$PATH xtool dev run     # build, sign, install, launch
```

`PATH=/usr/lib/swift/bin:$PATH` is required, exactly as for any xtool command: it puts
the toolchain's clang ahead of the system clang, without which SwiftUI fails with
`__builtin_bit_cast` errors.

`scripts/build-ios-native.sh` builds `libmadari_ios.a` for `aarch64-apple-ios` and
regenerates the committed Swift bindings. It also builds the web settings UI first,
because `madari-tv` embeds it with `include_str!`; set `MADARI_SKIP_WEB_BUILD=1` to
reuse the committed `dist`. `xtool dev build` produces `apps/ios/xtool/Madari.app`
without a device.

`scripts/fetch-ios-mpv.sh` vendors the libmpv stack that the fallback player needs.
It downloads a pinned release, verifies its sha256 and extracts 18 XCFrameworks into
the gitignored `apps/ios/native/mpv`, so a fresh checkout needs both scripts before
the app can link:

```bash
./scripts/build-ios-native.sh     # libmadari_ios.a + committed bindings
./scripts/fetch-ios-mpv.sh        # libmpv and its dependencies
```

Two environment details are easy to get wrong:

- `SDKROOT` must point at the iPhoneOS SDK, or `cc-rs` looks for `xcrun` — which does
  not exist on Linux — when building bundled SQLite and the network-interface helper.
- The app is linked with an explicit `-platform_version ios 17.0 <sdk>`. clang passes
  that itself on macOS, but not here, and without it `ld64.lld` records the
  *deployment target* as the SDK version. iOS reads that field to decide between the
  current design (Liquid Glass) and the pre-26 compatibility appearance, so the app
  silently looked like an old build. `Package.swift` reads the version out of the
  installed SDK.

## Validation

```bash
cargo test -p madari-ios                     # boundary round trips, host build
MADARI_IOS_SKIP_TARGET=1 MADARI_SKIP_WEB_BUILD=1 \
  MADARI_IOS_BINDINGS_CHECK=1 scripts/build-ios-native.sh   # committed bindings are current
cd apps/ios && PATH=/usr/lib/swift/bin:$PATH xtool dev build
```

The iOS section of CI (`.github/workflows/ios.yml`) runs the first two. It cannot
compile Swift: the Darwin Swift SDK and xtool exist only on the development machine,
so the Swift build is verified with `xtool dev build` there.

Verified on a physical iPhone 12 mini (iOS 26.6.1): the app builds, signs, installs
and launches; the Rust core opens its profile store and answers `profiles` on device;
the bindings and the 64 MB static library link with no undefined symbols; and the
libmpv stack links, embeds and is re-signed into the bundle. Adding a profile,
browsing an addon catalog, source selection and real playback have not been exercised
end to end yet, and remain the outstanding verification.

## Current limits

- **Playback runs on two backends.** AVPlayer handles what AVFoundation can decode
  (MP4/MOV/M4V/HLS) using the system decoder. Everything else — Matroska, WebM, AVI,
  WMV, FLV, MPEG-TS — goes to libmpv, so an MKV plays instead of failing to decode.
  `MediaFormat` picks the backend from the container, and either player can hand the
  same source to the other when it cannot cope with it.
- **libmpv is a vendored binary, not a package dependency.** `scripts/fetch-ios-mpv.sh`
  downloads media-kit's `libmpv-darwin-build` release, checks a pinned sha256 and
  extracts it into the gitignored `apps/ios/native/mpv`. `Package.swift` declares
  those as SwiftPM binary targets, which means SwiftPM itself never resolves or
  fetches anything — the script does, and the binaries stay ours to pin. Building this
  stack (FFmpeg, libass, freetype, harfbuzz, fribidi, dav1d) from source is a multi-day
  cross-compile, which is why it is vendored rather than built here.
- **The vendored build is libmpv-only and LGPL**: mpv 0.36 with FFmpeg 6.0, built with
  `-Dcplayer=false -Dlibmpv=true -Dgpl=false` and configured for iOS — `audiounit`,
  `ios-gl` and `plain-gl` for VideoToolbox→GLES interop, and no Metal, Vulkan or
  libplacebo. The frameworks are dynamic and embedded in the bundle, so the LGPL's
  replacement obligation is satisfied by the dynamic link itself. Provenance and
  licensing are recorded in `apps/ios/native/mpv/SOURCE.txt`.
- **Video output is OpenGL ES, which Apple deprecated in iOS 12.** libmpv's render API
  is OpenGL, so `MpvSurface` drives a `CAEAGLLayer` and the deprecation warnings are
  silenced on purpose. The alternative is a Metal/libplacebo build of mpv, which
  upstream does not support.
- **The decoder stack adds roughly 40 MB** to the app bundle; the 18 frameworks are
  embedded and re-signed on every device install, which shows up as a slower install.
- **Addon-declared subtitle URLs are still not attached.** AVFoundation has no public
  API to inject an external subtitle track into an `AVPlayerItem`. The libmpv backend
  does render the tracks a file already carries (libass is built into the vendored
  framework), and mpv's `sub-add` could attach the source's declared URLs, but that is
  not wired up yet. The source's declared subtitles are already parsed and retained.
- **The browser remote's key replay is not wired.** The settings server runs and a
  paired browser can manage profiles, addons and preferences, but its D-pad commands
  cannot be replayed into SwiftUI focus the way `MainActivity.dispatchKeyEvent` does
  on Android. The phone is its own input device, so this is not a functional loss.
- **SVG title logos do not decode**, because UIKit cannot render SVG. The title name
  is shown instead, which is what the TV client does when artwork fails.
- iOS asset catalogs are not compiled by this toolchain, so the app keeps the default
  icon; the brand mark is a bundled PNG used in-app.
- Localisation is English only, and no iPad-specific layout beyond the adaptive grids
  has been designed.

PIN access controls protect the app workflow, not the filesystem. iOS data protection
and the app sandbox apply, but the profile database is not separately encrypted.
Cleartext HTTP is enabled because user-installed addons and sources may require it.

## References used

- [xtool](https://github.com/saagarjha/xtool) and `~/oad/BUILDING-APPS.md` for the
  Linux-host toolchain, signing and device workflow.
- [UniFFI](https://mozilla.github.io/uniffi-rs/) for the Swift bindings.
- [Apple: AVAssetResourceLoaderDelegate](https://developer.apple.com/documentation/avfoundation/avassetresourceloaderdelegate)
  for streaming a custom-scheme asset.
- [Apple: AVMediaSelectionGroup](https://developer.apple.com/documentation/avfoundation/avmediaselectiongroup)
  for audio and subtitle selection.
- Existing `apps/tv` screens and `apps/linux/src/app/browsing` for behaviour and copy.
