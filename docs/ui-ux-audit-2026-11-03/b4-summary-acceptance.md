# B4 summary draft protection — 2026-11-03

## Implemented behavior

An existing summary stays visible and editable while a new draft is generated. Generation captures all semantic page graphs and non-manual transcript lines before its first asynchronous operation. If a draft existed at the start, or the user edited/cleared it while generation was pending, completion presents the new draft with explicit **Replace current summary** and **Keep current summary** actions. Only acceptance updates the saved summary. Cancellation, failure, a meeting switch and entry into replay retain the current draft and discard late completion.

`Meeting.summarySource` stores a compact content fingerprint for the accepted input snapshot. Later graph/transcript changes show a stale-summary notice, including after save/reopen. Processing latency and manual edit-log entries do not change this fingerprint. Existing meetings without a fingerprint start from their loaded content; the application cannot reconstruct whether an older summary was already stale.

Clear keeps the exact text and snapshot basis in one in-session undo slot. The user can restore it even after the empty summary has saved. A new edit or accepted replacement clears that slot. This is a recovery action, not a summary version history; closing the meeting does not preserve deleted drafts.

Copy supports Markdown source and plain text. If both the Clipboard API and the legacy copy path fail, the requested text appears in a focused, fully selected textarea for a system copy command. The temporary legacy-copy element is removed even if the browser throws. Replay leaves copying available while blocking generation, editing, deletion, candidate acceptance/discard and undo-clear at both the component and App action boundaries.

## Evidence

The production check is [check-summary.cjs](../../scripts/ui/check-summary.cjs), run using the existing Playwright/Chromium installation:

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-summary.cjs
```

The final production run passed **21 checks with zero browser errors**, using `assets/index-E2yL2hEs.js` with the new sidebar tabs and review entry. It exercised held responses, concurrent manual edits, explicit keep/replace choices, cancellation followed by a late response, an HTTP 503, frozen input followed by new meeting content, stale state after reload, denied clipboard access, exact clear/undo restoration and cross-meeting cancellation. Five replay assertions cover pending-draft actions, undo-clear, final-frame edit/delete/generate controls, cancellation of an in-flight result and retained copying. The complete run is recorded in [summary-check.json](evidence/implementation/summary-check.json); screenshots show [the retained manual draft](evidence/implementation/summary-kept-human.png) and [the selected-text copy fallback](evidence/implementation/summary-copy-fallback.png).

The fingerprint regression test changes transcript wording, speaker, page and semantic board content, while preserving identity across processing-status changes and manual log entries. The complete Node suite passed **240 tests, 0 failures**, including that test; TypeScript checking passed after the summary changes.

## Scope and limits

This implements the draft lifecycle and recovery requirements in plan §5.4 / C56–C57. Model responses and failures are controlled fixtures, so these checks make no claim about #68 factuality, real-model quality, long-context accuracy or macOS clipboard permissions. Summary input remains the existing semantic board text and transcript; arbitrary free artwork is not interpreted by this change. Pending proposals and the clear-undo slot are deliberately transient. Only accepted summary text and its source identity persist.
