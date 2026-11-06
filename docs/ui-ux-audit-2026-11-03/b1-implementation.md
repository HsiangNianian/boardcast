# B1 implementation evidence — 2026-11-03

Status: implementation, automated checks and production scene round-trip acceptance completed. No commit, push, release, or issue closure was performed by this agent.

## Data and ownership

- `Page.scene` / `Meeting.scene` hold Excalidraw elements, binary files, the generated baseline, explicit user field overrides, and an inert SVG projection for PAD. Generated node, edge, group, label, icon, and mark IDs are stable, including Excalidraw's automatically created bound labels.
- `mergeScene` performs a field merge. Explicit ownership survives semantic graph round trips, JSON serialization, subsequent model conflicts, model deletions, and new free objects. Customized objects whose model owner disappears survive as free objects.
- Explicit PAD rename/delete/move uses `applySemanticEdits` / `moveSceneOwner`, replacing older PC overrides for the same fields rather than allowing stale PC values to win.
- `sceneEdits` maps generated text/deletion/movement to graph/layout operations. `protectGraphFields` keeps later model updates consistent with owned node/edge labels and deletions. `sceneChanged` ignores Excalidraw version churn. App gesture batching and graph/layout/scene history integration are owned by B0.
- `Step.scene` / `replaySceneTo` support scene-only steps. PAD and the standalone player consume a data-image projection; raw imported SVG markup is not injected into the DOM.

## Persistence and recovery

- Meeting version is 2. Native data uses `meetings/v2/`; v1 JSON and its asset directories remain intact until an explicit delete. Native command `storageVersion` is optional so older clients keep their v1 namespace.
- Web uses the independent `boardcast-v2` IndexedDB database. This is a deliberate implementation adjustment from the draft plan's same-database new store: bumping the existing DB version would make the old client's `indexedDB.open('boardcast', 2)` fail during a rollback. Both versions keep the existing meeting/asset protocol.
- Only referenced current-page and historical scene resources are copied. Resource writes finish before the record commit. Underlays, inserted images, and scene projections are content-addressed; missing references remain present and prevent an unsafe save.
- Source v1 hashes detect edits made after downgrading. The list exposes changed legacy data as an “older version copy”; opening that item gives it a new meeting ID. Current v2 data remains separate.
- Unknown future versions reject loading/writing rather than being flattened. Scene structure, duplicate IDs, geometry, data-image references, links, and import size are validated.
- IndexedDB compare-and-put occurs inside one read/write transaction. Native compare-and-replace uses an OS file lock and storage revision check. Stale windows receive a conflict error. Same-process save/delete ordering and the deletion guard preserve B0's save lifecycle.

## Import and export

- Export captures graph/scene/layout/replay before its first await. Hand-drawn SVG/PNG are composed from the captured layout, independent of the currently mounted page DOM.
- PNG/SVG, animated SVG, PDF, and HTML include the user overlay and suppress replaced semantic ink. PDF accepts a page containing only free content. HTML/PDF rebuild each captured page's scene and historical projection before output.
- PC visual exports include the page underlay as a locked image. Boardcast re-import restores it once as `Page.underlay`, while unrelated Excalidraw clients receive a visible image. Markdown explicitly identifies omitted free visuals. The final HTML player frame uses the authoritative page graph even if an old step stream is incomplete.
- Excalidraw exports include the editable scene/files and Boardcast graph/layout/generated/ownership/underlay metadata. Import restores that metadata directly. External imports retain unlabelled shapes, strokes, images and files, remap semantic source IDs, and suppress duplicate generated visuals on the first layout. The importer now returns `scene`, optional `layout`, and optional `underlay` to App.

## Executed checks

- `node_modules/.bin/tsc --noEmit`: passed after B1 integration.
- `node --test 'src/engine/*.test.ts' 'eval/asr/*.test.mjs'`: **240 passed, 0 failed** in the latest combined implementation run, including the summary fingerprint and content-list tests. The earlier B1-only run of 230 tests is recorded in `/tmp/boardcast-b1-tests.log`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib meetings::tests -- --nocapture`: **5 passed, 0 failed**.
- Added coverage includes repeated model conflicts after graph synchronization; version-only callbacks; user deletion/model deletion; scene-only replay; JSON round trips; imported active links/external resources; resource copying and deduplication across page/history; missing asset recovery; stale storage revision; free shape/image import; and editable metadata restoration.
- The native suite validates v1 JSON/asset preservation, namespace selection, future-format protection, safe IDs/assets, and stale revision rejection. It is not a live Tauri IPC or macOS acceptance run.
- [scene-roundtrip-check.json](evidence/implementation/scene-roundtrip-check.json) records real PC rename/drag/undo, PAD semantic changes, model candidate adoption, free-only page save/reopen/replay/fork. [B4 export acceptance](b4-export-acceptance.md) records 67 production content checks, including offline HTML and image preservation.

## Verification boundaries

The production evidence covers PC/PAD semantic movement after an existing position override, offline HTML, and fixture font/image rendering in each visual format. It does not exhaust arbitrary imported frames or all Excalidraw element variants. Markdown states its visual omissions. Unknown future versions are protected by refusal, not a generic schema-aware viewer. These checks do not substitute for live Tauri IPC or macOS testing.


## Dark-board projection check

`board/overlayTheme.ts` changes only visible vector primitives using the Excalidraw theme transform. Embedded PNG, SVG, `<use>` image references, and clip/definition content remain unchanged. `InkBoard` memoizes the resulting image by source URL/theme; exports retain the original light projection.

The installed Excalidraw implementation applies a root filter and compensates raster image uses, while SVG images stay filtered (`dist/dev/chunk-4FTI6OG3.js`, export/render image code). Applying its root filter to the whole Boardcast projection would therefore change imported pictures. The implemented primitive-level transform avoids that behavior and preserves z-order.

The Chromium canvas test is archived at `scripts/ui/check-overlay.cjs`, callable through `scripts/ui/run-checks.cjs` with the existing `PLAYWRIGHT_PATH` and optional `CHROMIUM_PATH` environment variables:

```sh
node scripts/ui/run-checks.cjs http://127.0.0.1:1423 check-overlay.cjs
```

The test executes the actual source function inside Chromium, rasterizes the light and dark images, and asserts exact image pixel equality. Result on 2026-11-03:

| Content | Light RGBA | Dark RGBA |
| --- | --- | --- |
| Black vector shape | 0, 0, 0, 255 | 237, 237, 237, 255 |
| Embedded red PNG | 255, 0, 0, 255 | 255, 0, 0, 255 |
| Embedded green SVG | 0, 180, 70, 255 | 0, 180, 70, 255 |

Only the vector shape receives a theme filter. The test passed; this verifies both dark-board contrast and preservation of raster/vector image colors.


## Explicit model-candidate adoption

The former `protectGraphFields` call replaced a conflicting model label before it could be recovered. Clearing a scene override alone could not recover that proposal. `FreeScene.modelCandidates` now keeps the latest actually rejected label (including an explicitly empty edge label), or the proposed entity rejected by a user deletion. It is captured before protection for completed model results; unrelated subsequent turns do not fabricate or erase a candidate.

- `captureModelCandidates(rawGraph, scene)` records actual conflicts before `protectGraphFields`.
- `sceneOwnerAction(scene, selection)` distinguishes a real model proposal from a visual reset, or returns no action.
- `adoptSceneOwner(graph, scene, selection)` returns a new graph/scene for one selected owner. Model adoption clears that owner's overrides; a style-only reset preserves user-owned text/deletion semantics and unrelated free objects.
- The App records graph/layout/scene as one undo batch. Candidate records survive JSON, Page/Meeting/Step persistence and Excalidraw metadata, and are validated at load/import boundaries.

Added pure tests cover candidate loss prevention, later unrelated model turns, latest conflicting replacement, a genuine empty edge label, protected deletion, no-candidate styling reset, invalid candidate rejection, import metadata round trip and undo/redo. `scene-roundtrip-check.json` also records the production UI passes for style reset + undo, explicit adoption of the actual model proposal + undo, and subsequent deletion/redo.
