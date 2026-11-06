# B3 sentence recovery and read-only exit — 2026-11-03

Implemented per-line recovery beside failed transcript entries. A retry uses the same utterance ID, revision, original page, speaker and pointing targets, updates that existing entry, and runs at most once while queued/in flight. A successfully completed revision cannot be retried through this entry point. A further failure leaves the action available again.

An unconfigured model now settles the line as recoverable immediately, without requesting an empty URL or keeping the line pending. The original speech remains saved, with actions to configure the board model and retry. Failed quick drafts are removed. Cancellation and switching meetings continue to fence late completion/error/finally callbacks.

`scripts/ui/check-line-retry.cjs` passed against the rebuilt production app at `http://127.0.0.1:1430`. The actual UI recorded a line without a model, configured a model, encountered two controlled failures, rejected rapid duplicate clicks, then recovered the same line on its original board while another board remained displayed. It verified one transcript entry, one generated object, and no retry action after success. Evidence: `evidence/implementation/line-retry-check.json`.

The save coordinator also remembers acknowledged content, excluding derived layout/view state and generated drawing caches. An unchanged meeting no longer needs rewriting merely to close; an in-flight older write prevents this shortcut. Free-canvas changes are committed before this comparison and remain detectable.

`scripts/ui/check-missing-assets-close.cjs` passed using the actual App and a controlled native exit bridge. A two-page meeting with a missing original underlay could be browsed and closed with zero writes. After typing new content, close was refused, the original remained byte-for-byte unchanged, and the new unsaved content stayed visible. Evidence: `evidence/implementation/missing-assets-close-check.json` and `missing-assets-edit-retained.png`.

New unit tests cover same-ID retries, repeated failure/recovery, rapid deduplication, another page, unconfigured-model no-fetch behavior, canceling an in-flight retry, unchanged missing-assets snapshots, free-canvas gesture changes, and the older-write/undo race. These browser checks simulate transport/storage/native events; they do not claim a physical microphone or operating-system shutdown test.
