# Madari

An independently implemented Rust application core and headless media companion.
The first implementation supports Stremio-compatible HTTP addons, persisted
library/progress, real torrent streaming, and FFmpeg transcoding. No Stremio
application core, account service, Node.js runtime, or mock player is included.
A native Linux interface provides profiles, addon management, catalog browsing,
episode/source selection, library controls, and mpv playback.

The primary client is **Linux GTK4/libadwaita**, with an experimental Windows x64
build of the same interface. See [Windows build and VM testing](docs/windows.md).
A native Kotlin **Android TV / Google TV** client now lives in `apps/tv`, using
Compose for TV and the shared Rust libraries through JNI. See
[TV build and setup](docs/android-tv.md). Other platform clients remain deferred.

## Linux app

For source organization and testing, see [Linux app architecture](apps/linux/ARCHITECTURE.md).

Trakt device login is available under **Settings → Profile → Trakt**. It imports
watchlist, watched history and personal lists into **My list**, with separate
connections per profile. See [Trakt setup](docs/trakt.md) for application credentials
and sync behavior.

Install GTK4 ≥ 4.12, libadwaita ≥ 1.5, Pango ≥ 1.56, libmpv and libepoxy development packages, plus Rust 1.96
and the native build tools listed below. Then run:

```bash
cargo run --locked -p madari-linux
```

Create a regular profile first. Its optional 4–8 digit PIN protects entry and
settings. Open Settings to install addons by manifest URL, enable/disable,
reorder, reconfigure, remove a profile's link, share, or create more profiles.
Creating a kids profile requires a regular profile with a PIN; that profile
becomes its guardian. Kids can open their profile without a PIN. Changing their
addons, sharing, or switching out requires adult authorization. Kids mode remains
active after restarting the app. Close the Settings window to relock settings.

Sharing links one configured installation. Configuration changes apply to every
linked profile; enabled status, ordering, library and playback progress remain
independent. Sharing into another protected profile requires its PIN (or its
guardian's PIN for kids). Sharing into your own kids profile uses your already
unlocked settings. Kids content comes from adult-chosen addons; there is no
automatic age-rating filter.

Linux stores `profiles.sqlite` in `$XDG_DATA_HOME/madari` (normally
`~/.local/share/madari`), overridable with `MADARI_DATA_DIR`. PINs are salted
Argon2id hashes, with a persisted one-minute lockout after five incorrect attempts.
These are application controls, not protection from someone editing local files.
The database and configured addon URLs are not encrypted. Profile data currently
stays on this device; it is separate from the companion's single-owner state.

Browse an enabled addon’s catalogs, supply its search/filter fields, select a title
or episode, then choose a source. Source results retain addon provenance and show
individual addon failures. Titles can be saved to or removed from your library;
Continue watching restores the selected video’s profile-local position.

Home shows poster sections and cinematic title details, with bundled DM Sans and loading skeletons.
Search has its own section and queries all enabled searchable catalogs. Catalogs requiring a
search term appear only in Search. Explore opens directly into a poster grid. Explore and Search append pages as you scroll,
sending `skip` only when declared, in standard 100-item increments; short or repeated pages stop loading and overlapping results are deduplicated.
Metadata is requested from every supporting enabled addon; matching responses fill missing fields.
Settings uses native libadwaita preference pages. Browsing shares a fixed navigation
header outside the page scroller, with an active section indicator and consistent
headings for Search, My list, and Calendar. Layouts support windows down to 360 pixels
wide, with compact navigation and overflow menus, stacked calendar/search/actions,
smaller posters and scrollable forms. The player keeps previous episode, back ten
seconds, play/pause, forward ten seconds and next episode centered on one control row.
Episode selection remains directly accessible; secondary playback options share one
Settings menu.
Volume sits on the right of the player bar and expands its slider inline on hover
or keyboard focus. Clicking the speaker toggles mute.
The profile picker uses centered cards, a close-only window header, and staggered
entrance animations. Page transitions move and fade the outgoing view; animations
respect the desktop's reduced-motion setting.

Artwork loads only as it enters view. Two workers download and decode bounded-size images
outside the GTK thread. Resized PNG derivatives are cached under the data directory for seven
days, with a 192 MiB disk budget. Leaving a page cancels its pending artwork requests.
JPEG, PNG, WebP and GIF are supported; invalid/oversized images show a fallback.
Startup preserves the C numeric locale required by libmpv, while retaining other UI locales.

Direct HTTP(S) sources play **inside the GTK window**, using libmpv’s OpenGL renderer.
The overlay keeps play/pause, skip, volume, audio-track selection, PiP, time and fullscreen visible when needed.
The home hero appears first, followed immediately by Continue Watching. Enabled addons declaring a
lastVideosIds catalog provide series updates (including metasDetailed responses), respecting the declared ID limit.
Continue Watching only requests these declared catalogs for their supported metadata
types; it does not fall back to direct meta requests or a hardcoded provider.
Metadata is cached in a separate, profile-local SQLite table for six hours, with a
100-title limit. Card metadata is also saved with library entries and playback progress, so new movie history retains titles and artwork without a series feed. Only missing/stale series entries are fetched; cached data survives network
failures, and changing enabled addon origins invalidates incompatible cache entries.

Calendar follows titles saved to **My list**, using enabled addons that declare
`calendarVideosIds`. It marks release dates, offers month and day views, shows watched
episodes, and lets you play released episodes. Changing dates reuses the loaded data;
Refresh requests updates. A compatible installed addon is required.

Title details include artwork and optional logos, labelled ratings, a full synopsis,
genres, cast, credits, release information, and browser links for trailers when supplied.
Details show the synopsis once in the hero, followed by episodes, cast, and additional information.
Content pages share a centered 1440px maximum width. Placeholders pulse gently.
Continue Watching has a Remove action that preserves history; playing a title restores it.
Episode progress bars sit below their artwork; Continue Watching uses an inset overlay with a top-right remove button. The player
uses a larger adaptive episode dialog with a pinned, searchable season selector.
Both selectors include thumbnails, descriptions,
watched status and episode progress. Continue Watching resumes the latest episode or
offers the following one after completion. At playback end, a cancellable 10-second
Up Next countdown advances across seasons, excluding specials and future-dated episodes.
Source continuation requires an exact bingeGroup match; otherwise the source list opens.

Audio and subtitle selection each have a dedicated control-bar popover. Choose video source
loads a scrollable source list while playback continues; selecting a source resumes from saved progress.
Settings has separate pages for subtitle size, subtitle timing, audio timing, speed (0.25–3×), video size, aspect ratio and chapters,
plus shortcut help. Popovers use window-aware size limits and scroll long lists.
Buffered and played progress use the same native seek track. Controls and the cursor hide after three seconds without movement or input, except while
buffering, dragging the seek bar or using a menu. Video titles come from metadata.
Dragging previews the target position and commits an exact seek on release without snapping back. Space pauses, arrows seek/change
volume, F toggles fullscreen, I toggles a bottom-right in-app mini player while browsing; P opens a separate PiP window, A/C cycle tracks, and [ / ] change speed.
The in-app mini player can be dragged from its video or title area and has a corner resize grip, preserves 16:9 and stays within the window.
Toggling it preserves fullscreen; controls and seeking remain clickable.
Its size and position are remembered during playback.
Mini/expanded transitions keep the same mapped video surface and preserve the browsing page.
The 280 ms eased transition scales a captured frame while playback continues, so the live video surface only changes size once.
Page navigation crossfades the outgoing viewport; artwork, catalog cards, episode rows, and player status overlays appear gradually.
These short transitions use the GTK frame clock and respect the system animation setting.
PiP is a compact resizable window; closing it returns video to Madari. Staying above other
applications depends on compositor policy. Player commands run off the GTK rendering thread.
Madari saves progress every two seconds and on stop, and
closes the player when you close Madari. Switching profiles stops playback and saves progress before opening the profile picker.
Inline HTTP(S) subtitles are loaded when the source has no custom request headers;
subtitle-addon discovery and per-subtitle credentials remain future work.

Torrent playback is built into the Linux app by default. Its engine feeds
seekable bytes directly to mpv through an in-process stream callback: no HTTP server,
TCP listener, or Unix socket is created. Incoming peer connections, DHT and local
service discovery are disabled; torrent discovery uses trackers and outgoing peers.
Downloads and session state live in the application's private `media` directory.
Started video files continue downloading after playback closes and then seed while
the app is open. Saved transfers are restored on launch. Manage them under
**Torrents** in the top navigation next to Calendar (under **Browse** on small screens): live progress, download
and upload speeds, peer counts, Pause/Resume, and Remove. Remove forgets the torrent
but keeps downloaded files; quitting the app stops transfers. Other files in a
season pack are not selected automatically. Seeding uses outbound peer connections;
no incoming peer listener or media-server port is enabled.


In **Settings → Media server → Configure**, choose **Built-in** or **External server**.
External mode supports FFmpeg transcoding in addition to original streaming. Its API
key and selection remain in memory for the app session. Alternatively, supply an
external connection at launch:

```bash
export MADARI_COMPANION_URL=http://localhost:11470
export MADARI_COMPANION_API_KEY="your-companion-api-key"
cargo run --locked -p madari-linux
```

Only an origin is supported, without a reverse-proxy path prefix. Remote servers
should use HTTPS. The companion connection is device-wide, while addons and
viewing state stay profile-scoped. Media tickets are revoked after playback;
registered torrents remain managed by the companion. FFmpeg resumes at the saved
offset; seeking is limited by the live fragmented-MP4 transcode. Automatic codec negotiation and playlist redirects are not implemented yet.

## Run companion

Install Rust 1.96 or newer, a C/C++ build toolchain and CMake (native dependencies),
and FFmpeg/ffprobe. Transcoding currently requires FFmpeg's `libx264` and `aac`
encoders. FFmpeg is executed as a separate process and is not bundled.

```bash
export MADARI_API_KEY="$(openssl rand -hex 32)"
cargo run --locked -p madari-server
```

The server listens at `http://127.0.0.1:11470`. Keep the same API key across restarts
if you want existing clients to reconnect. Check it with:

```bash
curl -H "Authorization: Bearer $MADARI_API_KEY" \
  http://127.0.0.1:11470/v1/health
```

Interactive OpenAPI documentation is at **http://127.0.0.1:11470/docs/** and the
OpenAPI 3.1 document is at **http://127.0.0.1:11470/openapi.json**. These documentation
routes are public; operational `/v1` routes still require the API key. Click
**Authorize** in Swagger UI and enter the key to try API requests. Swagger assets
are served by the companion itself, with no runtime CDN dependency or external
validator. Authorization is not persisted across browser reloads.

Export the same generated contract without starting the server or supplying a key:

```bash
cargo run --locked -p madari-server -- --openapi > openapi.json
```

Schemas derive from the shared DTOs behind an optional `openapi` feature, and
operations are annotated on the actual handlers. Media GET/HEAD, range responses,
transcoding offsets, SSE, JSON errors, and bearer authentication are documented.

| Environment variable | Default | Purpose |
| --- | --- | --- |
| `MADARI_API_KEY` | Required | 32–1024 non-space printable ASCII characters; use a random secret |
| `MADARI_BIND` | `127.0.0.1:11470` | Set `0.0.0.0:11470` or `[::]:11470` for LAN/reverse-proxy access |
| `MADARI_DATA_DIR` | `data` | SQLite database, torrent session state, downloaded pieces |
| `MADARI_FFMPEG` | `ffmpeg` | FFmpeg executable path |
| `MADARI_ALLOWED_ORIGINS` | None | Comma-separated exact web origins, e.g. `http://localhost:5173` |

Remote access uses the same authenticated API. Put a TLS reverse proxy in front of
the server for remote deployment; this binary serves HTTP, not TLS. Preserve range
headers, disable response buffering for media/SSE, and avoid logging media URL
paths because they contain capability tokens. The server is a single-owner
service: possession of the API key grants control over addons, state, and torrent
activity. It is not a multi-tenant service.

On Unix the data directory is owner-only. Configured addon URLs are currently
stored in SQLite inside that directory, **not encrypted**. Keyring integration,
encrypted export, and Windows ACL setup are not implemented. The long-lived API
key is supplied at startup and is not written to the database. Public state
snapshots omit configured transport URLs; addon response data can still contain
credentials and should not be logged or published.

## Addon flow

Install a public or configured manifest URL:

```bash
curl -H "Authorization: Bearer $MADARI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"manifest_url":"https://your-addon.example/configuration/manifest.json"}' \
  http://127.0.0.1:11470/v1/addons
```

For an addon on your machine or LAN, explicitly include `"allow_local":true`.
This permission is stored per installation. Addons requiring configuration return
a structured error; configure them in their own external page and install the
resulting URL. Two installations of the same addon receive separate IDs.

Read `/v1/snapshot` to discover manifest-declared catalogs and supported extras.
Use a returned installation ID and its declared catalog type/ID:

```bash
curl -H "Authorization: Bearer $MADARI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"resource":"catalog","type":"movie","id":"all","extra":{"search":"bunny"}}' \
  http://127.0.0.1:11470/v1/addons/INSTALLATION_ID/query
```

The query resource can be `catalog`, `meta`, `stream`, `subtitles`, or
`addon_catalog`. Send video IDs for streams/subtitles and item IDs for metadata.
The API returns `{ "resource": "catalog", "data": [...] }` (or the corresponding
resource shape). Unknown source fields and behavior hints are retained.

`POST /v1/query` sends the same request to every enabled compatible installation.
Its result array stays in installation order, and each entry has an installation
ID plus `result.Ok` or `result.Err`. Catalog IDs vary by addon: clients should
enumerate their catalogs to search across different catalog IDs. Discovered
addons are returned as data and are never automatically installed.

## Torrent and media flow

For a selected addon stream, use **`POST /v1/playback/prepare`**. This replaces the
manual magnet → torrent → file → media-ticket sequence:

```json
{
  "source": {
    "infoHash": "0123456789abcdef0123456789abcdef01234567",
    "fileIdx": 0,
    "sources": ["tracker:https://tracker.example/announce"]
  },
  "capabilities": {"companion": true, "url_schemes": ["http", "https"]},
  "key": {"installation_id": "ID", "content_type": "movie", "item_id": "ITEM"},
  "video_id": "VIDEO"
}
```

Use an actual stream returned by an addon; the hash above illustrates the shape.
Include your bearer API key as with other `/v1` operations. Omit both `key` and
`video_id` if there is no saved identity. An optional top-level `file_index`
overrides `source.fileIdx`. If neither is set, the largest torrent file is selected,
with the lowest index breaking equal-size ties. An invalid explicit index returns
an error instead of silently selecting another file. Filename hints are preserved
for subtitles, not treated as authoritative episode selectors.

The response contains `plan` (including the original source/hints/subtitles and
`resume_ms`) plus `delivery`:

- `kind: "torrent"`: torrent details, selected `file`, `selection` reason and
  `media` ticket. Resolve its paths against the companion origin. The direct path
  serves the original timeline: seek the player to `plan.resume_ms`.
- The torrent ticket's `transcode_path` already includes the saved start offset.
  Start that output at player time zero. Add `media.transcode_start_ms` when
  reporting player-relative progress back to `/v1/progress`; do not apply the
  resume seek a second time. New seeking requests replace the `start` query value.
  Companion preparation currently bounds the resume offset to seven days.
- `kind: "direct"`: the source URL and validated, case-normalized
  `request_headers` for a capable native player. The companion does not fetch
  that URL. Codecs still need player/probe validation.
- `kind: "external_application"`: a validated HTTP(S) external URL to open at the
  client's discretion. `kind: "unsupported"` retains the source and plan reason
  without initializing a torrent or issuing a ticket.

Preparation does not launch playback or modify library/progress. A failed file
selection can leave resolved torrent metadata registered for a corrected request;
it does not issue a media ticket. Repeated preparations reuse that torrent and
create independent expiring tickets. The legacy `/v1/playback/plan` remains a
capability query that performs no media resolution.

The lower-level torrent and ticket APIs remain available:

1. `POST /v1/torrents` with `{"magnet":"magnet:?xt=urn:btih:..."}`, or upload
   `.torrent` bytes to `POST /v1/torrents/metainfo`. Both require the bearer API key.
   Addon-stream preparation constructs the magnet automatically, including
   HTTP(S)/UDP `tracker:` sources. V1 hashes must be 40 hexadecimal characters;
   base32 and v2-only magnets are not supported in this slice. Unknown magnet
   parameters, including `so`, are not forwarded; use explicit Madari file selection.
   Extra DHT hints remain in the source but are not injected into the engine.
2. The response contains a stable torrent info-hash ID and indexed files. Pick
   the addon-provided `fileIdx`, or let the user select a file. No file is selected
   for background bulk download by default; reading a file drives piece priority.
3. `POST /v1/media` with `{"torrent_id":"INFO_HASH","file":0}` to create a media
   ticket. The response includes `direct_path`, `transcode_path`, `token`, and
   `expires_in_seconds`. Prepend the companion server's origin to a returned path.
4. Pass the direct URL to a compatible platform player. It supports `GET`, `HEAD`,
   and single byte ranges, including suffix/open-ended ranges.
5. For incompatible codecs, use the transcode URL. It produces fragmented MP4
   with H.264 video and AAC audio. To seek, start a new transcode request with
   `?start=SECONDS`; transcoded output does not implement byte seeking.

Media URLs work without a bearer header for browser/native players. Their random
ticket is scoped to one torrent file, expires after six hours, and can be revoked
with `DELETE /v1/media/TOKEN`. Restarting the server also invalidates tickets.
Expiry/revocation prevents new requests; it does not interrupt an already-open
stream. Never put the long-lived API key in a URL. A ticket permits both direct
delivery and transcoding of its file.

The server limits active raw streams to 32, FFmpeg processes to two, outstanding
tickets to 256, and registered torrents to approximately 64. FFmpeg stops when
its response is dropped. Torrent entries can be forgotten with
`DELETE /v1/torrents/INFO_HASH`; downloaded data is retained. Automatic disk quota
and cleanup policies are still needed. Torrent peer traffic may include upload.

The current companion transcodes **torrent files only**. Direct addon URLs are
returned for platform players; HTTP-source proxying/transcoding, adaptive HLS,
hardware encoder selection, subtitle burn-in, and automatic codec negotiation
remain future work. Source headers/subtitles are preserved for player adapters.

## State and playback contracts

All routes below require `Authorization: Bearer …`:

| Route | Input / behavior |
| --- | --- |
| `GET /v1/snapshot` | API version, persisted revision, addons, library, progress |
| `GET /v1/events` | SSE: initial snapshot followed by changed revisions |
| `PUT /v1/addons/ID` | `{"enabled":false}` |
| `PUT /v1/addons/order` | JSON array containing every installation ID exactly once |
| `DELETE /v1/addons/ID` | Remove installation; preserve associated library/progress |
| `PUT /v1/library` | `{"key":KEY,"title":"Film title"}` |
| `DELETE /v1/library` | `KEY` as JSON body |
| `PUT /v1/progress` | `{"key":KEY,"video_id":"VIDEO","position_ms":12000,"duration_ms":60000,"completed":false}` |
| `POST /v1/playback/plan` | `{"source":STREAM,"capabilities":CAPABILITIES,"key":KEY,"video_id":"VIDEO"}` |
| `POST /v1/playback/prepare` | Same identity/source/capabilities plus optional `file_index`; resolves torrent and returns media links |
| `GET /v1/torrents` | Registered torrent details |
| `GET /v1/torrents/INFO_HASH` | File indices and sizes |

`KEY` is `{"installation_id":"ID","content_type":"movie","item_id":"OPAQUE_ID"}`.
Identities are provider-scoped. Playback-plan capabilities include `url_schemes`,
`web`, `torrent`, `companion`, `request_headers`, and `external_application`.
For example, a web player can send `{"url_schemes":["https"],"web":true,"companion":true}`.
Omit both `key` and `video_id` for a plan without saved resume state. Completed
videos resume at zero. Plans describe requirements; platform players perform
actual playback and report progress. Codec compatibility still needs probing.

Snapshots use atomic revision-checked SQLite writes. Subscribe before reading a
snapshot in the direct Rust API; the SSE route does this automatically. A slow
subscriber is disconnected after its bounded queue fills and must reconnect for
a fresh snapshot. Ignore events older than the last applied revision. Events are
in-process notifications, not a durable event log: run one server per data directory.
Closing a subscription unsubscribes. The core's `query_incremental` stream yields
per-addon results as they arrive; dropping it cancels its in-flight HTTP futures.
The initial HTTP query route returns aggregated results. HTTP disconnect-driven
query cancellation, persistent request deduplication, and binding-level request
IDs are not yet implemented; response `X-Request-Id` is diagnostic only.

## Workspace

| Package | Responsibility |
| --- | --- |
| `madari-model` | Portable DTOs, identities, structured errors, playback contracts |
| `madari-addon` | Manifest declarations, URL routing, typed resource parsing |
| `madari-core` | Installation lifecycle, queries, state, subscriptions, playback policy |
| `madari-native` | Tokio HTTP and SQLite adapters |
| `madari-tv` | Android JNI bindings to the shared core, profiles and in-process media |
| `madari-media` | Torrent engine interface, librqbit, ranges, FFmpeg process lifecycle |
| `madari-server` | Authenticated Axum companion API and media tickets |

The portable crates compile for WASM and native targets without the media engine.
The GTK4/libadwaita client and its player integration target Linux, with an
experimental Windows x64 build for VM testing. The ARM64 Android TV client uses
JNI bindings to these same native libraries. Other clients and browser
HTTP/storage implementations are deferred.

## Validate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
rustup target add wasm32-unknown-unknown
cargo check -p madari-core --target wasm32-unknown-unknown --locked
```

Tests require loopback sockets plus FFmpeg/ffprobe with the encoders described
above. They generate a short video locally, obtain its stream from a local addon,
discover a real seeder through a local tracker, resolve the magnet and media ticket,
stream and seek from a second peer, transcode it, and check output codecs. No
public torrent or external addon is used. Protocol fixtures are synthetic and
checked into `tests/fixtures`. See [compatibility](docs/compatibility.md) and
[architecture decisions](docs/architecture.md) for the current scope.

Continue Watching posters and resume actions play the first exact match for the saved
`bingeGroup`, preferring the previous addon. Source preferences are stored with each
profile’s progress and survive restarts. Older history without a group, or titles with
no matching source, open the source picker. The same source cards are used in the
picker and player popover, with quality, source details, transport, and current playback status.
Continue Watching keeps the card visible and shows a spinner in its play action while
finding and preparing a stream. Unmatched results open directly in the source picker.
Videos with saved progress past 95% restart from the beginning; earlier positions resume normally.
Completed series episodes offer the next released episode; finishing the final available
episode does not restart the series. Episode navigation and the end-of-episode prompt
are shown only for series.
During series playback, a dismissible “Play next episode” action appears at a late
credits chapter, or automatically at 95% of playback, whichever comes first.
This uses local chapter metadata and timing, not AI frame analysis. The early action
does not stop playback or start a countdown, and “Keep watching” dismisses it for that
playback.
“Skip intro” appears during explicitly marked intro/opening chapters when the next
chapter provides a known end. It does not guess a skip duration for unmarked videos.
Intro and next-episode actions sit at the bottom right and move above visible player
controls. Episode and source changes preserve fullscreen.
Switching into and out of picture in picture reuses the same OpenGL and mpv render
contexts. It does not reload the media or reselect its video track, and preserves
position, pause state and speed. Returning restores the previous fullscreen state.

Playback settings are stored separately for each profile. Audio and subtitle language
lists can be reordered; SDH, forced subtitles, audio description and commentary can
be preferred or avoided within the selected language. Automatic choices use track
metadata (and explicit track-title labels when flags are absent); manual selections
remain in control for that video. Previous/next episode controls appear only when an
available neighbor exists, and the player title includes the series, season/episode,
and episode name.
