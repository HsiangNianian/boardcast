# Local X11 release fix

Source: `global-hotkey` 0.8.0 from crates.io, also at
https://github.com/tauri-apps/global-hotkey/tree/2a620bf3852008b568f6d36c2baedcc3dd0822f2.
Original licenses, source and manifest (`Cargo.toml.orig`) are preserved.
Only unused example targets and their development dependencies are omitted from
the build manifest.

X11 release requests were buffered without a flush/round trip. After a failed
registration, a partially grabbed combination could keep consuming key events;
unregistering a working shortcut had the same delay. Check each release request
before reporting success or returning the registration error. Other platforms
are unchanged.

The ignored X11 regression requires an isolated display. With that display's
`DISPLAY` and `BOARDCAST_TEST_X11=1`:

```sh
cargo test --manifest-path src-tauri/Cargo.toml -p global-hotkey --lib releases_failed_and_removed_grabs_before_returning -- --ignored --test-threads=1
```

shortcut: this vendored patch is required until an upstream release synchronizes X11 release requests; remove it when that release passes the same regression.
