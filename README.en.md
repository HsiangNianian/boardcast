English | [中文](README.md)

# boardcast

**Talk, and the whiteboard draws itself.** Just discuss your meeting as usual, and hand-drawn diagrams grow on the whiteboard as you speak. Everything runs locally by default: speech recognition happens on your machine, and a small local model creates the board. No audio uploads, no cloud model calls. You can connect a larger cloud model if you prefer. Before you enable it, the app explains what will be sent, and the interface always shows whether you are using a cloud or local model.

> Talk, and the whiteboard draws itself. Local-first live meeting diagrams: speech → a small local LLM emitting graph edits → deterministic layout → hand-drawn Excalidraw board. Runs on Linux, Windows (x64 / ARM64) and macOS (Apple Silicon).

This is an early prototype. In the desktop app, you can speak into your microphone to build a board, type instead, or replay a sample meeting.

**[Try it in your browser](https://hsiangnianian.github.io/boardcast/)**: a web demo that replays sample meetings stroke by stroke (recorded earlier with the built-in model, so no model is needed); connect an OpenAI-compatible API to type your own. For real use install the desktop app: local models, voice input, and meetings saved on your computer.

Switch between two board styles in the bottom-left corner:

- **Handwriting**: looks like someone writing on a whiteboard with a marker, or with chalk on a blackboard in dark mode. Chinese characters appear stroke by stroke in their proper stroke order. The "Messiness" slider below ranges from neat to wildly scrawled, adjusting character tilt, size and baseline variation, overall slant, hand tremor, stroke offsets and rounding, and letter spacing. Above the halfway point, Chinese strokes start to connect and lowercase English letters switch to cursive. Capitals and digits stay printed for readability.
- **Pencil · Editable**: Excalidraw's hand-drawn style. Drag, edit, and add drawings directly on the board.

![Handwriting · Whiteboard](docs/screenshot-ink.png)

![Handwriting · Blackboard](docs/screenshot-chalk.png)

![Messiness at 0 / 50 / 90](docs/screenshot-messy.png)

![Pencil · Editable](docs/screenshot-board.png)

## How it works

```
Microphone / System audio ──► Streaming transcription ──► Completed utterance
                                                                 │
                         ┌───────────────────────────────────────┴─────────────┐
                         ▼                                                     ▼
             Fast path (rules, <10 ms)                          Slow path (small local model)
     Highlight existing nodes when mentioned;                  Read the entity table, previous two
     "Draw A to B" produces a pencil draft                     utterances, and new utterance;
                         │                                     emit a few graph edits (grammar-constrained)
                         │                                                     │
                         └──────────────────► Graph state ◄────────────────────┘
                              (Ink in drafts the model confirms; erase the rest)
                                                    │
                 Notes arranged in regions + ELK flowchart layout → Handwriting / Excalidraw rendering
```

Key design choices:

- **The model emits operations, not drawings.** Layout and visual style are entirely deterministic code. There are two families of operations:
  - Flowcharts: `E A -> B : 「label」` (connect), `X` (remove an edge), and `G` (group).
  - Notes: `T` (topic), `L` (item, including nested items and moving items elsewhere), `Q` (open question), `F` (formula), `K` (note), `M` (strike through, circle, underline, or star), and `I` (choose a small icon from the library).
  - Shared: `R` (rename), `A` (alias), and `D` (delete).
  Both types can coexist and connect on the same board. Rejected proposals are crossed out by default instead of erased, just like on a real whiteboard.
- **An entity table holds the memory.** References such as "it," "over there," and "that service" resolve against a node table and aliases maintained by the program. The model only needs to pick IDs from the list.
- **Grammar-constrained decoding.** Each call generates a GBNF grammar from the current board. References can only use existing node IDs, so the small model cannot invent references to nonexistent entities.
- **Prefix caching.** The system prompt and example conversations stay at the front. llama-server reuses their KV cache, leaving only about a hundred new tokens to process per utterance.
- **Handwriting is actually drawn.** This is not a handwriting font: Chinese characters use stroke-ordered skeleton paths from [hanzi-writer-data](https://github.com/chanind/hanzi-writer-data), while English letters and digits use Hershey single-line fonts: futural for print, switching lowercase letters to scripts for messy cursive. [perfect-freehand](https://github.com/steveruizok/perfect-freehand) then simulates marker pressure and stroke endings. New content is revealed stroke by stroke using the skeleton paths as masks. Each batch takes at most 2.4 seconds to write; longer batches speed up together so the board keeps pace with the discussion.
- **Batching when busy.** The model handles one request at a time. Queued utterances are combined in the next request, preventing an ever-growing backlog.

## Current evaluation

Two sets of synthetic meetings have gold annotations for nodes, edges, topics and items, strike-throughs, emphasis, questions, formulas, and groups:

- **Dev set**, `eval/cases.json`: 27 meetings, including 5 in English. Used for prompt tuning, so scores are optimistic. Three cases are regressions for known failures: useful remarks buried in small talk, emphasis that should apply to an item rather than its topic, and grouping expressed as "both belong to."
- **Holdout set**, `eval/holdout.json`: 27 meetings, including 5 in English. Excluded from tuning and checked only when changing models or preparing a release. Once scores are seen, prompts are not adjusted to fit them. Covers corrections, references, long greetings, pure small talk, sprint planning, owner swaps, rejected options, formulas, strings of questions, renaming, deletion, mixed Chinese and English, grouping, incident cause chains, retrospectives, flowcharts with action items, nested items, moving items between topics, loops, and references to something mentioned several turns earlier.

`pnpm eval --set dev|holdout|all --repeat 3` runs each meeting three times, reporting averages and variation between runs. Cases with large differences are marked unstable.

The results below are from the original 20 dev and 22 holdout meetings; they do not include the newly added English cases. Both default model tiers were tested on an RTX 3060 Laptop GPU (6 GB), using Q4_K_M and the same settings as the desktop app's built-in model (`-ngl auto --fit on -c 4096`, 2 CPU threads):

| Model (Apache 2.0) | Set | Node F1 | Edge F1 | Topic recall | Item F1 | Mark accuracy | Group accuracy | Latency p50 / p95 |
|---|---|---|---|---|---|---|---|---|
| **Qwen3.5-4B** (lightweight) | Dev | 0.86 | 0.61 | 0.96 | 0.66 | 0.85 | 0.50 | 699 / 1199 ms |
| | Holdout | 0.89 | 0.71 | 0.89 | 0.74 | 0.83 | 1.00 | 688 / 1369 ms |
| **Qwen3.5-9B** (higher-end) | Dev | 0.90 | 0.79 | 1.00 | 0.73 | 0.90 | 1.00 | 1531 / 3418 ms |
| | Holdout | 0.94 | 0.84 | 0.89 | 0.74 | 0.83 | 1.00 | 1512 / 3266 ms |

Local inference at temperature 0 is deterministic: all three runs produced identical results, with zero variation. In actual use, utterances may be merged when the model is busy, so results can differ from evaluation one utterance at a time. A few layers of the 9B model do not fit in 6 GB of VRAM and stay on the CPU, making it more than twice as slow. GPUs with at least 8 GB of VRAM are much faster.

Item F1 checks that items belong under the correct topic. Mark accuracy covers strike-throughs, emphasis (including checks that emphasis was not applied to the wrong place), questions, and formulas.

An earlier model comparison used the first 17 dev meetings, the same GPU, and the prompts in use at the time. The lightweight tier uses 4B instead of Gemma 4 E4B because it is nearly half the size with similar latency:

| Model (all Apache 2.0) | Size | Node F1 | Edge F1 | Topic recall | Item F1 | Mark accuracy | Latency p50 / p95 |
|---|---|---|---|---|---|---|---|
| Qwen3.5-2B | 1.3 GB | 0.68 | 0.33 | 1.00 | 0.44 | 0.50 | 269 / 737 ms |
| Gemma 4 E2B | 3.1 GB | 0.83 | 0.45 | 0.64 | 0.37 | 0.26 | 295 / 739 ms |
| Qwen3.5-4B | 2.7 GB | 0.88 | 0.58 | 0.90 | 0.54 | 0.64 | 382 / 653 ms |
| Gemma 4 E4B | 5.0 GB | 0.87 | 0.69 | 0.88 | 0.69 | 0.69 | 347 / 657 ms |
| Qwen3.5-9B | 5.7 GB | 0.88 | 0.77 | 1.00 | 0.73 | 0.88 | 551 / 999 ms |

Typical failures observed on the holdout set, recorded without using them to tune prompts:

- Missing the second half of an utterance, such as "save it to the database if it passes" or "put the final list in Redis; the app reads it directly when it opens," especially with 4B.
- Failing to apply a correction: the old edge remains after "don't refund directly; review it first."
- Failing to move items after an owner swap, such as "then Zhang takes recommendations and Liu takes search."
- Failing to create an action-items topic after "note this as a to-do."
- Failing to add a star after "we absolutely have to fix this."

Next: record real discussions, transcribe and annotate them by hand, and add them to the holdout set. Then use synthetic data for LoRA fine-tuning of 2B–4B models (#6), aiming to bring small-model accuracy closer to 9B.

## Desktop app and models

Desktop installers bundle llama.cpp's llama-server, using Vulkan for NVIDIA and AMD GPUs, Intel integrated graphics, and Snapdragon Adreno. It falls back to the CPU when no usable GPU is available. The server starts automatically with the app; no terminal needed:

- When a discrete GPU is available, only that GPU is used. The model is not split across integrated and discrete GPUs.
- If the full model does not fit in VRAM, for example when running 9B on a 6 GB GPU or when other applications occupy some VRAM, as many layers as possible go on the GPU and the rest stay on the CPU, instead of falling back entirely to the CPU.
- If the app crashes or is forcibly closed, llama-server exits with it rather than leaving a process holding VRAM.
- After the first load, a warm-up request runs. Vulkan compiles shaders on the first launch on each machine, which can take tens of seconds; the driver caches them afterward.

On first launch, the "Local model" panel recommends a tier based on your GPU:

| Model | Size | Best suited for |
|---|---|---|
| Qwen3.5-4B (Q4_K_M) | 2.7 GB | Thin-and-light laptops, integrated graphics, CPU-only machines |
| Qwen3.5-9B (Q4_K_M) | 5.7 GB | Discrete GPUs with at least 6 GB of VRAM |

Downloads try ModelScope first (directly, bypassing the system proxy), then hf-mirror, then Hugging Face. Downloads can be paused and resumed. Once complete, the file is verified against the SHA-256 checksum registered on Hugging Face, then the app switches to it automatically. Use "Manage models" at any time to switch or delete models.

![Local model panel](docs/screenshot-models.png)

You can also put any `.gguf` file in the model directory manually, or select one with the environment variable `BOARDCAST_MODEL=/path/to/model.gguf`:

| OS | Model directory |
|---|---|
| Linux | `~/.local/share/games.mmstudio.boardcast/models/` |
| Windows | `%APPDATA%\games.mmstudio.boardcast\models\` |

llama-server writes its log to the app's log directory: `~/.local/share/games.mmstudio.boardcast/logs/llama-server.log` on Linux.

Before downloading, the app checks free space on the drive containing the model directory. If there is not enough, it tells you how much more is needed instead of failing halfway through.

### Voice input

Speech models for recognition, utterance detection, and speaker identification (about 0.27 GB total) are bundled with the installer. Install, click "Start listening," and speak:

- The desktop shell captures microphone audio through cpal, resamples it to 16 kHz, and passes it to Silero VAD for utterance detection. A pause of about 0.45 seconds ends an utterance.
- SenseVoice transcribes each utterance, supporting Mandarin Chinese, English, Japanese, Korean, and Cantonese, with punctuation and number normalization. The result goes straight to the board. While you speak, gray provisional captions refresh every 0.6 seconds.
- A 5.6-second utterance takes only 200–300 ms to recognize, measured in CI on all three platforms.
- Choose your microphone in "Settings." On Linux, PulseAudio / PipeWire exposes the actual input devices. If the selected microphone is unplugged, the app falls back to the system default.
- Speakers are identified automatically by voice. Each utterance also produces a CAM++ voiceprint (26 MB model, Chinese and English), compared with voices already heard in the meeting. A close match uses the same speaker; otherwise a new speaker is added as A, B, C, and so on. Click an avatar in the transcript to give someone a real name; using an existing name merges the two speakers. Future matches use that name. Voiceprints are saved with the meeting, so identification survives turning the microphone off and on or resuming another day. Very short utterances such as "mm-hmm" or "yes" are attributed to the previous speaker. Turn this off in "Settings" to use the name beside the input box instead.
- For online meetings, capture system audio too: choose "Microphone + system audio" under "Settings → Audio input." Your voice comes from the microphone and other participants from speaker output. Linux captures the default output device's monitor source through PulseAudio / PipeWire; Windows uses WASAPI loopback for the default speakers. Both streams are resampled to 16 kHz, mixed, then segmented and transcribed. Capture does not stall when nothing is playing. Headphones work best, since speakers let the microphone pick up other participants a second time. You can also capture system audio alone, for example when listening in on an online meeting. macOS support is not implemented yet; it requires screen-recording permission.

### Glossary

Internal names such as project codenames, service names, and English acronyms are often misheard by speech recognition or misspelled by the model. Under "Settings → Glossary," enter one term per line, for example `MediaCrawler = media crawler, media crowler`. The left side is the correct spelling; the right side lists common misrecognitions or typos. The correct spelling alone also works.

- Spoken and typed text is corrected with the glossary before reaching the model. English matching uses whole words and ignores case; spaces and hyphens between words are optional, so variants such as `media crawler` and `Graph RAG` are corrected without changing parts of unrelated words.
- The glossary is also included in messages to the model so it uses the preferred names.
- There are two scopes: "All meetings" is stored locally, and "This meeting only" is saved with the meeting. Select a name on the board and click "Add to glossary" to add it to the current meeting's glossary.

### Remote models (OpenAI-compatible API)

If you prefer a remote or larger model, enter a base URL, API key, and model name under "Settings → Remote model." Any service compatible with `/chat/completions` works, including OpenAI, DeepSeek, Qwen, vLLM, LM Studio, and Ollama:

- "Save and enable" sends a real request to verify connectivity before enabling the model. The connection is checked again whenever the app opens.
- Once enabled, the remote model is preferred for board updates. If a request fails, that utterance automatically falls back to the local model, and the interface indicates this.
- Requests use standard OpenAI fields, plus an attempt to disable thinking mode on reasoning models. If a service rejects that parameter, the app retries without it and remembers the choice.
- In the desktop app, the shell sends requests. The key stays in `remote.json` in the app's configuration directory, with permissions 0600 on Linux, and is not passed to the web UI. Only its first and last few characters are shown. The web version must store it in the browser, and many providers block direct browser requests, so the desktop app is recommended.
- Remote APIs do not have the local model's output grammar constraints, so results depend on the model itself.

Privacy:

- Before enabling a cloud endpoint for the first time, the app lists what will be sent: utterances and speaker names, the current board's text, and the full transcript when you request a summary. It also lists what will stay local: audio recordings and meeting files. The endpoint is enabled only after confirmation, once per address. Local endpoints such as LM Studio, local vLLM, and Ollama are not treated as cloud services and need no confirmation.
- The model status in the top bar and the right side of the input bar always show "Cloud" or "Local." Click either to open settings.
- "Use only local models for this meeting" under "Settings → Remote model" is remembered per meeting. Sensitive meetings stay local while other meetings can keep using the remote model.
- Falling back temporarily from a failed remote request to the local model needs no extra privacy prompt.

### Stable layout

Things already on the board should stay in place while the meeting continues:

- Regions such as topics and open questions keep their positions. New regions find empty space, favoring a landscape layout. If a region grows into another, only the later region is pushed downward.
- Flowcharts get a full layout only at the start. Afterward, existing nodes stay put, and new nodes appear beside connected nodes: upstream to the left, downstream to the right. Occupied space is avoided by searching above and below, without intruding into other regions. Edges whose endpoints have not moved keep their routes.
- The view stays still while new content remains visible, and zooms to fit only when it goes out of view.
- If gaps or long detours accumulate, click "Tidy layout" in the top bar to rearrange the whole board.

### Import

Use the import button in the top bar, or drag files onto the board:

- **.excalidraw**: labeled rectangles, circles, and diamonds become flowchart nodes. Arrows bound at both ends become edges, with arrow text as labels. Frames become groups, and loose text goes into a "Text on the whiteboard" topic. If the current board already has content, the import creates a new board. Keep speaking to add to it. boardcast uses its own layout rather than preserving original positions.
- **PDF**: each page becomes a board with the page image underneath. The text layer is assembled into fixed source regions, one per heading or paragraph. Say "circle the inventory paragraph," "connect these two paragraphs," or "add a comment here: the timeout should be configurable." The model circles, strikes through, underlines, stars, or connects source text. Comments appear in a sidebar to the right, linked back to the source by a thin line. You can also click source text and mark it manually. Source text cannot be edited or deleted. Cloud models receive only its text, not page images. Imports are limited to 40 pages. Scans without a text layer provide a background image but no addressable source text. PNG / SVG exports include the background; Markdown adds a "Source annotations" section. Pencil mode does not display the background yet.
- **Papers**: PDF import analyzes page layout to segment papers and other structured documents more accurately. Two-column pages are read left column first, then right, without crossing columns. Section headings such as `3.`, `3.1.`, `IV.`, Chinese section numbering, Abstract, and References, together with first-line indentation, determine paragraphs. Figure and table captions such as `Figure 3.` and `Table 2:`, including Chinese equivalents, are grouped with the figures or tables themselves: axes, legends, and table cells. Regions are named "Figure 3: …" or "Table 2: …"; whether captions sit above or below is inferred from the document's conventions. Numbered equations are named "Equation (5)." Margin arXiv stamps and page numbers are removed, and pages containing only references do not create boards. When a paper is detected, its title becomes the meeting name, and a "Paper structure" topic beside the first board lists its sections. Say "the baseline in Figure 3 is unfair" or "Equation 5 is missing normalization" to refer to the right region. On desktop, paste an arXiv link or ID (`arxiv.org/abs/…`, `arXiv:2401.12345`, or `2401.12345`) into the import menu to download and import directly; downloads come only from arxiv.org. Browser cross-origin restrictions mean web users must download the PDF first. Optional GROBID parsing is still in progress (#32).
- **Images**: screenshots, whiteboard photos, or architecture diagrams in PNG / JPEG / WebP become a board with the image underneath. Circle, connect, and comment on them just like PDFs. Image processing locates text by joining characters into lines and lines into paragraphs, removing table lines, box outlines, and arrows first. The built-in model's vision component then reads each region. In local tests, the 4B vision model read text accurately but placed bounding boxes tens of pixels off, so it is used only for reading text. A screenshot takes 5–25 seconds. First download the matching vision component under "Settings → Manage models": 640 MB for 4B or 880 MB for 9B. The built-in model restarts with it after installation. Images are processed locally and never sent to the cloud. The web version uses the configured local endpoint, whose llama-server must run with `--mmproj`. Lines and arrows in the original image do not become board edges; to revise them, cross out or annotate nearby text.

### Export

Use the export button in the top bar, available when the board has content:

- **Markdown**: the meeting name becomes the title, followed by the summary if present. Multiple boards each get a section, and transcript entries are separated by board too. Topics become headings with numbered items, preserving strike-throughs, circles, and stars. Flowcharts become Mermaid diagrams, followed by the meeting transcript.
- **PNG**: exports what you see. Handwriting mode crops to the content at 2× resolution, using a whiteboard or blackboard according to the current theme. Pencil mode includes your manual edits.
- **SVG**: scalable vector graphics that stay sharp when enlarged, ready to embed in documents or edit in vector software. Handwriting mode preserves the chalk or marker texture.
- **Excalidraw**: a `.excalidraw` file you can drag into [excalidraw.com](https://excalidraw.com) to keep editing.
- **Full meeting PDF**: one board per page, including imported backgrounds, followed by the summary and transcript, in landscape A4. Pages are images; Chinese text is rendered with system fonts instead of embedding fonts in the PDF. Text cannot be selected or copied, so use Markdown when you need copyable text.
- **Replay webpage**: a single `.html` file to share with people who missed the meeting. Double-click to open it in a browser, switch between boards, replay each from an empty board, scrub the timeline, and change playback speed. The summary and transcript appear on the right, with the transcript following playback. The player, meeting data, imported page images, and all required Chinese stroke data are bundled in the file. No network connection or boardcast installation is needed; a typical meeting is a few hundred KB.

The desktop app opens the system's Save As dialog. The web version downloads the file directly.

### Automatic updates

Shortly after opening, the desktop app checks GitHub for the latest stable release. When one is available, "New version x.y.z" appears in the top bar. Open it to read release notes. "Download and restart" shows download progress, verifies the package signature (only packages signed with this project's private key are accepted), stops audio capture and the built-in model, installs, and restarts into the same meeting. If downloading fails, nothing changes and the meeting continues. Disable automatic checks or check manually under "Settings → Updates." Prereleases with suffixes such as `-1` are not offered. Automatic updates are available from v0.7.0 onward; earlier versions need one manual download.

### Interface

- The whiteboard fills the window. Speech and typing share an input bar at the bottom: the listening button on the left shows a live waveform while recording, and text input is in the middle. The small label before the input shows the current speaker; click to cycle speakers, or start a line with `@name ` to switch directly, including to a new name. Transcript avatars use matching colors. Provisional captions float above the bar. Collapse the input bar when needed and press `/` to return to it. Sample meetings are in the empty-board guide.
- The meeting transcript is on the right. Click an utterance to see the board operations it generated, or collapse the panel when it is not needed.
- "Summary" at the top of the transcript uses the active model, preferring an enabled remote model and falling back locally on failure, to turn the transcript and board into conclusions, decisions, action items with owners, and open discussion points. All boards are included, and conclusions and tasks identify the board where they were discussed. Imported pages contribute only source regions that were marked or annotated. Long meetings are summarized in chunks before being combined. Edit the summary directly; it is saved with the meeting and placed first in Markdown exports. When using a remote model, the transcript is sent to that endpoint, as the button's tooltip explains.
- "Replay" in the top bar replays the meeting from an empty board, drawing stroke by stroke at the original pace while the transcript follows each utterance. The bottom bar becomes playback controls: play / pause with Space, scrub or step with the arrow keys, select 1× / 2× / 4× speed, and press Esc to exit. Exports capture the board at the paused moment. Model edits, manual changes, and undo actions replay faithfully because each board change is recorded, rather than just the operations. Meetings saved by older versions have no step-by-step history, so their whole board appears at once.
- A meeting can have several boards. Thumbnails in the upper-left corner switch between them, and "+" adds a new board while keeping old ones. Turn the page when the topic changes or the board gets too crowded to read; a one-time hint appears after a board exceeds 30 objects. Transcript entries show which board each utterance drew on; click to jump there. Each board keeps its own layout and replay history. The model sees only the current board, so references to an earlier board are drawn again on the current one.
- Local and remote models, the microphone, and messiness are under "Settings" in the upper-right corner. The adjacent button cycles between system, light, and dark themes.
- Meetings save automatically. On desktop, `meetings/` in the app data directory holds one JSON file per meeting. Imported PDF pages and images are stored separately in `meetings/<meeting-id>/`, deduplicated by content and written only once. The web version stores meetings in the browser. Reopening the app resumes the last meeting. The current meeting name appears beside the wordmark in the top bar; click it to open, rename, or delete earlier meetings. The top-bar "+" starts a new meeting.
- Continue on an old board: click "Continue on it" in the meeting list (pick a board first if there are several) to start a new meeting on that board. Its layout, imported page and meeting glossary come along, and the new meeting is named "<old name> (cont.)". What stays as it was on the old board is drawn in faded ink; what the new meeting adds, renames (the box stays old, the new name is in fresh ink) or marks (say, a line through an old option) is in full ink, so the board, PNG / SVG / PDF exports and the replay all show at a glance what this meeting changed.
- Edit directly in handwriting mode: click a box, item, topic title, or edge to select it and reveal "Edit text / Strike through / Circle / Star / Delete." `Delete` removes it; `Ctrl+Z` / `Ctrl+Shift+Z` undo and redo both model and manual changes. Drag a selected box or topic elsewhere; its items move with it and edges reroute automatically. Dragging is undoable too. Manual changes appear in the transcript, and the model sees the edited board on the next utterance.
- Imported source text can be marked at word, sentence, or paragraph level. Say "circle the word predicting," "underline this sentence," or "star this paragraph." To do it manually, click the source text and choose "Word / Sentence / Paragraph" from the popup.
- Shortcuts: hold Space to talk, `PageUp` / `PageDown` to switch boards, `Ctrl+Shift+N` for a new board, `Ctrl+O` to import, `Ctrl+Shift+R` for replay, `Ctrl+Shift+S` for a summary, and `?` for the full list. `Ctrl+Shift+Space` toggles listening while the app is in the background and can be changed in settings. Wayland prevents apps from listening for global keys; bind `boardcast --toggle-mic` to a system shortcut instead.
- At startup, the same pen used on the board writes out "boardcast." Click or press any key to skip. If the system requests reduced motion, it simply fades out.
- Phones use landscape mode. Browsers that support orientation locking lock to landscape; others, such as iOS Safari, rotate the entire interface 90 degrees when the phone is held vertically. Pencil editing has inaccurate pointer positions in the rotated interface, so physically turn the phone sideways to edit.

## Run locally

Requires Node 24+, pnpm, and Rust for desktop builds.

```sh
pnpm install

# Desktop: automatically downloads the platform's llama-server to src-tauri/llama/
pnpm tauri dev

# Web only: start your own llama.cpp server (any GGUF; add -ngl 99 if you have a GPU)
llama-server -m path/to/model.gguf --port 8080 -c 4096 --jinja -np 1
pnpm dev            # http://localhost:1420
```

Run evaluation:

```sh
pnpm eval -- --label my-model          # Write reports to eval/reports/
node eval/run.ts --only login-flow -v  # Inspect operations for one meeting, utterance by utterance
pnpm test                              # Engine unit tests; no model needed
```

## Continuous integration (CI)

- `CI`: type checks, unit tests, and a web build, followed by desktop installers for Linux x64, Windows x64, Windows ARM64, and macOS (Apple Silicon).
- `Board eval`: changes to `src/engine` or `eval` trigger evaluation with llama.cpp on a CPU machine. Scores appear in the job summary to help spot regressions.
- `Release`: pushing a `v*` tag uses that version's section in CHANGELOG.md as the release body, builds and uploads installers for all three platforms, and publishes the release only when everything succeeds.

## Releases

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/zh-hans/), such as `feat(scope): …`, `fix: …`, and `docs: …`. Release notes are generated from them:

```sh
pnpm release 0.2.0 --dry-run   # Preview the release notes without changing files
pnpm release 0.2.0             # Bump versions, prepend CHANGELOG.md, commit, and tag v0.2.0
pnpm release 0.2.0 --push      # Also push the commit and tag, triggering the Release workflow
```

- Versions are updated together in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
- The changelog groups entries by type: Features, Fixes, Documentation, Build and CI, and so on. Bullet points beginning with `- ` in commit bodies are preserved as subitems. Each entry links to its commit, with a comparison against the previous release at the end.
- Without `--push`, you can edit the wording in CHANGELOG.md before pushing. The release body uses exactly the section that was pushed.
- Manually pushed tags work too. If CHANGELOG.md has no section for that version, the release body is generated from commits since the previous tag.
- Versions with a suffix such as `0.19.0-1` are marked as prereleases. All others, including 0.x, are stable releases. The suffix must be a number: the Windows MSI rejects `-rc.1`; prereleases skip the RPM (RPM versions can't contain `-`). If a platform build fails, the release stays a draft; fix the problem and rerun Actions manually for that tag.
- Automatic updates: the Release workflow signs update packages using the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and its password, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. tauri-action collects update metadata in `latest.json` and uploads it to the release. The app's public key is in `plugins.updater` in `tauri.conf.json`. Update packages are built only for releases using `src-tauri/tauri.updater.conf.json`; ordinary CI does not need the private key. Losing that key prevents installed versions from updating automatically, so users would need to download a new version manually.

## Roadmap

- [x] Text → graph operations → hand-drawn board, with evaluation and latency reporting
- [x] Notes: topics, nested items, questions, formulas, notes, strike-throughs / circles / stars, and small icons
- [x] Handwriting mode: animation in stroke order, whiteboard marker / blackboard chalk, adjustable messiness
- [x] Small icons in handwriting mode
- [x] SVG export
- [x] Voice input: microphone → Silero utterance detection → SenseVoice transcription with punctuation, plus provisional captions while speaking
- [x] Speaker identification by voice: CAM++ voiceprints, online clustering, editable names
- [x] System audio for other participants in online meetings: Linux and Windows; macOS not implemented yet
- [x] Tauri shell starts the bundled llama-server (#2)
- [x] Model download on first launch (#3)
- [x] Settings, light and dark themes, custom OpenAI-compatible endpoints with local fallback, and microphone selection
- [ ] A test set excluded from tuning; synthetic data and LoRA fine-tuning for small models
- [x] Incremental layout that keeps existing nodes still as new ones appear
- [x] Markdown / PNG / `.excalidraw` export
- [x] macOS builds for Apple Silicon with Metal llama-server; installers are not signed yet, so allow the app under "System Settings → Privacy & Security" on first launch

## License

The code is MIT licensed.

Chinese stroke data used in handwriting mode comes from hanzi-writer-data, derived from Make Me a Hanzi and Arphic fonts, under the Arphic Public License. The license text is distributed with the data in `public/hanzi/ARPHICPL.TXT`. Hershey font data comes from hersheytext (MIT).
