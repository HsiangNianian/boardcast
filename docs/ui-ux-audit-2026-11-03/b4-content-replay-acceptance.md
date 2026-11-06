# Content list replay boundary review — 2026-11-03

The independent production regression passed **11 checks, 0 browser errors**, on `assets/index-BU5B_Ab9.js`. [The report](evidence/implementation/content-replay-check.json) records the current snapshot, keyboard and history assertions. Reproduce with `scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-content-replay.cjs` and the documented `PLAYWRIGHT_PATH`.

The review found and the root agent repaired these concrete paths:

- A rename form opened before replay stayed editable when the node reappeared at the final frame. The component now closes that draft and rejects read-only submission; the semantic edit entry point also rejects replay. The initial browser probe established the editable input, but did not establish a persisted mutation; that distinction is retained here.
- The content list always received the live scene, exposing future free objects at step zero. Both the list and free-object location now read the selected historical scene.
- Sidebar arrow keys also reached the global replay handler, advancing the replay when changing tabs. Tab handlers consume their navigation keys.
- Space on the Locate button started playback and suppressed the native button action. The global handler now leaves interactive controls to their own keyboard behavior.

The final fixture verifies an unsubmitted rename is not applied, both classes of rename/delete buttons are disabled, step zero and an earlier step do not expose future objects, tabs preserve playback position, Space locates while remaining paused, and the final scene returns at the final step. After exit, the saved graph label and original two transcript lines remain unchanged, with no extra manual-history entry. Focus is awaited through the layout paint before it is assessed.

The free-object helper review found no additional definite loss path: naming preserves visible text; deletion detaches frame/arrow bindings and removes a free bound caption; the prior snapshot remains available for one history undo. `finishForReview` was checked for stop → optional drain/keep unfinished → cancellation → save ordering, failure recovery and the audio-final transition guard. This was a source review of that flow, not a real microphone finalization test.
