# Native shell implementation and bridge validation

Date: 2026-11-03. This covers the native shell portion of B5. Actual macOS menu dispatch, system clipboard, VoiceOver, microphone finalization, and signed update installation still require the G2/G3 platform checks.

2026-11-04 follow-up: [actual Linux menu and lifecycle checks](native-menu-validation.md) now cover menu dispatch, language switching, focused draft undo/redo, native file-picker cancellation, meeting restoration, window modes and idle quit/close with a cold restart. These extend the bridge evidence below; macOS, tray, clipboard, active-audio and signed-update checks remain separate.

2026-11-04 clipboard follow-up: [actual Linux clipboard checks](native-clipboard-validation.md) fix native clipboard dispatch and canvas-aware Select All, with real text/modal/canvas operations and retained draft/storage checks. Native Wayland, macOS, image/file clipboard, tray, audio and signed installation remain outside that result.

2026-11-05 renderer recovery follow-up: [actual Linux recovery checks](native-webprocess-validation.md) cover native English/Chinese warnings, reloading saved meetings after WebKit termination, continuing to save, canceling pending requests, recovery Quit and cold restart. Normal exit keeps its save handshake; a stopped interface now offers an explicit native recovery or quit choice. The underlying crash trigger, physical audio and macOS remain unverified boundaries.

## Implemented behavior

`src-tauri/src/shell.rs` creates localized App, File, Edit, View, Window, and Help menus, including Settings (`Cmd/Ctrl+,`), import (`Cmd/Ctrl+O`), export, meeting actions, and help. Tray labels follow the selected language. Standard cut, copy, paste, and select-all remain native predefined items so they keep the system responder's focus.

Closing the main window, choosing Quit, or requesting normal application exit creates one pending request with a monotonic ID. Duplicate requests are ignored until the frontend answers. The frontend waits for meeting restoration, registers its listener, and reads any pending startup request. It stops audio, cancels pending work while retaining transcript text, and flushes the meeting before acknowledging inside the transition lock. A stop/save failure acknowledges refusal and leaves the current meeting open. Only a matching current ID can authorize close/exit; there is no timeout that forces data loss.

Undo and redo use application actions because the locally installed Muda 0.20.0 macOS predefined items invoke responder selectors (`undo:`/`redo:`), which cannot establish access to the application's graph history. The frontend preserves focused text editing first, including text fields inside Settings, then blocks background edits under a modal. Hand-drawn mode invokes its history directly. Free edit dispatches one keyboard event to the Excalidraw surface; the actual bundled editor was checked for one-step undo and redo. Cut/copy/paste/select-all retain native implementations.

## Reproducible checks

Native state tests compile against the installed Tauri 2.12.5 API:

```sh
cargo test --manifest-path src-tauri/Cargo.toml shell::tests --lib -- --nocapture
```

Both tests passed: duplicate/stale acknowledgments and refusal/retry/approval state transitions.

The browser check runs the actual production App with a controlled native IPC fixture. It neither exits a real application nor records audio or changes real meeting files:

```sh
node node_modules/vite/bin/vite.js build --outDir /tmp/boardcast-native-shell-dist
node node_modules/vite/bin/vite.js preview --outDir /tmp/boardcast-native-shell-dist --host 127.0.0.1 --port 1424
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1424 check-native-shell.cjs
```

The isolated production run passed startup restoration before pending acknowledgment, stop failure refusing exit, save failure refusing exit, duplicate request suppression, and the final transcript sentence being saved before successful acknowledgment. The transition remains locked at acknowledgment. The same check passed Settings text undo/redo without moving focus, hand-drawn graph undo/redo, and exactly one Excalidraw shape undone then restored. No page errors were observed. Evidence: [native-shell-check.json](evidence/implementation/native-shell-check.json).

The extended check also passed synthetic composition isolation for help, hold-Space capture, and menu Escape; the single live transcript caption and its persisted opt-out; and the end-and-review save barrier. Stop failure and disk-full keep review's dialog open with recovery actions. A deliberately unresolved save cannot show the review summary; resolving it preserves the final transcript sentence before entering review. Synthetic composition flags verify the event guards, not real Chinese/Japanese input-method delivery.

The final production run also started an input preview in the real App's Preparation dialog and requested native close while no meeting recording was active. The App invoked preview cleanup before acknowledging exit (`previewStoppedBeforeExit: true`). Excalidraw checks now wait for the explicit `.pencil-stage[data-ready="true"]` gate; the run initially exposed a first-gesture race while font/scene initialization was pending, and passed after that surface was made inert until initialization finished.

The final default Rust library suite passed 50 tests with 10 explicitly ignored hardware, download, remote-service, and corpus tests. The opt-in isolated virtual-input test passed separately. Raw default-suite output: [native-library-tests.txt](evidence/implementation/native-library-tests.txt).

Those tests compile with `cfg(test)` and did not establish a production native build. A later isolated production build caught the input preview calling test-only `Mixer::pop`; that call now uses the production `pop_tracks` and `mix` path. After the correction, both `cargo check --manifest-path src-tauri/Cargo.toml --lib` and `cargo build --manifest-path src-tauri/Cargo.toml --lib` passed without the test configuration. The latter compiles and links the Linux library; its raw output is [native-production-build.txt](evidence/implementation/native-production-build.txt). This check does not establish a packaged Tauri launch or macOS build/runtime behavior.

An unchanged restored meeting is allowed to acknowledge without a redundant write; the test requires restoration before acknowledgment and preservation of its contents. Edited meetings and final audio text still require completed save before acknowledgment. This distinction follows the storage checkpoint optimization and does not relax the dirty-data check.

Development HMR changed the mounted App during an earlier run, so the final evidence comes from a separate production output directory and preview process. The controlled IPC result proves frontend ordering and UI routing, not macOS native event delivery. Actual native close/quit behavior and focused system editing remain explicitly unverified on macOS.
