# boardcast on GPUI

The native rewrite of boardcast. Its core is producing the board from speech and marking on it. Excalidraw free
editing is dropped. The Tauri app in `src-tauri/` and `src/` stays until this one covers the agreed scope.

| Crate | Owns | Must not depend on |
| --- | --- | --- |
| `boardcast-board` | Board state, operations, layout, pen strokes, layout check | UI, audio, network |
| `boardcast-audio` | Capture, VAD, speech recognition | UI, board |
| `boardcast-agent` | Prompts, the layout skill, local and remote models, request queueing | UI, audio |
| `boardcast-ui` | The GPUI window, tiled ink rendering, wiring the chain together | |
| `boardcast-tools` | `check` and `agent-eval` for CI and model evaluation | UI |

```sh
export DEEPSEEK_API_KEY=...                                 # read from the environment, never stored
cargo run --release -p boardcast-ui -- app <root> --deepseek
cargo run --release -p boardcast-ui -- app <root>           # local Qwen 4B
cargo run --release -p boardcast-ui -- app <root> --case login-flow --deepseek
cargo run --release -p boardcast-ui -- app <root> --demo bs-mixed --llm http://127.0.0.1:1
cargo run --release -p boardcast-ui -- app <root> --synthetic 5000 --zoom-test --llm http://127.0.0.1:1
cargo run --release -p boardcast-tools -- check <root> --all-demos --tidy
cargo run --release -p boardcast-tools -- agent-eval <root> out.json --deepseek --only-dev --workers 6 --rules-only
cargo test -p boardcast-board -p boardcast-agent
```

`<root>` is a boardcast checkout with `public/hanzi` generated, and models come from the Tauri app's data directory;
both are bundled in a later milestone. Regenerate the answer key and `crates/agent/prompts.json` with
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

## Layout skill

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

Real clicks on the mark toolbar (hit testing and toggling have unit tests), Mac and Windows, comparison with the
Tauri app, icons, the pointing highlight, pencil drafts, speaker identification, saving meetings, imports and
exports, and auto-tidy in a live meeting (the repair itself has tests).

## Migration order

1. **Done:** crates, board, marks by voice and by hand, zoom, layout check, local and DeepSeek models.
2. **Keep a meeting:** save and reopen in the existing format, undo, a model and key setting. A board that can be
   lost is not usable, so this comes first.
3. **Ship it:** bundle models, glyphs and llama-server; packages and updates on Linux, then Mac and Windows.
4. **Migrate the rest once the core is stable:** summary and to-dos, replay, PDF and image import, all export
   formats, the web demo and website, speaker identification, and Apple speech. Only then retire `src/` and
   `src-tauri/`.
