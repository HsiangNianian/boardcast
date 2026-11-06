# B0/B1 Linux native storage validation

Status: the scoped Linux native storage batch is complete. This report records actual Tauri/WebKit UI and filesystem work; it is separate from the Chromium IPC fixtures. Eighteen successful observations and two historical failures that were fixed and retested are retained in [machine-readable evidence](evidence/implementation/native-storage/check.json). This is not a claim that every platform/release gate has passed.

## Isolation and build identity

The disposable run is `/home/hsiangnianian/.cache/boardcast-g2-xh767p12`, identifier `games.mmstudio.boardcast.g2`. It has private data/config/cache roots, a separate D-Bus session for each app, a private Cargo target copied with reflinks, and its own Cua socket. No fault injection targets the user's meeting directory. All PID-specific signals are checked against the private binary path. UI actions use the cua-driver skill and freshly observed exact window targets.

The first native binary SHA-256 is `09e4f689322ecaeb975e7a22cc191039bc815c300ad17cb28bb603938a9b15f4`; source and fixture hashes are archived alongside the evidence. The frontend was embedded from the isolated production build on port 1431, not the shared preview on 1430.

The final binary is `8a0d1183687f15d75cf8daef27a9da72160e9eee2793cb130dac122345eb38ff`. Its [source hashes](evidence/implementation/native-storage/source-final-hashes.json) and [production Rust build log](evidence/implementation/native-storage/native-final-build.log) identify the exact frozen version. Sources were checked unchanged across the frontend/native build. Later export UI work in the shared workspace is outside this build.

After frontend fixes, a mixed-version test exposed an unavailable newly added Rust command (`voice_input_check_stop`). Copied mtimes also let Cargo reuse an obsolete package artifact. A private `cargo clean -p boardcast` forced production compilation and exposed the preview's `Mixer::pop` being restricted to `cfg(test)`. B2 fixed that production-only compile failure; the final binary was rebuilt after all relevant TS/Rust changes were frozen. The mixed-version run is not accepted as native lifecycle proof.

The visible desktop later had unreliable foreground input/window lifetime, so those uncertain inputs were excluded. Final resource, downgrade, scale and crash checks used a separate Xvfb display `:90`, private D-Bus, Openbox and Cua endpoint. Xvfb/Openbox packages were unpacked into the disposable directory, without system installation. Cua screenshots sometimes contain opaque cursor/tooltip artifacts from the uncomposited display; file hashes and the relevant UI error/content remain independently recorded.

## Verified on the first native build

| Check | Actual observation |
| --- | --- |
| v1 readonly open | No v2 record was created; original JSON and referenced/unreferenced asset hashes stayed unchanged. |
| Copy-on-write migration | An edit through the native composer created v2 revision 1, copied only the referenced PNG, retained two pages plus summary, voices, glossary, corrections and local-only metadata, and did not alter v1 bytes. |
| Real write failure | Changing the private v2 directory to mode `0500` produced `Permission denied (os error 13)`. The current new line stayed visible and New meeting was blocked. Restoring permissions allowed recovery to revision 2. This establishes EACCES recovery, not ENOSPC. |
| OS lock | An external `fcntl.flock` on the actual private `.lock` prevented the submitted native write from changing disk. Releasing it allowed revision 3. |
| Stale window | A second app's earlier revision was refused by the frontend preflight; the winner's disk record and loser's unsaved line were preserved. This sequential case alone is not native CAS proof. |
| Simultaneous native CAS | Two real native apps submitted edits from revision 3 while the OS lock was held. On release, exactly one reached revision 4; the other received the Rust conflict message, with its unsaved line still visible. |
| Restart | A new app reopened the winning revision and displayed its retained line. |
| Future schema | Opening version 99 was refused and its original hash/current meeting remained intact. The first build hid the detailed toast behind the library modal; root's later library error fix is separate. |

The lock logs record timestamps and unchanged hashes while contention was held. Native screenshots and the final conflict message are in the evidence directory. A later orderly teardown advanced the first fixture to revision 5 without invalidating the recorded contention-time assertions. The [final fixture inventory](evidence/implementation/native-storage/final-fixture-inventory.json) states current revisions' byte hashes separately; deliberate old-client mutations happen after the initial preservation assertions.

## Failures repaired and verified on the final native build

1. A rich readonly missing-image meeting could not close. Teardown rebuilt model context from transcript rows, adding timing/page/operation properties to `history`. Comparing those properties as meeting content caused an unnecessary save. `meetingContent` now compares only utterance fields in context; transcript rows still preserve and compare their actual page and telemetry. A regression covers unchanged cleanup, changed speech and changed page ownership.
2. Restoring the exact missing PNG and clicking Retry still failed because `snapshot.missingAssets` was cached from load. Save now validates current resources instead of treating that old diagnostic as permanent; load/stash clear stale diagnostic state. Tests cover retrying the same loaded meeting after restoration and restoring a file deleted after load from retained in-memory bytes.
3. Nonempty damaged images and valid images under a wrong content-hash filename were accepted by the resource path. B1 added independent hash/decode validation, preserved unavailable references, rejected conflicting asset writes and rechecked restored resources on retry. The actual native fixtures below verify those paths.

| Final-build native check | Result |
| --- | --- |
| Rich readonly missing-image close | Native Ctrl+W closed the window; the original JSON SHA-256 stayed exact. |
| Missing-image edit and recovery | New board created a retained second page, save failed and disk stayed unchanged. Restoring the original PNG and clicking Retry in the same loaded meeting saved revision 2 with both pages and no persisted diagnostic flags. |
| Normal PNG control | The expected cyan PNG rendered normally. |
| Bad bytes under the old hash | The UI reported a checksum mismatch. New board could not save, and both original JSON and damaged bytes stayed intact. After preserving the bad bytes separately and restoring the correct PNG, Retry saved both pages in revision 2. |
| Valid PNG under the wrong hash | The incorrect magenta image was not displayed; a checksum diagnostic appeared. Readonly navigation remained available and JSON/image bytes were unchanged. |
| Invalid PNG with a matching hash | Native WebKit reported that the image could not be decoded, retained its reference and allowed readonly navigation. This separates decoding from hash validation. |
| Legacy changed after v2 migration | The changed legacy record appeared as an older-version copy. Opening and editing it produced a new ID with three pages and the older client's new transcript; both existing v1/v2 hashes stayed unchanged. |
| Original changed before first migration | After readonly open, the source file was changed as an older client would. New board failed to save, no v2 file appeared, the newer source hash stayed intact and the unsaved third page remained visible. Native close was also blocked. |
| Future version | Version 99 was refused with an upgrade-required error visible inside the library dialog. The current meeting and original future-format hash were unchanged. |
| Large synthetic meeting | A 1,324,766-byte, 40-board/2,000-line meeting opened, accepted a new sentence and saved revision 2. All original IDs/page assignments survived. After an actual process restart it reopened with 40 boards/2,001 lines and exactly the saved file hash. |
| Crash during record write | A separate 45,017,989-byte/20,000-line fixture received a real composer submission. Inotify saw production `crash-target.json.tmp` creation; SIGKILL left a 2,097,152-byte partial temporary file. The formal record retained its exact original hash. Restart opened all 20,000 original lines in revision 1 and ignored the incomplete temp file. |

The [crash log](evidence/implementation/native-storage/record-crash.json) records the observed event, exact PID, zero bytes at signal dispatch, partial bytes after process exit and the unchanged original hash. The guard permits only the explicitly selected private executable. Its assertions accept the exact old record or the complete expected new revision; JSON syntax alone is insufficient. This test deliberately uses a resource-free record and does not claim an asset-preparation crash.

Supporting verification at the frozen build: save tests 8/8; B1 asset tests 13/13; B1 Rust storage tests 8/8; combined Node tests 254/254; TypeScript passed. B1's production Chromium resource script passed 14/14, and the missing-assets close/diagnostic/text-rescue export script passed against the private frontend. These are labeled separately from actual native file evidence.

## Reproduction and cleanup

The retained disposable directory contains the frozen native project, private Cargo target/binary, fixture data, tool packages and raw UI captures. No source files were committed or pushed. The UI scripts do not call internal App functions.

```bash
run=/home/hsiangnianian/.cache/boardcast-g2-xh767p12
python scripts/ui/run-native-isolated-display.py "$run"
# Read headless-processes.json and list_windows for the actual current PID/window.
cua-driver list_windows '{}' --socket "$run/cua-headless.sock"
python scripts/ui/native-g2-action.py "$run" PID WINDOW snapshot --socket cua-headless.sock --name check
# After opening a fresh crash fixture and typing, but before submitting:
python scripts/ui/watch-native-record-crash.py "$run" PID crash-target --expected-line 'native crash edit.'
```

`seed-native-storage.mjs` seeds a **fresh** owned run; do not rerun it over archived evidence. The source fixture seeder and crash watcher explicitly reject an unrelated directory. Later fixtures and changes are identified by manifests/final inventories. Private processes are shut down after evidence capture; [cleanup verification](evidence/implementation/native-storage/cleanup.json) records no remaining process using the private executable, Cua sockets or XDG data root. The cached build and fixtures are retained for repeat runs. The final source-conflict test's unsaved extra page is synthetic test data and is intentionally discarded only during test-process cleanup.

## Remaining boundaries

Later [delete-race validation](delete-race-validation.md) fixes resurrection by late record/image writes and adds real IndexedDB, production-browser recovery and Linux filesystem tests with independent processes. Subsequent [native deletion validation](native-delete-validation.md) exercises the Tauri window/IPC path: three record formats, two native processes actually queued on the same file lock, a fresh-process library and a real HTML recovery file. The first save dialog later remained mapped; a separate direct-desktop save succeeded, but the earlier cause remains unresolved.

This batch covered EACCES and record-write SIGKILL. The later [native space/crash continuation](native-space-crash-validation.md) adds actual attachment/JSON ENOSPC on a private 32 MiB filesystem and a kill with a 24 MiB attachment temporary file still open before rename, followed by native recovery and cold starts. It does not cover every quota/filesystem variant or sudden power loss. The later native save/delete test observes save completing before deletion after lock release, not every possible wake-up order. Atomic rename without file/directory fsync does not establish power-loss durability. The 45 MB fixture took appreciable time to load; it is evidence for integrity, not a latency/performance acceptance result. Synthetic large-file checks do not establish a real 60-minute audio/thermal/background workflow. macOS, signed installation, the full native export/save/open matrix and the residual-dialog investigation remain separate gates. A truncated JSON fixture stayed byte-for-byte intact while valid meetings remained accessible; it is omitted by the native list, so this batch does not claim a corrupt-record recovery UI.
