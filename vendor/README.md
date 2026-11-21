# vendor/

Third-party crates the build uses from here instead of crates.io.

## tao/

tao 0.37.0 exactly as published on crates.io (Apache-2.0 OR MIT, see
`tao/LICENSE` and `tao/LICENSE.spdx`), with one change in
`src/platform_impl/windows/event_loop.rs`, marked `ailimits:`: the fix of
https://github.com/tauri-apps/tao/pull/1394 (the event loop could block on a
user event posted before it was queued). The workspace pins `tao = "=0.37.0"`
because 0.37.1 and later take the focus on `set_visible(true)`; remove this
copy and the `[patch.crates-io]` entry in Cargo.toml once a tao release has
the fix and not that change.
