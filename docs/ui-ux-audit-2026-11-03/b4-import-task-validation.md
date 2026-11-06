# B4 import task acceptance — 2026-11-03

PDF, image, Excalidraw and arXiv inputs now share a cancellable task panel: reading → ready preview or retryable error → explicit destination choice. The task retains the original File/input and target meeting/page. Every commit stops and saves the current meeting before changing content; a failed save retains both the current content and the ready preview. Retrying uses the latest model configuration rather than the original render's stale URL.

The preview shows filename, fixed target, resulting board count and actual scan/reference-skip/unread results. Image previews disclose detected/read regions, the 60-region limit when reached, and the original-image fallback when recognition yields no text. All formats require explicit append/new-meeting confirmation. Multiple-file drops are rejected visibly without silently selecting one; selection cancellation does nothing, and the file input resets for same-file selection. JSON MIME works without an extension.

PDF cancellation destroys the pdf.js loading task/document; image recognition passes AbortSignal to fetch and checks cancellation between decode/read stages. Uninterruptible File reads are guarded against late success/error. The initial arXiv implementation only rejected late results; the [native cancellation continuation](native-arxiv-cancel-validation.md) now cancels the actual registered request, with real connection/body interruption and fresh-import recovery evidence. Images are sent only to a loopback model; an image error offers local-model configuration while preserving the original file for retry.

The production browser run exposed an actual autosave gap: content created inside `switchMeeting` could render while the transition flag was set, and no later dependency change restarted the save timer. The autosave effect now observes transition completion and defers marking/scheduling until then. An imported image with only an underlay now reaches Saved and reloadable IndexedDB data. The unchanged missing-assets close scenario still passes.

## Executed evidence

A separate production build was served from `/tmp/boardcast-import-dist` at `http://127.0.0.1:1431`; the shared 1430 output was not replaced. `scripts/ui/check-import-task.cjs` passed 14 checks with zero page errors:

- Multi-file drop imports none and explains why; extensionless JSON MIME prepares a preview.
- Canceled File reads ignore both late success and late error.
- Invalid Excalidraw data retries the same file; preview/cancel leave original pages unchanged.
- Explicit append preserves the original board and adds free objects.
- Injected write failure blocks new-meeting commit and retains the preview; retry saves the original unsaved transcript before creating the imported meeting.
- A real 41-page generated PDF reports 40 boards, scanned-page fallback and the remaining unread page before commit.
- Cancel during actual PDF extraction leaves no partial pages.
- Cancel aborts a vision request; a deliberately uncancellable late response is ignored.
- Vision HTTP failure retries the same PNG; no-text output previews and commits its original image underlay.
- A remote image endpoint receives no image POST; changing to a local endpoint through the retained error panel permits retrying the same file.

Evidence: [import-task-check.json](evidence/implementation/import-task-check.json), [PDF preview](evidence/implementation/import-pdf-preview.png), [completed import](evidence/implementation/import-task-complete.png).

The latest production `check-line-retry.cjs` and `check-lifecycle.cjs` also passed; the latter now cancels the import task before switching meetings through the visible UI. `check-missing-assets-close.cjs` passed after the autosave scheduling change. TypeScript and the combined engine/ASR Node suite passed. The added cancellation unit test verifies pre-aborted PDF/image inputs never begin decoding and that the vision reader forwards a signal and rejects when canceled.

## Boundaries

Transport/storage failures and model replies in these browser runs are controlled fixtures. PDF processing uses the real worker/canvas pipeline; image segmentation uses an actual generated PNG, but recognition responses are controlled, not an installed physical vision model. The later [native arXiv run](native-arxiv-validation.md) verifies a real versioned download, 13-board commit/save, HTTP 404 retry, repeated-preview cancellation and cold restore; native DNS/offline/timeout and remaining variants are still open. Native file permissions, native file chooser behavior, and a signed desktop build require their separate G2/G3 evidence. The native-storage gap audit is [b0-b1-g2-gap-audit.md](b0-b1-g2-gap-audit.md).
