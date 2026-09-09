# Compatibility status

This is the first implementation, not a claim of universal addon or playback support.

| Area | Implemented and tested | Limits |
| --- | --- | --- |
| HTTP addon protocol | Configured paths/query preservation; escaped resource segments/extras | HTTP(S) only; bounded redirects with DNS/local-network validation at every hop; no legacy/IPFS/IPNS |
| Manifest routing | String/object declarations; explicit resource overrides; opaque custom types/IDs | Missing detailed `types` means unsupported; empty ID prefixes are unrestricted |
| Catalogs | Manifest-declared catalogs; search extras; required-extra checks | Linux multi-catalog search and declared skip pagination supported; legacy extras supported; option-value enforcement pending |
| Resources | `catalog`, `meta`, `stream`, `subtitles`, `addon_catalog` | Dedicated subtitle extras forwarded as supplied; no subtitle merge yet |
| Installations | Duplicate configurations isolated by ID; enable/remove/reorder; persistence | Linux configured-URL replacement available; provider configuration forms remain external |
| Failures | Per-provider result/error, deterministic aggregation order | Native HTTP retries and profile metadata cache; no universal resource cache |
| State | Library/progress persistence, revision conflicts, provider-scoped resume | No cloud sync or import/export; next-episode policy is local |
| Linux profiles | Regular/kids profiles, PIN-gated entry/settings, restart-persistent kids mode; linked addon configurations and separate library/progress | Device-local SQLite; no rating filter, cloud sync, profile deletion or PIN recovery UI |
| Linux playback | Catalog/filter browsing, episodes, source selection, library, direct HTTP(S), companion torrent/transcode delivery, real mpv progress/resume | Embedded GTK/libmpv playback, cached poster artwork and dedicated search; no automatic codec choice, playlist redirects or subtitle-addon discovery |
| Subscriptions | Snapshot/revision recovery; bounded overflow disconnect | No durable replay or cross-process notifications |
| Torrent delivery | v1 magnets/metainfo; file selection by index; real range streaming | Public DHT/tracker reliability and cross-native engine builds not verified |
| Unified preparation | Addon hash/trackers → magnet → file → ticket; saved resume; explicit/default selection; local tracker discovery tested | Hexadecimal v1 hashes only; extra DHT hints preserved, not applied; codec selection still manual |
| Transcoding | Real torrent → loopback HTTP → FFmpeg → fragmented MP4; offset seek | No direct-URL input, HLS, hardware acceleration, embedded subtitle selection |
| Source preservation | Raw source fields, trackers, subtitles and behavior hints retained | YouTube/NZB/archive/external sources do not imply server playability |
| Web portability | Portable core compiles as WASM | No JS package, IndexedDB/fetch adapters or browser runtime test yet |
| Native portability | Portable core is separated from native/media crates | Linux browsing and native mpv playback available; other clients/bindings pending |

Platform implementation is currently **Linux only**; other clients/bindings are
deferred. Historical compile checks are evidence about the portable boundary,
not a commitment to build those clients in this phase.

Protocol implementation was checked against the official
[protocol](https://stremio.github.io/stremio-addon-sdk/protocol.html) and
[manifest](https://stremio.github.io/stremio-addon-sdk/api/responses/manifest.html)
documentation on 2026-09-08. The checked-in fixtures are authored for Madari, not
copied addon responses. The source review below pins the upstream revisions;
upstream documentation is not vendored.

Deterministic integration coverage is in `apps/server/tests/end_to_end.rs`.
It covers all five resources plus catalog search, configured installations,
isolated failures, restart recovery, bearer authentication, media tickets,
real local torrent transfer, byte ranges, and FFmpeg output verified with ffprobe.
No real third-party addon compatibility smoke tests have run yet.

Unified preparation also follows the dedicated
[stream format](https://stremio.github.io/stremio-addon-sdk/api/responses/stream.html)
for info hashes, `fileIdx`, default largest-file selection, tracker sources and
header/subtitle hints. Its local tracker fixture uses the compact response format
from [BEP 23](https://www.bittorrent.org/beps/bep_0023.html). Direct-source preparation
does not imply companion HTTP proxy/transcode support.

## SDK and client behavior review (2026-09-08)

Reviewed the public SDK at commit `2728da3ee853207cd5ee200aabe15a08cc1d01d1`
and Stremio core at `ab28e8423689ef8cca166cc097b8d6c2bc89f3df`.
Madari remains an independent implementation; no Stremio runtime is embedded.

- Normalize legacy catalog extras, treat empty ID-prefix lists as unrestricted,
  accept null resource lists and skip malformed individual list entries.
  Wrong top-level response shapes still fail. See the upstream
  [manifest implementation](https://github.com/Stremio/stremio-core/blob/ab28e8423689ef8cca166cc097b8d6c2bc89f3df/src/types/addon/manifest.rs) and
  [response parser](https://github.com/Stremio/stremio-core/blob/ab28e8423689ef8cca166cc097b8d6c2bc89f3df/src/types/addon/response.rs).
- Follow the SDK's standard 100-item skip pagination; a shorter page ends loading.
  See [catalog requests](https://github.com/Stremio/stremio-addon-sdk/blob/2728da3ee853207cd5ee200aabe15a08cc1d01d1/docs/api/requests/defineCatalogHandler.md).
- Store card previews with library/progress independently of expiring metadata.
  `lastVideosIds` is a series-update feed, not a universal movie-metadata endpoint.
  Only installed, enabled catalogs are considered; declared types, prefixes,
  required extras and `optionsLimit` apply. Select recent IDs before sorting the
  request for stable URLs. See [ID request planning](https://github.com/Stremio/stremio-core/blob/ab28e8423689ef8cca166cc097b8d6c2bc89f3df/src/types/addon/request.rs).
- Calendar follows saved library titles through `calendarVideosIds`, keeps loaded
  results while navigating dates, and uses UTC release days. See the upstream
  [calendar model](https://github.com/Stremio/stremio-core/blob/ab28e8423689ef8cca166cc097b8d6c2bc89f3df/src/models/calendar.rs).
  Madari exposes this through the shared Rust core and Linux navigation; the
  calendar has no dedicated companion HTTP endpoint or persistent calendar cache.

This is targeted compatibility, not full client parity. Addon subtitle discovery
with video hashes, provider-directed HTTP cache lifetimes, and unsupported source
formats remain incomplete. Older history without a stored preview still depends
on available cached/addon metadata. Tests use local authored fixtures; this review
sent no private viewing IDs to external addons.
