# Trakt on Linux

Open **Settings → Profile → Trakt → Connect**, approve the displayed code on
Trakt, and Madari imports the account into the current profile. No callback
server, listening port, Trakt password, or manually copied user token is needed.

## Application setup

Register a Trakt application at <https://trakt.tv/oauth/applications>. Trakt's
device-token and refresh endpoints require both its client ID and client secret.
Set the redirect URI to `urn:ietf:wg:oauth:2.0:oob`, or enter the exact URI you
registered when configuring Madari. It is used for token refresh; Madari never
starts a redirect listener.

For a local build, enter application credentials in the setup dialog. The secret
field is masked. Alternatively, provide these environment variables to the app:

- `MADARI_TRAKT_CLIENT_ID`
- `MADARI_TRAKT_CLIENT_SECRET`
- `MADARI_TRAKT_REDIRECT_URI` (defaults to `urn:ietf:wg:oauth:2.0:oob`)

When application credentials are provided by the launcher, users go straight to
device approval. Do not commit real credentials to the repository. Application
credentials distributed with a desktop application cannot be treated as
confidential. A production release still needs its own registered Trakt app.

## Imported data

**My list → Trakt** has a selector for watchlist, watched history, and each
personal list, including private lists available to the connected account.
Lists scroll horizontally and load cards in batches. All API pages are fetched;
the UI's card batch size does not truncate the import. Episode and season list
items retain their parent show and episode/season identity. Unsupported entries
are counted in the sync result.

Card clicks resolve full details through enabled metadata addons using **type +
ID**: Trakt movies use `movie`; shows and parent shows of season/episode entries
use `series`. IMDb IDs (`tt…`) are tried first, then known TMDB/Trakt IDs only when
an addon declares support. A response must match both the requested type and ID;
a Trakt preview alone is not a successful addon lookup. If all compatible addons
fail, the details page shows a retryable error. Cards remain visible independently
of addon availability. Posters, backgrounds and
descriptions are imported with Trakt’s full metadata,
and Trakt image URLs are fetched through Madari’s disk artwork cache. Installed
addons enrich metadata and supply episode lists and playback sources. Trakt is
not a streaming provider. Lists stay separate from locally saved titles, so
refreshing Trakt never removes local saves. The refresh button beside the list
selector updates only the Trakt section, including older caches without artwork.

On opening a title's details, watched history is matched to the addon’s actual
movie/episode IDs and imported as completed flags. Existing local positions,
including rewatches, take priority. No runtime or resume timestamp is invented.
Imported flags are retained as local history; removing an event on Trakt or
disconnecting does not erase those flags. Importing Trakt playback positions,
ratings, recommendations, check-ins and editing remote lists are not included.

The app syncs on connection, on profile opening if the cache is at least 15 minutes
old, and through **Sync now** in settings. Cached lists remain available offline.
A failed or rate-limited import keeps the previous complete cache. Token refresh
is serialized, and replacement refresh tokens are persisted before fetching data.

## Playback scrobbling

Connected profiles automatically report start/resume, pause (including buffering),
and completion while the video plays. At 95% Madari sends a watched scrobble once;
stopping earlier saves progress with a pause event so Trakt’s lower completion
threshold does not mark the title watched prematurely. Movie IDs are explicit;
series use addon season/episode numbers to resolve the exact Trakt episode ID,
which is cached for the playback session. Unsupported IDs are shown as unmatched.

A compact badge beside player stats displays **Watching**, **Paused synced**, or
**Watched synced** only after Trakt acknowledges the corresponding event. Pending
requests show **Syncing**, and failures show **Not synced** with an explanation in
the tooltip. Failures retry after 30 seconds during playback; they are not claimed
as successful or stored in an offline outbox. Reporting never blocks decoding,
seeking, fullscreen or picture-in-picture. One ordered queue keeps outgoing-episode
updates ahead of incoming-episode updates, and each report is bound to its original
profile/account connection even if the user switches profiles.
Quitting gives the reporting queue up to five seconds to finish pending requests.

## Storage and disconnect

Account credentials, OAuth tokens and cached Trakt data live in a separate
`profile_trakt` table in the profile SQLite database, inside the app's private
user data directory. They are not included in Core/public snapshots. This is
local filesystem protection, not keyring encryption; protect database backups
like account credentials. Each profile has its own connection, and account
changes require access to that profile's settings.

**Disconnect** deletes that profile's account and cache and attempts remote token
revocation. Local saves and playback history remain. If offline revocation fails,
the app explains that the connection can also be removed in Trakt's app settings.

## Verification

Mock HTTP tests cover device pending/slow-down/success/denial/expiry, token refresh,
pagination, complete list/history imports and scrobble acknowledgements. Storage tests
cover profile isolation, public snapshot exclusion and safe token rotation across
profile switching. Episode matching tests cover custom addon IDs and rewatches.
The desktop layout test includes large Trakt lists at small window widths.

Scrobbling is tested against mock endpoints; tests do not add entries to a real
account’s watch history.

References: [authentication](https://docs.trakt.tv/reference/auth),
[device polling](https://docs.trakt.tv/reference/postoauthdevicetoken),
[token refresh](https://docs.trakt.tv/reference/postoauthtoken),
[watchlist](https://docs.trakt.tv/reference/getsyncwatchlistget),
[history](https://docs.trakt.tv/reference/getsynchistoryget),
[personal lists](https://docs.trakt.tv/reference/getuserslistspersonal).
