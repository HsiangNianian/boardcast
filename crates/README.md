# boardcast on GPUI

The native rewrite of boardcast. Its core is producing the board from speech and marking on it. Excalidraw free
editing is dropped. The Tauri app in `src-tauri/` and `src/` stays until this one covers the agreed scope.

| Crate | Owns | Must not depend on |
| --- | --- | --- |
| `boardcast-board` | Board state, operations, layout, pen strokes, layout check | UI, audio, network |
| `boardcast-audio` | Capture, VAD, speech recognition | UI, board |
| `boardcast-agent` | Prompts, the layout skill, local and remote models, request queueing | UI, audio |
| `boardcast-meetings` | Meeting files, the open meeting's state, undo, the replay record | UI, audio, network |
| `boardcast-models` | The board models too large for the installer: the catalog and downloading | UI, audio, board |
| `boardcast-export` | Writing a meeting out as Markdown, PNG, SVG, animated SVG, PDF or a replay page | UI, audio |
| `boardcast-update` | Finding, verifying and installing a new version | UI, audio, board |
| `boardcast-ui` | The GPUI window, tiled ink rendering, wiring the chain together | |
| `boardcast-tools` | `check` and `agent-eval` for CI and model evaluation | UI |

## Install on Linux

```sh
sh packaging/linux/package.sh     # target/package/boardcast-<version>-<arch>-linux.tar.gz
tar -xzf boardcast-*-linux.tar.gz
./boardcast-*-linux/install.sh    # into ~/.local, no root needed; --uninstall removes it and keeps your data
```

The package holds the app, the speech models, `llama-server` and the handwriting data. A board model is about 3 to 6 GB, so
the first launch offers to download one, or to use a remote model instead. It runs from the unpacked folder too.

## Updates

An installed copy looks for a newer version shortly after starting, and Settings and `boardcast --update` do it on request.
It installs in the background and waits for you to restart, so a meeting is never interrupted, and a restart saves first.

A package must carry a valid signature from the older app's update key, must contain a newer version, and must run its
self-check from where it was unpacked. Only then is anything replaced, and a failed swap restores the previous version.
Your meetings and models live elsewhere and are not touched. iPad versions will update through the App Store instead.

Releases go to the `gpui-latest` tag, which the older app's update channel never reads. To publish one, raise the workspace
version and run the **Release GPUI app** workflow.

## Develop

```sh
export DEEPSEEK_API_KEY=...                                 # read from the environment, never stored
cargo run --release -p boardcast-ui                         # the model chosen in Settings
cargo run --release -p boardcast-ui -- --deepseek
cargo run --release -p boardcast-ui -- --root . --case login-flow --deepseek
cargo run --release -p boardcast-ui -- --root . --demo bs-mixed --llm http://127.0.0.1:1
cargo run --release -p boardcast-ui -- --synthetic 5000 --zoom-test --llm http://127.0.0.1:1
cargo run --release -p boardcast-ui -- --self-check         # what was found, and which graphics devices
cargo run --release -p boardcast-tools -- check <root> --all-demos --tidy
cargo run --release -p boardcast-tools -- export <root> meeting.json out.md out.png out.svg out.pdf
cargo run --release -p boardcast-tools -- agent-eval <root> out.json --deepseek --only-dev --workers 6 --rules-only
cargo test -p boardcast-board -p boardcast-agent -p boardcast-meetings -p boardcast-models -p boardcast-export
```

Meetings are shared with the older app's data directory, and `BOARDCAST_DATA` and `BOARDCAST_CONFIG` point elsewhere for
testing. To check a change to the file format against real data, work on a copy:

```sh
cp -r ~/.local/share/games.mmstudio.boardcast/meetings /tmp/copy/
cargo run --release -p boardcast-tools -- meetings-check /tmp/copy
node crates/meetings/fixtures/verify-with-typescript.mjs . /tmp/copy
```

A checkout runs with the older app's resources in `src-tauri/` and `public/hanzi`. Regenerate the answer key and `crates/agent/prompts.json` with
`node crates/board/fixtures/make-goldens.mjs <root> crates/board/fixtures/goldens.json`.

Measured on Linux with niri at 100 Hz, on Intel Iris Xe and an RTX 3060, each forced with `VK_DRIVER_FILES`.

## Findings

**Drawing.** GPUI's path cost follows total vertices, and merging paths does not help: 500 characters
fell to about 12-17 fps. Rasterizing finished strokes into tiles held 100 Hz at 5000 characters with 802
marks on both GPUs.

**Zoom.** A 27257-stroke board swept from fit to 4x, panned and swept to 0.2x: frame p50 10 ms, p99 16-17 ms,
worst 33-59 ms, peak memory 660-760 MB. A whole-board low-resolution base plus a viewport detail tier replaced a
single tier that reached 1.3-2 GB. Textures stretch while a background thread renders in 53-85 ms.

**Engine port.** `graph.rs` and `board_layout.rs` match the TypeScript engine exactly on 132 steps of recorded
model output: board state, prompt, grammar, regions, incremental nodes, routes and bounds.

**ELK replacement.** `sugiyama.rs` matches ELK on the 16 recorded flow boards. On 80 dense random graphs it has no
node overlaps, edges through nodes, or strangers in frames, the same as ELK, but it is visibly weaker: 549 crossings
against 267, 3881 bends against 1390, and 1.6 times the area.

**Layout check.** Incremental layout caused hard defects on 194 of 1682 steps of random growth. A full layout
cleared all of them. No recorded sample has a hard defect.

## Keeping a meeting

Files stay compatible with the older app, in both directions. Twelve real meetings went through open and save, then the
older app's own rules: migration hash, replay, node and edge text, and layout memory all held, and the v1 originals stayed
byte-identical. That check caught two bugs unit tests had not: rewritten nodes had their fields reordered, which the
older app reads as an edit, and numbers can be written differently, so the hash now covers the file's text.

- A v1 file is never written to. Its first save goes to `meetings/v2/` with the original's hash.
- Fields this version does not understand are kept, down to the field order of each node.
- Saves are atomic and locked, and refuse a newer format, a deleted meeting, or a change made elsewhere.
- A failed save keeps the meeting open, shows why, retries, and blocks leaving it. Closing is refused once.
- Opening another meeting discards anything in flight for the old one: late replies, late failures and queued lines.
- Imported pages and free-edit content are view-only until they move over, and those files are not touched.


`crates/agent/skills/board-layout/SKILL.md` is the guidance, and `RULES.md` is its short form for the prompt.
`Provider::default_rules` follows the results: on for DeepSeek, off for local Qwen 4B. Hints stay opt-in, since they
did not help either model. Each holdout set was scored once and not used for tuning.

| Model | Set | Variant | Node precision | Edge precision | Edge recall | Item precision | Better / worse cases |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DeepSeek | Holdout | Without | 0.852 | 0.819 | 0.956 | 0.977 | |
| DeepSeek | Holdout | Rules | 0.921 | 0.893 | 0.956 | 0.930 | 3 / 2 |
| DeepSeek | Dev | Without | 0.862 | 0.872 | 0.955 | 0.857 | |
| DeepSeek | Dev | Rules | 0.857 | 0.871 | 0.955 | 0.750 | 4 / 5 |
| DeepSeek | Dev | Hints | 0.864 | 0.856 | 0.941 | 0.847 | 5 / 5 |
| DeepSeek | Dev | Rules and hints | 0.829 | 0.826 | 0.949 | 0.739 | 5 / 8 |
| Qwen 4B | Holdout | Without | 0.974 | 0.886 | 0.862 | 0.909 | |
| Qwen 4B | Holdout | Rules and hints | 0.974 | 0.802 | 0.790 | 0.873 | 1 / 7 |

On DeepSeek's dev set, rules cut layout issues summed over turns from 94 to 57; on holdout they stayed about level at
61 and 63. The improvement is modest and the sets are small. Part of the item precision drop is the scorer: a line
that became a note without rules became an item with them, and only items count.

The checker's auto-repair fired on real model output once, on DeepSeek's `group-phrasing`.

## Not validated

Real clicks on the mark toolbar, the undo buttons and shortcuts, the library and settings panels, and the download
button (the logic behind them has unit tests), a machine without this one's libraries, Mac and Windows, comparison with the Tauri app, icons, the pointing highlight, pencil drafts, speaker
identification, imports and exports, renaming and deleting meetings, and auto-tidy in a live meeting (the repair itself
has tests).

## Migration order

1. **Done:** crates, board, marks by voice and by hand, zoom, layout check, local and DeepSeek models.
2. **Done:** save and reopen in the existing format, undo, a model and key setting.
3. **Installable on Linux, done:** a package with its resources, model downloads, and automatic graphics selection.
   In-app updates are done too, and the `gpui-latest` prerelease has been installed through them. Mac and Windows
   are waiting until they can be tested. iPad is the main target and Mac the second.
4. **Migrate the rest once the core is stable:** summary and to-dos are done, and so are replay and Markdown, PNG, SVG,
   animated SVG, PDF and replay page export. Next come PDF and image import, speaker identification, Apple speech,
   and the web demo and website. `.excalidraw` export is still open. Only then retire `src/` and
   `src-tauri/`.
