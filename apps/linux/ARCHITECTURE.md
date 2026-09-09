# Linux app source layout

`src/main.rs` is the executable entry point. Implementation lives under `src/app`,
so feature modules can share the private `Ui` state without exposing it outside
the app.

| Location | Responsibility |
| --- | --- |
| `app/mod.rs` | Shared `Ui` state, request lifecycle, page cleanup and refresh |
| `app/bootstrap.rs` | Data paths, window construction, startup and shutdown |
| `app/browsing/` | Home/featured content, catalogs, search, details, episodes, calendar and source selection |
| `app/playback/mod.rs` | Playback target and session context |
| `app/playback/engine.rs` | mpv worker, commands and playback events |
| `app/playback/controls.rs` | Player controls, menus and statistics |
| `app/playback/embedded/` | GTK video surface, player lifecycle, fullscreen, mini-player and PiP |
| `app/playback/internal_stream.rs` | In-process torrent-to-mpv stream bridge |
| `app/playback/trakt.rs` | Ordered playback scrobbles and confirmed Trakt status |
| `app/playback/up_next.rs` | Intro and next-episode timing rules |
| `app/profiles/` | Profile picker and profile-session actions |
| `app/settings/` | Settings pages, access controls and addon configuration/sharing |
| `app/browsing/trakt.rs` | Cached Trakt lists, history browsing and addon artwork |
| `app/settings/trakt.rs` | Trakt device login and account/sync actions |
| `app/transfers/` | Torrent management UI and file-manager actions |
| `app/interface/` | Shared widgets, dialogs, navigation, artwork, loading, motion, responsive layout and CSS |

Keep new feature code in its owning module. Shared GTK presentation belongs in
`interface`; reusable torrent/network/storage logic belongs in workspace crates,
not in the desktop UI. Cross-feature references use explicit `crate::app::…`
paths. Feature entry points are visible within `crate::app`; local implementation
details remain private.

Assets stay in `apps/linux/assets`. Rust embeds assets using
`CARGO_MANIFEST_DIR`, so moving a module does not change its asset lookup.

Unit tests stay beside the implementation. Larger rendering and process fixtures
live under the corresponding feature's `tests/` directory. They remain unit-test
modules because they exercise private implementation details.

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p madari-linux --all-targets -- -D warnings
cargo test --locked --offline -p madari-linux
```

Display-dependent tests are ignored by default. Run each in its own test process
because GTK initialization is thread-affine. For example:

```sh
env GDK_BACKEND=x11 GSK_RENDERER=cairo LIBGL_ALWAYS_SOFTWARE=1 LP_NUM_THREADS=2 \
  xvfb-run -a -s '-screen 0 1600x1000x24' \
  cargo test --locked --offline -p madari-linux \
  app::interface::responsive_tests::all_screen_layouts_fit_small_and_large_windows \
  -- --ignored --nocapture
```

Trakt HTTP and data parsing live in `crates/madari-native/src/trakt/`; profile-scoped
secret storage lives in `crates/madari-native/src/profiles/trakt.rs`. See
[Trakt setup and behavior](../../docs/trakt.md).
