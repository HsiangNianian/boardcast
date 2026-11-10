# boardcast on GPUI

The native rewrite of boardcast. Its core is producing the board from speech and marking on it. Excalidraw free
editing is dropped. The earlier Tauri and TypeScript app has been removed (#150); what the Rust port was checked against
is kept as frozen fixtures.

| Crate | Owns | Must not depend on |
| --- | --- | --- |
| `boardcast-board` | Board state, operations, layout, pen strokes, layout check | UI, audio, network |
| `boardcast-audio` | Capture, VAD, speech recognition | UI, board |
| `boardcast-agent` | Prompts, the layout skill, local and remote models, request queueing | UI, audio |
| `boardcast-meetings` | Meeting files, the open meeting's state, undo, the replay record | UI, audio, network |
| `boardcast-models` | The board models too large for the installer: the catalog and downloading | UI, audio, board |
| `boardcast-export` | Writing a meeting out as Markdown, PNG, SVG, animated SVG, PDF or a replay page | UI, audio |
| `boardcast-import` | Turning PDFs and images into boards: reading a page's layout, its picture and arXiv | UI, audio |
| `boardcast-update` | Finding, verifying and installing a new version | UI, audio, board |
| `boardcast-ui` | The GPUI window, tiled ink rendering, wiring the chain together | |
| `boardcast-tools` | `check` and `agent-eval` for CI and model evaluation | UI |
| `boardcast-asr-eval` | `asr-eval`: the app's speech recognition measured on fixed recordings | UI, board |

## Install on Linux

```sh
sh packaging/linux/package.sh     # target/package/boardcast-<version>-<arch>-linux.tar.gz
tar -xzf boardcast-*-linux.tar.gz
./boardcast-*-linux/install.sh    # into ~/.local, no root needed; --uninstall removes it and keeps your data
```

The package holds the app, the speech models, `llama-server` and the handwriting data. A board model is about 0.7 to 6 GB, so
the first launch offers to download one, or to use a remote model instead. It runs from the unpacked folder too.

## Updates

An installed copy looks for a newer version shortly after starting, and Settings and `boardcast --update` do it on request.
It installs in the background and waits for you to restart, so a meeting is never interrupted, and a restart saves first.

A package must carry a valid signature from the older app's update key, must contain a newer version, and must run its
self-check from where it was unpacked. Only then is anything replaced, and a failed swap restores the previous version.
Your meetings and models live elsewhere and are not touched. iPad versions will update through the App Store instead.

Releases are published by pushing a tag `v<version>` that matches the workspace version; the **Release** workflow
builds and publishes every package. The older app's update channel never reads them.

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
cargo run --release -p boardcast-tools -- agent-eval <root> out.json --deepseek --set dev --workers 6 --rules-only
cargo run -p boardcast-tools -- fetch pdfium   # the PDF library, into target/pdfium
cargo test -p boardcast-board -p boardcast-agent -p boardcast-meetings -p boardcast-models -p boardcast-export -p boardcast-import
```

Meetings are shared with the older app's data directory, and `BOARDCAST_DATA` and `BOARDCAST_CONFIG` point elsewhere for
testing. To check a change to the file format against real data, work on a copy:

```sh
cp -r ~/.local/share/games.mmstudio.boardcast/meetings /tmp/copy/
cargo run --release -p boardcast-tools -- meetings-check /tmp/copy
```

A checkout runs with the resources in `resources/`, fetched by `boardcast-tools fetch llama` and `fetch voice`, and `assets/hanzi` from `fetch glyphs`.

`crates/board/fixtures/goldens.json` is what the TypeScript engine did with real recorded model replies, frozen when
that engine was removed: the board crate's tests hold the Rust engine to it. `crates/agent/prompts.json`, the system
prompt and fixed examples first recorded from the same engine, is now edited by hand.

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
- Free-edit content is view-only for good, as free editing is gone, and those files are not touched.


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

Speech is recognized by SenseVoice. A sentence it hears as English, with no Chinese, Japanese or Korean in it, is recognized
again by Parakeet Unified EN, which did better on English meetings (18.49% against 13.79% WER on four business calls in
`eval/asr/long-meetings-2026-11-02.md`). Chinese with English terms stays with SenseVoice, as Parakeet has no Chinese.
A sentence with kana in it, and at least 0.3 s long, is recognized again by ReazonSpeech k2 v2, which did better on
Japanese (5.02% against 9.14% CER on short clips, and about the same on a long meeting, in
`crates/audio/examples/asr-japanese.rs`); shorter pieces made it invent words. Any of the three models may be left out of a
package: without SenseVoice, the one that is there hears everything, and on a Mac the system's recognizer can stand in
for all of them.
`crates/audio/examples/asr-compare.rs` compares the two and the routing on any recordings with known text, such as
`eval/asr/own-voice/`.

Speakers are told apart by voice with CAM++, comparing each sentence with the voices heard so far, as the older app does and
with its threshold. On the two recordings in `eval/asr/corpus.json` with four labelled speakers, run through the whole
speech chain, it found the right number of voices. Purity is how often a name holds one person, and coverage how often a
person keeps one name. These are two recordings, so the threshold was left alone; a wrong split or join is put right by
renaming or merging in the speakers panel.

```sh
cargo run --release -p boardcast-audio --example speaker-eval -- resources/voice <recording.wav> <reference.json>
```

| Recording | Voices found | Purity | Coverage |
| --- | --- | --- | --- |
| `en-ami`, -22 dB | 4 of 4 | 0.893 | 0.929 |
| `zh-alimeeting`, -39 dB, far-field with much overlap | 4 of 4 | 0.680 | 0.640 |

The smaller local models were compared with Qwen 4B on the same cases, prompt and grammar, without rules or hints. The
prompt was tuned for Qwen, which may favour it, and only Q4_K_M files were tried. Worse and better count holdout cases
against Qwen 4B. Time is the model's per turn, and memory is the whole graphics card's peak, on an RTX 3060 Laptop.

| Model | Node precision | Edge precision | Item recall | Mark accuracy | Worse / better | Time | Memory |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Qwen 4B | 0.974 | 0.886 | 0.891 | 0.852 | | 667 ms | 3049 MiB |
| MiniCPM5-2B | 0.903 | 0.741 | 0.561 | 0.789 | 15 / 4 | 300 ms | 1845 MiB |
| MiniCPM-V 4.7 1B | 0.733 | 0.333 | 0.394 | 0.378 | 25 / 1 | 317 ms | 759 MiB |
| MiniCPM5-1B | 0.438 | 0.204 | 0.076 | 0.059 | 26 / 0 | 162 ms | 961 MiB |

So Qwen stays the default, and the other three can be chosen in Settings with a note saying what they fall short on.

## Not validated

Real clicks on the mark toolbar, the undo buttons and shortcuts, the library and settings panels, and the download
button (the logic behind them has unit tests), a machine without this one's libraries, Mac and Windows, comparison with the Tauri app, icons, the pointing highlight, pencil drafts, real clicks in
the speakers panel (it was driven by a test script), a live microphone with several people, imports and exports, renaming and deleting meetings, and auto-tidy in a live meeting (the repair itself
has tests).

## Migration order

1. **Done:** crates, board, marks by voice and by hand, zoom, layout check, local and DeepSeek models.
2. **Done:** save and reopen in the existing format, undo, a model and key setting.
3. **Installable on Linux, done:** a package with its resources, model downloads, and automatic graphics selection.
   In-app updates are done too, and the `gpui-latest` prerelease has been installed through them. Mac and Windows
   are waiting until they can be tested. iPad is the main target and Mac the second.
4. **Migrate the rest once the core is stable:** summary and to-dos are done, and so are replay and Markdown, PNG, SVG,
   animated SVG, PDF and replay page export, imported pages can be marked, PDFs, arXiv papers and images can be imported, and speakers
   are told apart by voice and can be renamed or merged. Pointing at a speaker shows what they raised on the board, and a selected item leads back to its line. The window is being redesigned, with the board filling it and the controls floating over it, built from `gpui-component` and colored through its theme. The website is Rust too, and the earlier app is retired. `.excalidraw` export is still open.
