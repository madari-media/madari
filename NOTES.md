# Madari — Shared Rust Core Plan

## Implementation status — 2026-09-08

The first shared Rust core and headless companion server are implemented. The
workspace includes portable models/protocol/core crates, native HTTP/SQLite
adapters, a librqbit torrent adapter, FFmpeg transcoding, and an authenticated
Axum server with generated OpenAPI 3.1 at `/openapi.json` and embedded Swagger UI
at `/docs/`. Local integration tests exercise actual BitTorrent transfer and
transcoding, addon resource requests, and persistence recovery. The Linux
GTK4/libadwaita UI includes profile/addon management, catalog browsing, episode and
source selection, library controls and embedded libmpv playback, bounded cached artwork, dedicated search and native GNOME settings.
It connects to the companion for original torrent streams and FFmpeg transcodes. No mock player is included. Other platform SDK bindings
remain future work.

Unified playback preparation is now exposed at `/v1/playback/prepare`: it resolves
addon torrent sources, selects the explicit/addon/largest file, restores resume
state and issues media links. Direct sources return validated player headers.
The current client-development scope is Linux only; other platform clients and
bindings are deferred.

See [README.md](README.md) for running the server and its current limitations,
[architecture decisions](docs/architecture.md), and the
[compatibility status](docs/compatibility.md).

## Confirmed implementation direction — 2026-09-08

These decisions supersede conflicting proposals in the original planning notes below.
The original notes are retained as background. Implementation has started; see
README.md and docs/architecture.md for the implemented scope and remaining work.

### Initial deliverable

Build the shared Rust application core and a headless companion server built on
that core first. Full client UIs come afterward. Implement our own application
core and Stremio-compatible addon client; do not depend on Stremio's application
core or services. Support addons implementing the Stremio addon SDK protocol;
this does not require running the Node.js SDK inside Madari.

### Target clients

**Current implementation scope: Linux only.** Build the GTK4/libadwaita Linux
client when platform work starts. Do not implement other platform clients or
bindings in this phase. Keep the shared core portable; the other platforms below
remain future architectural targets, not current deliverables.

| Platform | UI | Integration |
| --- | --- | --- |
| Linux | GTK4 + libadwaita, Rust | Direct Rust core API |
| Windows | WinUI | Native core bindings |
| macOS | SwiftUI | Native core bindings |
| iOS | SwiftUI | Native core bindings; companion server for torrents |
| Android | Jetpack Compose | Native core bindings |
| Web | React + TypeScript + Meta Astryx | Portable core through WASM; companion server for torrents |

Share business logic across all platforms. Players and persistence adapters are
platform-specific. SQLite is a candidate for native persistence, not a requirement
for every platform. Kotlin Multiplatform is no longer the prescribed mobile UI
strategy; Android and Apple clients use their respective native UI stacks.

### Linux profiles — confirmed

- Regular profiles have optional PINs protecting entry and settings.
- Kids profiles use a PIN-protected regular profile as guardian; only adult-chosen
  addons are available. Addon changes, sharing and leaving kids mode require PIN
  authorization. Kids mode persists across restart.
- Sharing links one installation; configuration changes affect all linked
  profiles. Enabled state and ordering remain profile-specific.
- Library, history/progress and resume state are separate for each profile.
- SQLite persists Linux profiles; PINs use salted Argon2id with attempt throttling.

### Torrent delivery and companion server

- An existing open-source torrent engine may be used after evaluating its license,
  embedding API, supported targets, cancellation, file selection, and seeking.
- Stream torrents while downloading; do not require a completed download first.
- Keep the torrent engine behind an interface so it is not a dependency of the
  portable core or its browser build.
- The web does not run a native torrent engine. It receives playable media from
  the companion server.
- iOS uses the companion server for torrent playback initially. Native engine
  support on iOS is unverified, not declared technically impossible.
- The companion server streams media and uses FFmpeg for transcoding. Prefer
  direct streaming when compatible with the player; transcode when needed.
- Support companion servers on the same machine, the local network, and remote
  hosts, with API-key authentication. Specify authenticated media delivery and
  secure remote transport during server API design.
- FFmpeg, torrent delivery, and serving media belong to runtime/media adapters
  and the companion server; portable application logic remains independent.
- Playback itself belongs to platform-specific players. Mock playback is not a
  requested deliverable or an acceptance milestone.

### Revised acceptance direction

A headless integration should install an addon, browse/search, retrieve metadata
and sources, select a torrent file, and deliver playable media while downloading.
The companion server must support authenticated media access and an FFmpeg
transcoding path. Persist application state through a storage adapter and recover
it on restart. Preserve per-addon failure isolation. Validate actual media delivery
without requiring a full UI or a mock-player application.

### Remaining engineering decisions

- Torrent engine selection and licensing/dependency audit.
- FFmpeg integration, distribution, capability negotiation, and stream format.
- Linux GTK/player integration; other bindings deferred.
- Native/browser persistence implementations and credential storage.
- Server API, API-key lifecycle, pairing/configuration, TLS deployment, and media
  request authentication compatible with platform players and browsers.
- Metadata identity/merge policy and concrete compatibility fixtures.

## Original planning notes (historical)

The sections below predate the confirmed decisions above. In particular, the
Stremio-core reuse evaluation, mock-player milestone, KMP mobile prescription,
and deferral of torrent delivery and the companion server are superseded.

## Status

Planning only. No application code has been implemented.

Madari is a Stremio alternative with a native Linux client first and a shared Rust application core for future mobile and web clients.

## Product direction

- **Linux:** native GTK4 + libadwaita UI, written in Rust using gtk4-rs. This is the recommended desktop stack, not a webview or Tauri application.
- **Mobile:** Kotlin Multiplatform, consuming the shared Rust core through native bindings.
- **Web:** React + TypeScript + Astryx, consuming a WASM build of the core.
- **Scope now:** plan the full application core, not the clients or a media engine.
- **Compatibility target:** current Stremio addon protocol plus tested common quirks. Legacy transports are not part of the initial milestone.

Astryx is React-based and belongs to the web client, not the native GTK or Kotlin UI. Libadwaita provides a GNOME-style experience; it works on other Linux desktops but does not automatically adopt their widget styling.

## Architecture

```text
Linux                        Mobile                       Web
GTK4 + libadwaita             Kotlin Multiplatform         React + Astryx
Direct Rust API               Native bindings              WASM bindings
       \                            |                         /
        +---------------------------+------------------------+
                                    |
                              Madari Core
                    Addons · discovery · metadata
                    library · progress · playback plans
                                    |
                             Platform adapters
                       HTTP · storage · time · tasks
```

Share business logic and contracts, not necessarily runtime and storage implementations.

- Portable core: no GTK, GLib, Tauri, browser, or platform-player dependencies.
- Native adapters: Tokio, native HTTP client, SQLite; exact libraries remain to be selected.
- Browser adapters: browser fetch, IndexedDB, browser-compatible task execution.
- Client adapters translate commands and subscriptions into each platform's UI model.
- Local-first operation; accounts and cloud sync are a separate future phase.

### Linux integration

- Call the core directly from Rust; no JSON or IPC bridge is needed for the in-process Linux API.
- Keep GTK objects on the GTK main thread.
- Run networking and blocking persistence work away from the UI thread.
- Deliver core updates through a thin adapter onto GTK's main loop.
- Start with gtk-rs; evaluate Relm4 only if its application model is useful.
- Flatpak is the initial packaging recommendation; packaging is outside the current core scope.

## Core responsibilities

| Area | Responsibilities |
| --- | --- |
| Addons | Install, configure, enable/disable, reorder, refresh, remove |
| Discovery | Catalogs, search, filters, pagination, incremental results |
| Metadata | Details, episodes, video IDs, provider provenance |
| Streams | Fetch sources, preserve source data and behavior hints, identify playback requirements |
| Subtitles | Fetch and merge addon-provided and stream-provided subtitles |
| Library | Saved items, watched state, history, continue watching |
| Playback coordination | Resume position, session tracking, next episode, source continuity |
| Preferences | Languages, subtitles, addon order, playback policy |
| Persistence | Local state, migrations, cache, export/import |

### Outside the core

- UI layout, widgets, themes, and navigation presentation.
- Video decoding, rendering, and audio output.
- OS integration and external application launching.
- Torrent downloading, local media serving, transcoding, NNTP delivery, and archive extraction.
- Cloud account and synchronization services in the initial release.

Optional media capabilities should be accessible through interfaces without becoming dependencies of the portable application core.

## Stremio addon compatibility

Stremio addons generally expose remote JSON services. Consuming them does not require executing addon code locally or embedding Node.js.

### Resources

Support all five documented resources:

- `catalog`
- `meta`
- `stream`
- `subtitles`
- `addon_catalog`

### Manifest and routing behavior

- Handle resource declarations in string and object form.
- Respect resource-specific content types and ID prefixes, including their override semantics.
- Treat catalogs as manifest-declared resources rather than applying ordinary item ID-prefix filtering to them.
- Support custom content types and opaque item/video IDs; do not assume IMDb IDs or only movie/series types.
- Support search, skip, genre, required extras, declared options, and custom catalog extras.
- Construct protocol resource paths and encoded extra arguments correctly.
- Preserve configured manifest URL paths and other meaningful URL components; test URL derivation explicitly.
- Support configuration-required and external configuration-page flows.
- Support multiple configured installations of the same addon.
- Preserve unknown fields where useful for forward compatibility.
- Return per-addon failures without failing an entire aggregated view.

Use a unique **installation ID**, not `manifest.id`, as the identity of an installed addon. Two configurations of one addon must have separate state and cache entries.

Prefer typed protocol models plus explicit normalization into domain models. Compatibility fixes must be narrow and backed by fixtures, rather than silently accepting arbitrary invalid data.

### Subtitle requests

The dedicated subtitle-handler documentation specifies a video ID with optional `videoHash`, `videoSize`, and `filename` extras. The general protocol page contains older/conflicting examples. Resolve such discrepancies using dedicated resource documentation, SDK/client behavior, and compatibility tests rather than treating every example as authoritative.

### Protocol support is not universal playback

The reviewed stream documentation includes:

- Direct URLs, including protocols that not every player supports.
- Torrent `infoHash`, optional file selection, and peer-discovery sources.
- YouTube identifiers.
- External URLs.
- NZB sources and server information.
- Archive sources: RAR, ZIP, 7zip, TGZ, and TAR.
- Embedded subtitles and behavior hints such as proxy headers, `notWebReady`, `bingeGroup`, video hash, size, and filename.

Recognize and preserve documented source forms even when a client cannot play them. A capability-aware playback plan should report one of:

```text
Playable directly
Requires media service
Requires external application
Unsupported by this client
```

Do not label an addon fully playable merely because its manifest and stream response parse successfully. Track protocol compatibility and playback capability separately.

Legacy JSON-RPC, IPFS, and IPNS transports are outside the initial milestone. Unknown transports should produce clear unsupported-transport errors.

## Client-facing API

Expose commands, queries, and subscriptions with typed, versioned contracts. Names below are illustrative, not a frozen API.

### Commands

```text
InstallAddon
SetAddonEnabled
ReorderAddons
RemoveAddon
AddToLibrary
RemoveFromLibrary
RecordPlaybackProgress
```

### Queries

```text
GetCatalog
Search
GetMetadata
GetStreams
GetSubtitles
PreparePlayback
```

### Subscriptions

```text
AddonsChanged
SearchResultsUpdated
LibraryChanged
PlaybackSessionChanged
```

### Contract requirements

- Request IDs, cancellation, and stale-result protection.
- Incremental results as individual addons respond.
- Structured errors with addon attribution.
- Snapshot and revision semantics so subscribers can initialize and recover.
- Bounded event queues, explicit unsubscribe, and defined overflow behavior.
- Stable DTOs at binding boundaries rather than exposing internal implementation structures.
- Consistent semantics across direct Rust, native bindings, and WASM.

### Playback boundary

`PreparePlayback` returns a playback plan containing the source, required capabilities, applicable headers, subtitles, resume position, and relevant source hints.

The client/player adapter performs playback and reports session events, progress, completion, and errors. The core owns resume and next-episode policy; the player owns decoding and rendering.

The Linux playback backend uses **libmpv with GTK GLArea** for embedded rendering, subtitles, seeking and fullscreen. Startup must preserve `LC_NUMERIC=C`. This choice does not affect addon or library logic.

## Native and browser bindings

### Kotlin Multiplatform

Expose a narrow common Kotlin API with platform-specific implementations.

Evaluate:

- UniFFI-generated Kotlin bindings for Android.
- An iOS C interop bridge or a proven KMP-capable binding generator.
- Async operations, cancellation, event delivery, object lifetime, and error conversion across both platforms.

Do not assume ordinary UniFFI Kotlin output works on Kotlin/Native. Validate Android and iOS integration with a small prototype before committing to a binding approach.

### Web

Compile the portable core to WASM and run it in a Web Worker. Generate or validate TypeScript contracts from the Rust-facing API to prevent drift.

Browser limits remain in force:

- CORS and mixed-content restrictions.
- Restricted request headers.
- No direct native torrent or NNTP engine.
- Codec and container limitations.

An optional companion service may provide unavailable capabilities later. WASM does not bypass browser security or playback restrictions, and a companion service is not required for the initial portable core design.

## Suggested repository layout

```text
NOTES.md
apps/
  linux/                 GTK4/libadwaita client, later
crates/
  madari-model/          Domain types and public contracts
  madari-addon/          Protocol models, routing, normalization
  madari-core/           Application state and orchestration
  madari-native/         Native networking, storage, runtime
  madari-wasm/           Browser adapters and JS bindings
  madari-ffi/            Mobile-facing native API
tests/
  fixtures/              Sanitized manifests and resource responses
  compatibility/         Protocol and known-quirk cases
  integration/           Mock addons and end-to-end core scenarios
```

This is a proposed layout, not existing code. Keep library and playback coordination as modules inside `madari-core` initially; split crates only when justified. Cargo integration-test placement and test-support crates should be settled during workspace setup.

## Reliability, identity, and security

- Bound concurrency and response sizes; enforce timeouts and cancellation.
- Deduplicate identical in-flight requests without conflating addon configurations.
- Respect HTTP cache policy and supported addon cache hints with documented precedence.
- Avoid presenting expired cached playback URLs as fresh.
- Partition caches by installation/configuration and all request inputs that affect responses.
- Preserve deterministic ordering despite variable addon response times.
- Preserve addon provenance when aggregating metadata, catalogs, and streams.
- Define identity and merge policy explicitly; opaque custom IDs may collide across providers and must not be blindly merged.
- Treat configured URLs, proxy headers, and media-server credentials as secrets; redact logs and diagnostics.
- Keep secret storage behind a platform adapter; define safe export behavior separately from ordinary settings export.
- Validate untrusted URLs and allowed schemes; constrain redirects and credential forwarding.
- Require explicit permission for local-network addons.
- Apply stronger SSRF protections if a remote companion service is introduced.
- Never automatically install addons discovered through `addon_catalog`.
- Isolate malformed or unavailable addons from the rest of the application.

## Existing Stremio core: evaluate before rewriting

Stremio already publishes a Rust application core with addon types, transport, state models, and a WASM bridge. Its repository describes it as MIT-licensed.

Before implementation:

1. Review source, license files, dependencies, and public API stability.
2. Evaluate reuse of protocol types and transport behavior against an independent implementation.
3. Identify assumptions about Stremio services, accounts, and application state.
4. Record an architecture decision: dependency, selective reuse with license compliance, fork, or independent implementation.

Do not commit to reusing the full application model without that review. The repository overview was consulted during planning; a detailed source/dependency audit has not yet been performed.

## Delivery phases

### Phase 0 — Foundation validation

- Define the resource, transport, source, and quirk compatibility matrix.
- Resolve reuse versus independent implementation.
- Define core contracts and platform interfaces.
- Prototype native, WASM, and Android/iOS binding paths with a small shared operation and event subscription.

**Exit:** portability and binding assumptions are demonstrated, and architecture decisions are recorded.

### Phase 1 — Addon engine

- Installation lifecycle and configuration handling.
- All five resource types.
- Manifest routing, URL construction, parsing, normalization, and structured errors.
- Mock-addon tests and sanitized compatibility fixtures.

**Exit:** a headless client can install an addon, browse, search, fetch details, streams, subtitles, and addon catalogs.

### Phase 2 — Application core

- Library, preferences, history, and resume state.
- Playback plans and next-episode/source-continuity policy.
- Persistence, migrations, caching, subscriptions, and cancellation.

**Exit:** state survives restart and a mock player drives a complete viewing session, including progress and completion.

### Phase 3 — Shared client SDKs

- Direct Rust facade usable by GTK with a minimal integration harness.
- TypeScript WASM package.
- KMP-facing native API.
- Contract tests covering equivalent operations across bindings.

**Exit:** the same core scenario works through each integration without duplicated business logic. Production clients remain separate work.

### Phase 4 — Compatibility hardening

- Curated real-addon smoke tests, separate from deterministic fixture-based CI.
- Explicit, fixture-backed common quirks.
- Offline behavior, malformed responses, slow addons, cache isolation, cancellation, and secret-redaction tests.

**Exit:** publish a tested compatibility matrix and known limitations instead of an unqualified claim that every addon and source works.

## Initial acceptance scenario

A headless Linux harness should be able to:

1. Install a configured addon.
2. Load a catalog and perform a search.
3. Fetch metadata and select a video/episode.
4. Fetch streams and subtitles.
5. Produce a capability-aware playback plan.
6. Receive progress from a mock player.
7. Restart and recover library and resume state.
8. Complete the flow even if another installed addon fails.

Implement this before building the full GTK UI or a media-delivery engine.

## Open decisions

- Reuse strategy for existing Stremio Rust code.
- Exact Rust dependencies and supported toolchain.
- Mobile binding implementation, especially Kotlin/Native on iOS.
- Linux player backend: libmpv embedded in GTK GLArea.
- Metadata conflict resolution and cross-provider identity policy.
- Secure persistence and export rules for configured URLs and credentials.
- Which optional media-service capabilities to implement after the initial core.

## References

- Stremio addon SDK documentation: https://github.com/Stremio/stremio-addon-sdk/blob/master/docs/README.md
- Protocol: https://github.com/Stremio/stremio-addon-sdk/blob/master/docs/protocol.md
- Manifest: https://github.com/Stremio/stremio-addon-sdk/blob/master/docs/api/responses/manifest.md
- Stream sources and hints: https://github.com/Stremio/stremio-addon-sdk/blob/master/docs/api/responses/stream.md
- Subtitle request parameters: https://github.com/Stremio/stremio-addon-sdk/blob/master/docs/api/requests/defineSubtitlesHandler.md
- Existing Stremio Rust core: https://github.com/Stremio/stremio-core
- Astryx: https://astryx.atmeta.com/

The links above were consulted for planning. Pin protocol fixtures and source revisions during implementation because upstream documentation can change.
