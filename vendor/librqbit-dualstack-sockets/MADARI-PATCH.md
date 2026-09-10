Based on crates.io librqbit-dualstack-sockets 0.7.0 (upstream source and license
retained).

Madari patch: `src/bind_device.rs` treated only macOS as an Apple platform.
It selected the `bind_device_by_index_v4/v6` implementation under
`cfg(target_os = "macos")` and sent every other non-Windows target to the Linux
`SO_BINDTODEVICE` path. iOS and the other Apple platforms therefore failed to
compile:

    error[E0599]: no method named `bind_device` found for reference `&Socket`

`socket2` 0.6 exposes `bind_device_by_index_v4`/`bind_device_by_index_v6` on
`ios`, `visionos`, `macos`, `tvos` and `watchos`, backed by `IP_BOUND_IF` and
`IPV6_BOUND_IF`. The patch widens both `cfg` predicates to
`any(target_os = "ios", target_os = "macos")`, which is the platform set Madari
actually targets. Linux, Android, Windows and FreeBSD behaviour is unchanged.

`src/bind_device/tests.rs` still branches on `target_os = "macos"` alone. That
only affects test expectations, not compilation, and no iOS test target is built
here, so it was left untouched to keep the diff minimal for upstream.
