# Attachment integrity and recovery

2026-11-03. The previous loader accepted any nonempty attachment and cached its filename as the hash of those bytes. A damaged PNG or a different valid image under an existing filename could therefore load without a diagnostic and be saved under the wrong reference. The native save command also returned success whenever the target file existed.

The shared TS path now independently computes the original v1 hash (SHA-256 of the complete data URL, using the filename's hash prefix) and then decodes the image using the browser's image decoder. An image must load with positive dimensions within five seconds. Only successful decodes of immutable data strings are cached, with a bounded cache; filenames never populate a hash cache. Page underlays, scene images/projections and historical scene files use the same checks.

Unavailable resources retain the original `asset:` reference and produce load-only `assetIssues` with `missing`, `unreadable`, `hash-mismatch` or `decode`. Other resources remain readable. Saving rechecks the current stored bytes, including after an earlier successful load; unresolved resources reject the record commit and existing files remain untouched. Restoring the original permits retry of the same retained reference and pending edits. Diagnostics are cleared on a successful reload and omitted from persisted records. The App displays these issues separately from meeting content. A future-version record cannot bypass the version guard through the virtual older-copy entry.

Native `meeting_asset_load` independently verifies the data-URL hash before returning bytes. `meeting_asset_save` validates the incoming hash, holds the meeting's filesystem lock, and compares an existing file rather than silently accepting its presence. A conflicting or damaged existing file is preserved and rejected. Native image decoding remains the frontend browser's responsibility, covering the formats it actually renders.

Validation in this change:

- 13 asset storage/integrity Node tests passed; the full Node suite passed 254 tests. Storage tests explicitly inject a decoder for synthetic payloads; they are not presented as real image decoding evidence.
- Eight Rust meeting tests passed, including the shared v1 hash vector and real temporary-file tests for idempotent saves, mismatched incoming bytes and preservation of a damaged existing file.
- `check-asset-integrity.cjs` passed 14 checks using real Chromium decoding and real IndexedDB. It covers valid PNG/SVG, another valid PNG at the wrong hash, truncated PNG at its own correct hash, corruption confined to a historical scene, original-record/byte preservation after refused saves, recovery without discarding pending edits, post-load replacement, cache isolation and future legacy-copy rejection. An intentionally stalled image decoder failed after 5000 ms. See [the machine-readable result](evidence/implementation/asset-integrity-check.json).
- TypeScript and `git diff --check` passed.

The browser check imports the production storage functions on the development origin in an isolated browser context. It does not claim native IPC/UI validation, macOS/Windows decoding parity, power-loss durability, or detection of every malformed image that a browser still successfully renders. B0's separate G2 run rebuilds the complete native app and exercises the real file/IPC/UI boundary with this frozen source.
