# Madari for Android TV / Google TV

The TV client is a native Kotlin application in `apps/tv`, using Google's
`androidx.tv:tv-material` components and Media3 ExoPlayer. It uses the Linux app's
cinematic hero, poster sections, DM Sans font, profile flows and addon-driven
browsing as its reference. It has its own television layout and D-pad controls.
There is no WebView, companion server requirement, or duplicate Kotlin media core.

## Architecture

```text
Compose for TV screens
    → TvViewModel / CoreRepository (calls on Dispatchers.IO)
        → NativeCore JNI
            → madari-tv (thin platform boundary)
                → madari-core (catalogs, metadata, library, resume, episode policy)
                → madari-native (profiles, PINs, HTTP, SQLite, internal media)
                → madari-media (embedded torrent engine)

Media3 ExoPlayer
    → HTTP / HLS / DASH data sources
    → NativeTorrentDataSource → JNI read/seek → InternalMedia
```

`madari-tv` owns one Tokio runtime, the profile session and a checked registry of
media readers. Handles are integers, not Rust pointers exposed to Kotlin. Every
JNI entry point catches Rust panics and reports errors as Java exceptions.
Source fields cross the boundary intact; shared Rust code validates delivery,
source headers and resume positions. Torrent bytes stay in process, without a
loopback HTTP listener. Reads have a 15-second timeout and a bounded copy buffer.

The profile database and downloaded media live in app-private storage. Profiles
are local to this TV; they do not synchronize with a Linux installation. PINs and
kids-mode enforcement come from the existing native profile implementation.
Backup/device transfer is disabled for this private state. Android HTTPS uses
rustls-platform-verifier's Java trust-store adapter, located through Cargo
metadata and bundled with the APK.

## Source layout

`apps/tv/app/src/main/java/dev/madari/tv` is organized by layer and feature:

```text
MainActivity.kt        app shell, back handling, error dialog
MadariApplication.kt   Coil image loader with the SVG decoder
core/                  NativeCore JNI bridge, CoreRepository, JSON helpers, domain models
state/                 TvState, TvViewModel and snapshot-derived screen models
ui/theme/              TvColors, MadariTheme, the app-wide BringIntoViewSpec
ui/components/         Glyph, Action/Input, poster rows, Hero, cinematic rows, loading
ui/navigation/         expanding left navigation rail
feature/<area>/        one screen per addon-driven area: profiles, home, library,
                       search, explore, details, sources, settings, calendar, player
```

Dependencies point inward: `feature/*` and `ui/*` depend on `state` and `core`, while
`core` depends on nothing above it. Screens never call JNI directly; every native
operation goes through `CoreRepository` on `Dispatchers.IO`.

## Implemented flows

- Create adult or kids profiles; enter a protected profile; guardian authorization
  for settings and leaving kids mode. New kids profiles require a PIN-protected adult.
- Home hero, addon catalog rows, Continue watching, saved list and title details.
- Search declared searchable catalogs; browse catalogs and supply declared
  filters; explicit pagination in 100-item increments with deduplication and
  repeated/short-page termination.
- Shared-core metadata resolution, season/episode lists, source provenance and
  partial-addon failure messages.
- Native HTTP, HLS, DASH and embedded torrent playback, pause/seek, Media3 track
  and subtitle controls, playback speed controls, audio focus and a media session.
- Shared resume policy, position persistence every two seconds and on player
  disposal, and manual next-episode selection using the core's availability and
  episode-order rules. A new episode opens source selection.
- Add/remove/enable/disable/reorder addons, with explicit local-network permission.
- Calendar for saved titles, using the core's addon-declared calendar support.
- TV launcher entry and banner, remote focus indication, catalog focus restoration,
  error recovery, empty states and font scaling through Compose typography.

Nothing is installed automatically. Create a profile, open Settings, unlock it
(with an empty PIN if none was set), then install a configured addon manifest URL.
Only content supplied by the user's addons is shown.

## Build

Prerequisites on Linux:

- JDK 17 or 21.
- Android SDK platform 36 and build tools; Android NDK 28.2.13676358.
- Rust 1.96+ with `aarch64-linux-android` and `armv7-linux-androideabi` installed.
- Internet access for Cargo and Gradle dependencies on the first build.

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
cd apps/tv
./gradlew :app:assembleDebug :app:lintDebug :app:assembleDebugAndroidTest
```

Gradle runs `scripts/build-tv-native.sh`, builds the Rust library for ARM64 and ARMv7,
packages its matching Java TLS adapter, and places the shared library in
`jniLibs/arm64-v8a` and `jniLibs/armeabi-v7a`. Both native artifacts are generated and ignored by version
control. The NDK linker uses 16 KiB page alignment.

The APK supports **ARM64 and 32-bit ARM Android TV, Android 8.0 / API 26 or later**.
x86 emulators need an additional native ABI build.

Output: `apps/tv/app/build/outputs/apk/debug/app-debug.apk`.

```bash
adb install -r apps/tv/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n dev.madari.tv/.MainActivity
```

`./gradlew :app:assembleRelease` also validates R8 shrinking. The resulting release
APK is **unsigned**; configure your own release signing before distribution.
The debug APK is signed for local installation. No release key is stored here.

## Validation

```bash
cargo test -p madari-core -p madari-tv
cd apps/tv
./gradlew :app:lintDebug :app:assembleDebug :app:assembleRelease
./gradlew :app:connectedDebugAndroidTest
```

The native tests cover profile persistence, incorrect PIN rejection, kids-mode
restart enforcement, saved metadata, progress, resume/header planning, episode
selection across seasons, and real local HTTP addon installation, search,
metadata and source queries through the platform boundary. The addon HTTP test
needs permission to bind a local port.

Android instrumentation tests cover JNI profile/library/progress round trips and
D-pad focus/activation on TV Material buttons. They compile but have not been run. The app was subsequently installed and
launched on a physical ARMv7 Google TV running Android 14; its interface was
verified by a device screenshot, and a text-contrast issue was corrected.
Actual media playback and the complete remote-navigation suite remain unverified.

Before distribution, run the instrumentation tests and test on an ARM64 TV with
its remote: profile PIN entry, long rows and filters, back/focus restoration,
real authorized HTTP/HLS/DASH and torrent media, seek/resume, alternate tracks,
background/foreground transitions, and interrupted networks.

## Current limits

This is the first TV client, not full Linux feature parity. Trakt login/sync,
addon sharing/reconfiguration UI, PiP, transfer management, automatic next-episode
source matching, subtitle-addon discovery and TV home-screen recommendations
are not wired into the TV interface. Continue watching currently lists unfinished
items; completed series can advance from title details using the shared policy.
The player uses codecs available to Media3/device decoders; desktop FFmpeg
transcoding is not bundled. Inline subtitles are loaded only for sources without
custom HTTP headers, matching the Linux client's credential isolation policy.

Cleartext HTTP is enabled because user-installed addons and sources may require
it. PIN access controls protect the app workflow, not rooted-device file access.

## References used

- [Google: Compose for TV](https://developer.android.com/training/tv/playback/compose)
- [Google: TV components and design](https://developer.android.com/design/ui/tv/guides/components)
- [Google: Media3 ExoPlayer setup](https://developer.android.com/media/media3/exoplayer/hello-world)
- Existing `apps/linux/src/app/browsing`, profile and playback modules.
- The locally resolved rustls-platform-verifier Android integration instructions.

## TV navigation and performance build

The left navigation rail stays at a 72dp content inset. It expands over the page
on focus, and selecting a destination moves focus back into the content. Catalog
requests run concurrently (up to three); native network operations do not hold
the session mutex. JSON response parsing runs on the IO dispatcher, and unchanged
web-server polls do not emit new screen state.

For local TV testing with R8 optimizations and no debugger overhead, use
`./gradlew :app:assemblePerformance :app:lintPerformance`. Install
`app/build/outputs/apk/performance/app-performance.apk`. This build uses the local
debug signing key so it can update the development installation without clearing
data; it is not a distribution signing configuration.

The left-rail instrumentation regression passed on the ARMv7 Android 14 TV:
focus expands labels, D-pad selects Explore, and activation returns to content
and collapses the rail. Native TV regression tests: 6 passed.

Title details use a dedicated backdrop layout with Watch/Resume, My List and
Details actions. Series expose season selectors and landscape episode cards;
the synopsis and credits are available below and in the Details dialog. Resume
labels and episode selection use the shared core's selected video and progress.

Focus scrolling uses one app-wide `LocalBringIntoViewSpec`: fully visible controls
leave the viewport unchanged; clipped controls scroll only to the nearest edge,
with a small focus margin. Home and detail hero groups request their entire bounds
when focus enters, keeping the title and artwork visible above their actions.
This applies to vertical pages, horizontal catalog rows, grids and dialogs.


The current local design uses the Linux brand asset unchanged, a framed home
artwork banner, an expanding featured catalog card, and a dedicated two-column
season/episode browser. The repeated About-title footer is removed; complete
synopsis and credits remain in Details. Startup holds focus on a non-interactive
loading surface until actual catalog items arrive, then focuses the hero.
These revisions are built locally; they have not been installed on the TV.

The overview is now a fixed viewport with vertical Watch and Episodes actions;
only secondary panels scroll. Panel dismissal restores the launching action,
and a saved-list update does not reset focus to Watch. The launcher and banner
reference the Linux logo, and the shared Coil 2 image loader registers its SVG
decoder for addon title logos. Missing/failed title artwork retains the text name.

On the user's personal TV, update only with `adb install -r` using the existing
signing key. Never uninstall or clear app data to update. Do not run Gradle
connected-device instrumentation tasks on this TV: their application lifecycle
can remove the installed app. Use an emulator/dedicated test device for that suite.

The profile picker uses left-aligned vertical profile rows with deterministic
avatar colors, a white focus outline, and the bundled Linux logo. PIN entry is a
modal with initial input focus. Startup placeholders do not expose creation fields
before profile loading completes. Updates preserve the existing profile store.

Profile visual refinement: square 104dp tiles in a left-aligned row, with names
below, no wide profile card or background watermark, and no focus scaling. Home
uses consistent poster rows; the expanding featured card and its synopsis block
were removed. Focus borders are thinner and poster scaling is reduced to 1.025.

Catalog rows now keep source order and initially expand their first card. Focus
expands in place over 200ms; selected and neighbouring backdrops are preloaded at
display resolution. The poster remains behind the backdrop until decode succeeds,
then artwork fades in. Metadata has a fixed compact height, and the final expanded
bounds are brought into view after the width transition. Profile tiles stay square.

Cinematic catalog rows scroll the selected card to the leading content inset.
Extra trailing padding allows the final card to align there too. The catalog
order stays unchanged. This is horizontal row behavior only; the app-wide
vertical focus policy continues to preserve already-visible content.

Transition correction: the cinematic row now uses a single row-local
BringIntoViewSpec for left anchoring instead of running animateScrollToItem
against changing card widths. Poster and backdrop layers remain mounted with
fixed decode sizes; focus only animates their opacity. Metadata crossfades inside
fixed-height bounds. Late catalog responses cannot swap the current home hero.

Bounce correction: each row uses one shared selection transition for all card
widths. Entry focus restoration is captured once and disabled after focus is
received; recycling a card during scrolling cannot reclaim focus or restart
selection. The selected card remains expanded when focus leaves its row.

The current catalog treatment supersedes width expansion: all cards stay 336×189dp
at rest and on focus. Backdrops stay visible across focus changes. Only the first
image load fades, the focus border changes, and the row scrolls to the leading
edge. Selection no longer controls dimensions or switches poster/backdrop layers.

Current row interaction: exactly one permanently wide focus slot stays at the
left; narrow upcoming posters slide beside it. Left/right changes the selected
catalog index without moving or resizing that slot. Its previous image stays
until the new backdrop (or poster fallback) is decoded, then crossfades inside
fixed bounds. Left from the first item returns to normal rail navigation; up/down
moves between rows; OK opens the currently selected title. Earlier all-wide and
per-card expansion implementations are superseded.
