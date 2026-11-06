# B0/B1 runtime validation — 2026-11-03

The production app was built with the installed TypeScript and Vite binaries, including the standalone player, then exercised in an isolated Chromium context at `http://127.0.0.1:1430`.

The reproducible check is `scripts/ui/check-scene-roundtrip.cjs`, run through `scripts/ui/run-checks.cjs`. It seeds a meeting fixture in IndexedDB and sends a controlled model response through a network route. Every edit, mode/page switch, undo/redo, replay, and fork uses the actual visible UI; the test does not invoke App action functions.

Verified:

- Renaming a generated node in PC mode updates its semantic graph and saved scene.
- A real pointer drag persists its new coordinates. One undo restores the complete gesture, and redo restores its position.
- Renaming the node again in PAD replaces its earlier PC text override.
- A subsequent model response that attempts a conflicting rename while adding another node preserves the user's name and accepts the new node.
- Without an actual model candidate, the UI offers only “Reset styling”; resetting keeps the user's semantic text, and undo restores the earlier visual ownership. After a conflicting model response, “Use model suggestion” adopts that real candidate, and one undo restores the user's text and the available candidate.
- PAD deletion removes the customized PC object; undo and redo restore/delete the graph and scene together.
- A board containing only a free rectangle survives page switching and reloading.
- Replay of that scene-only board displays the rectangle, and forking creates another board containing the same free object without changing its source board.
- No uncaught page errors occurred.

The first runtime pass exposed a genuine drag-loss defect: Excalidraw mutated element objects shared with the application's comparison and history snapshots, so visible movement was incorrectly considered unchanged. The fix gives Excalidraw an independent clone when projecting elements/files into its API. The same drag, including undo/redo, passed after rebuilding.

Evidence: `evidence/implementation/scene-roundtrip-check.json`, `scene-pc-rename.png`, `scene-pad-rename.png`, `scene-model-and-undo.png`, and `scene-scene-only-fork.png`.

`scripts/ui/check-lifecycle.cjs` additionally passed against the production app with a transport that deliberately ignores AbortSignal and with delayed file reads. Both late success and late failure were tested for model operations, summaries, and Excalidraw imports. Old model results never appeared in the new meeting, and canceled original speech remained saved in its original meeting. A newer summary remained in progress after the older summary's success/error/finally completed, then saved its own result normally. Delayed imports never changed the new meeting or displayed an old error. Evidence: `evidence/implementation/lifecycle-check.json` and `lifecycle-isolated.png`.

B0 targeted Node checks also cover revision-aware save acknowledgement, save failure/retry, ordered per-meeting writes/deletion, late native-style model completion/rejection after cancellation, and an in-flight layout finishing after another meeting is restored. Existing audio revision and pointing tests pass alongside those checks. Native microphone finalization, actual remote model quality, and signed desktop restart remain separate platform acceptance checks.
