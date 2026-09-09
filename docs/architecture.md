# Initial architecture decisions

Recorded 2026-09-08. These decisions implement the confirmed direction in NOTES.md.

## Independent core

Madari implements its own application logic and HTTP addon consumer. Stremio's
addon SDK protocol is an interoperability target, not a runtime dependency. No
Stremio application state model or account/service assumptions are imported.

The portable boundary is `madari-model` → `madari-addon` → `madari-core`. HTTP and
persistence are injected traits. Native trait futures are Send; WASM futures and
adapter objects may remain on their browser worker thread. No Tokio, SQLite,
FFmpeg, GTK, browser APIs, or torrent engine appears in the portable dependency
graph. Cargo.lock pins the dependency graph; initial supported toolchain is 1.96.

## Native persistence

Use rusqlite 0.37 with bundled SQLite for the initial server adapter. Schema v1
stores a versioned JSON snapshot with an atomic revision-checked update. SQLite
runs on blocking workers, uses WAL, and rejects a newer database schema.
This is adequate for the first single-owner server, not a final large-library
schema. Normalize/index tables when query scale requires it. Other clients can
implement the same storage contract using their native persistence mechanisms.

Configured transport URLs are sensitive. The server keeps them in an owner-only
data directory on Unix and excludes them from public snapshots. Encryption,
keyrings and safe exports are unresolved. Do not treat the snapshot DTO as a
credential-free export: addon-controlled fields may themselves contain secrets.

## Torrent engine

Use `librqbit = 9.0.1` behind `TorrentEngine`. The published package declares
Apache-2.0 and records source revision
`a499d2f243d124e144aef137afe7cb304a6e3f36`, under `crates/librqbit`.
Its Session, FileStream, API and persistence source were inspected for this
integration. Its async reader prioritizes pieces around reads and supports seeks.
The deterministic integration test demonstrates a real local peer transfer.

This is an initial integration review, not a full transitive security/license
audit. No guarantee of iOS engine support or stable upstream API is assumed.
The adapter shields the portable core from engine-specific types. Torrent
trackers/peers are intentionally contacted by an authenticated owner's media
engine; this service is not hardened for mutually untrusted tenants.

Sources:
- [Published package](https://crates.io/crates/librqbit/9.0.1)
- [Versioned Rust API](https://docs.rs/librqbit/9.0.1/librqbit/)
- [Pinned upstream source](https://github.com/ikatson/rqbit/tree/a499d2f243d124e144aef137afe7cb304a6e3f36/crates/librqbit)

## Companion server and FFmpeg

Axum exposes `/v1` with bearer API-key authentication. Random expiring tickets
authorize one torrent file for player-compatible media URLs. API keys are hashed
in memory and compared using a constant-time digest comparison. CORS is an exact
origin allowlist; it is not authentication. Remote TLS belongs to the deployment
reverse proxy in this slice.

OpenAPI 3.1 is generated with utoipa from shared DTOs and annotated handlers.
The optional `openapi` crate features keep documentation tooling out of ordinary
portable-core builds. `/openapi.json` and `/docs/` are public documentation routes;
Swagger UI assets are vendored and served locally. The UI does not persist bearer
credentials or call an external specification validator. `--openapi` exports the
same document without initializing storage, torrents, or an HTTP listener.

FFmpeg reads the same loopback HTTP torrent stream as a player, allowing input
seeking without downloading a whole torrent first. Input is restricted to Madari
loopback URLs and a media demuxer/protocol allowlist. No shell is involved. stdout
is streamed with backpressure; the owned child is killed when its body is dropped.
Processes and blocked reads have bounded resources/deadlines. The output is
fragmented MP4/H.264/AAC, with a new start-offset request for transcoded seeking.

No FFmpeg binaries are distributed here. Distribution packaging and the license
of the chosen FFmpeg build/encoders must be reviewed before shipping installers.

## Remaining milestones

1. Harden core contracts: cache policy, deduplication, request cancellation across
   HTTP/bindings, full search aggregation, addon refresh/configuration UX, preferences,
   subtitle merging, next-episode/source continuity, broader protocol fixtures.
2. Expand media delivery: direct HTTP sources and headers, probe-driven codec
   negotiation, subtitle choices, disk quotas, torrent lifecycle controls, HLS,
   remote deployment and API-key management.
3. Refine the Linux GTK4/libadwaita client and mpv player integration with
   richer catalog presentation and codec negotiation. Other platform clients and language bindings are deferred
   by the user's current scope; keep the core portable without building them.

## Unified playback preparation

`Core::prepare_playback` owns validation, provider-scoped resume lookup and torrent
file selection. Its `PlaybackMedia` interface delegates metadata resolution and
ticket creation to the server, keeping the native torrent engine out of the core.
`POST /v1/playback/prepare` exposes that operation and its generated OpenAPI DTOs.

Source priority is explicit: conflicting primary source forms are rejected.
An explicit request index overrides addon `fileIdx`; otherwise the largest file
is selected, with the lowest index breaking ties. This matches the documented
addon default. Filename hints do not select episodes. An unavailable or empty
explicit file returns an error and does not issue a ticket.

Magnet normalization validates the v1 hash, deduplicates and encodes trackers, and
removes unhandled parameters before entering the engine. Arbitrary `so` ranges
are never expanded. Existing torrents are reused, including when the engine has
reached its new-torrent limit. There is no pre-download prerequisite.

The result preserves source behavior hints/subtitles and a selection reason.
Direct sources carry validated request headers for the player. Companion torrent
tickets include a transcode URL with the saved start offset and
`transcode_start_ms` for translating player-relative progress to the original
timeline. Direct playback uses the original timeline. Codec probing and choosing
between direct/remux/transcode remain a separate next milestone.

The integration scenario uses an addon-returned info hash and local tracker to
discover a real peer, asserts zero content progress before the first media read,
then verifies seeking and actual FFmpeg output. No player simulator is involved.

## Initial verification

- 19 tests pass, including actual local BitTorrent/FFmpeg delivery, persistence,
  protocol routing, resume isolation, subscriber overflow, and OpenAPI/UI serving.
- Formatting and Clippy with warnings denied pass.
- Foundation-stage portable core compile checks passed for Linux, wasm32-unknown-unknown,
  aarch64-linux-android, aarch64-apple-ios, aarch64-apple-darwin, and
  x86_64-pc-windows-gnu. These are compile checks, not Swift/WinUI/Kotlin binding
  or runtime validation.

## Linux profile storage

`apps/linux` is a GTK4/libadwaita client using the Rust core directly. Tokio runs
network and profile operations away from the GTK main loop. The native `Profiles`
service exposes authenticated sessions and a profile-scoped `Storage` view to Core.
SQLite stores a shared installation registry plus per-profile links (order/enabled)
and private library/progress snapshots. A global revision and transactional CAS
protect linked configuration updates. Removing one link retains other profiles'
installation. Sharing checks destination/guardian authorization.

PINs use salted Argon2id hashes and persisted attempt throttling. Session tokens
stay in memory; changing profile invalidates old Core handles. Editing grants are
explicitly closed by Done or leaving the profile. Persisted kids mode prevents
switching profiles by restarting the app. Kids profiles require a PIN-protected
regular guardian, whose PIN authorizes settings and exit. These controls apply
inside the application; they do not defend against local database modification.

Linux's `profiles.sqlite` is separate from the companion's single-owner state.
There is no profile sync or server profile endpoint yet. Linux browsing and
playback use this local profile state.

## Linux browsing and playback

Catalogs expose the addon's declared search/filter fields and optional skip paging.
Details keep the catalog provider's identity; streams are queried across enabled
profile addons and retain source provenance. Library and resume keys remain scoped
to the original catalog installation and selected video.

The Linux companion adapter implements `PlaybackMedia` through authenticated
`/v1/torrents` and `/v1/media` requests. The latter accepts optional `resume_ms`
(default 0) so a desktop Core can create offset transcodes using its own progress,
without writing profile data to the companion. Responses are bounded; redirects
are refused; bearer credentials are never handed to the player. Media paths must
resolve within the configured companion origin.

mpv runs as a child with a private Unix-domain IPC socket, no user configuration,
no external scripts and no youtube-dl integration. URLs and request headers are
sent as structured JSON rather than process arguments. The adapter observes
actual player position/duration, periodically persists progress, and handles EOF,
errors, stop and window shutdown. EOF only marks completion near a known duration;
unknown-duration streams keep resume state. Transcoded relative timestamps are
mapped back to the original timeline using the ticket offset. Closing Madari waits
for player cleanup and progress persistence. The current native player uses its
own window; embedding into GTK remains separate work.

Tests decode a generated H.264/AAC HTTP video in real mpv (headless video/audio
outputs), verify request headers, stopping, persisted resume and completion. The
real torrent/FFmpeg integration also exercises desktop companion authentication,
profile-local resume offsets and media-ticket revocation.

IPC behavior follows the [official mpv manual](https://mpv.io/manual/stable/#json-ipc),
including structured `loadfile` options (mpv ≥ 0.38) and observed property events.
