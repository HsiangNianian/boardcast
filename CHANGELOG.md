# Changelog

All notable changes to boardcast. Generated from Conventional Commits by `pnpm release`.

## 0.9.0 - 2026-10-30

### Features

- **import:** Import images and mark them up with the local vision model (closes #12) ([`12410b4`](https://github.com/HsiangNianian/boardcast/commit/12410b41364f4a4c65c468e09e188599df951feb))
  - screenshots, whiteboard photos and diagrams become a board with the image as underlay, marked up the same way as PDF pages
  - positions come from image processing (segment.ts): ink against the most common background, long straight lines removed with tolerance for hand-drawn wobble, gaps between characters bridged, connected runs merged into lines and lines into paragraphs
  - each block is cropped and read by the built-in model's vision projector; blocks that read as nothing or only symbols (arrowheads) are dropped
  - measured on an RTX 3060 with Qwen3.5-4B: the model reads text accurately but its own boxes drift by tens of pixels, which is why it only reads; a document screenshot takes about 5 s, a hand-drawn diagram up to 25 s
  - the model catalog gets a vision component per LLM (mmproj-F16, 640 MB for 4B, 880 MB for 9B) from ModelScope / hf-mirror / Hugging Face; llama-server starts with --mmproj when the selected model's component is installed, and the status reports it
  - images are only read locally; import asks for the vision component when it is missing
  - the prompt tells the model that lines in the source image are not edges and should be changed by striking or annotating the text next to them
  - tests: block finding on a synthetic page with a framed label and an arrow, reading cleanup, catalog pairing

**Full diff:** [v0.8.0...v0.9.0](https://github.com/HsiangNianian/boardcast/compare/v0.8.0...v0.9.0)

## 0.8.0 - 2026-10-30

### Features

- **import:** Import PDFs and mark them up during the meeting (refs #12) ([`f3f55d9`](https://github.com/HsiangNianian/boardcast/commit/f3f55d9140544dbb91a8fde2e8fe70669f6d07fa))
  - each PDF page becomes a board: the page is rendered as an underlay in the top-left corner, and pdf.js's text layer is merged into lines and paragraphs that become pinned "原文" regions
  - regions are graph nodes of kind region with a fixed position; the model sees them as [原文] in the entity table and can only mark, connect, annotate or attach questions to them, and applyOps restores any rename, delete or move
  - marks are drawn on the page (circle, strike, underline, star), links between regions reuse the curved arrows, and notes go in a margin to the right of the page with a thin line back to their paragraph
  - regions can be clicked and marked by hand; rename and delete are hidden for them
  - other topics and flowcharts are laid out around the page; the page size is part of the graph so replay and thumbnails keep it
  - PNG / SVG exports include the underlay, and Markdown gets a 原文标注 section
  - up to 40 pages per import; scanned pages get an underlay but no regions; password-protected PDFs say so
  - pdf.js loads on demand; a vite plugin serves its cmaps, standard fonts and wasm in dev and copies them into the build, and the desktop CSP allows wasm
  - a cloud model only receives the text of the regions, never the page image

### Build and CI

- Stop XProtect before building the macOS .dmg ([`8e286a6`](https://github.com/HsiangNianian/boardcast/commit/8e286a6ca845f10e9422fc289dda70dd6367eba7))
  - GitHub's macOS runners sometimes fail the .dmg with "hdiutil: create failed - Resource busy" because XProtect holds the new disk image; v0.7.0's release hit it
  - kill XProtect before packaging in both CI and the Release workflow

**Full diff:** [v0.7.0...v0.8.0](https://github.com/HsiangNianian/boardcast/compare/v0.7.0...v0.8.0)

## 0.7.0 - 2026-10-30

### Features

- **desktop:** Update the app in place from GitHub releases (closes #17) ([`28dae69`](https://github.com/HsiangNianian/boardcast/commit/28dae69849ca38679a61572a557d69f8d13f28ff))
  - the Tauri updater plugin checks latest.json on the latest GitHub release a few seconds after launch; a "新版本 x.y.z" button appears in the top bar when there is one
  - the update panel renders the release notes and has "下载并重启": it shows download progress, verifies the signature, then stops listening and the built-in model, installs and restarts; a failed download leaves the meeting untouched, and a failed install brings the model back
  - settings get an 更新 section: current version, a toggle for checking on launch, and a manual check
  - both commands live in Rust (update_check, update_install), so the webview needs no updater permissions
  - release builds sign update bundles with the TAURI_SIGNING_PRIVATE_KEY secret through tauri.updater.conf.json, and tauri-action uploads latest.json; regular CI builds need no key
  - the public key is in tauri.conf.json, and a Rust test checks the updater config parses, since a bad one would crash the app on launch
  - prereleases never become GitHub's latest release, so they are not offered
- **voice:** Capture system audio for online meetings (refs #10) ([`8c5b1dd`](https://github.com/HsiangNianian/boardcast/commit/8c5b1ddd52b6284b7dcca0321df1331facc99e03))
  - settings choose what to listen to: the microphone, the microphone plus system audio, or system audio only
  - Linux reads the default output's PulseAudio / PipeWire monitor source; Windows opens the default speakers as a WASAPI loopback input; other systems say it is not supported yet
  - each source is resampled to 16 kHz on its own and a Mixer sums them before sentence splitting and recognition
  - the mixer does not wait for a source that has gone quiet (Windows loopback sends nothing while nothing plays): after 0.3 s it treats the gap as silence
  - opening a stream is shared between microphone and system audio, with errors that say which one failed
  - tests: the mixer is unit-tested, and an ignored hardware test reads one second of system audio (48 kHz monitor source on a Linux laptop)
- **import:** Import .excalidraw drawings as boards (refs #12) ([`742b970`](https://github.com/HsiangNianian/boardcast/commit/742b970e4e377b5a58cc6282dc094086610b7230))
  - an import button in the top bar, and files can be dropped onto the board
  - boxes, ellipses and diamonds with text become flowchart nodes; arrows bound at both ends become edges, with the arrow's text as the label; frames become groups; loose text goes under a "白板上的文字" topic
  - two boxes with the same text stay two nodes, and connected boxes without text get a name
  - everything goes through ops and applyOps, so ids, dedupe and validation match what the model draws
  - a drawing lands on a new board when the current one has content, is logged in the transcript, can be undone, and the model can keep adding to it
  - drops are caught in the capture phase so Excalidraw's own drop handler on the pencil board does not replace the canvas; the desktop window passes file drops to the page (dragDropEnabled: false)
  - invalid files are rejected with a reason

**Full diff:** [v0.6.0...v0.7.0](https://github.com/HsiangNianian/boardcast/compare/v0.6.0...v0.7.0)

## 0.6.0 - 2026-10-30

### Features

- **privacy:** Say what goes to a cloud model and mark cloud vs local (closes #18) ([`8126f7d`](https://github.com/HsiangNianian/boardcast/commit/8126f7d14d94a71d220fdc49fa395d3f29621919))
  - before a cloud endpoint is enabled for the first time, settings list what will be sent (what is said and by whom, the board's text, the full transcript for summaries) and what is not (audio, meeting files); it is only enabled after the user confirms, once per host
  - loopback endpoints (LM Studio, local vLLM, Ollama) are not cloud and need no confirmation
  - the model status in the top bar and the composer show a 云端 / 本地 tag at all times; clicking the composer tag opens settings
  - "这场会只用本地模型" keeps a meeting on the local model for both the board and summaries, is saved with the meeting, and starts the built-in model if it was idle
  - the summary button's tooltip names the host the transcript goes to
  - README states that boardcast is fully local by default and documents the privacy behaviour
- **pages:** Several boards per meeting with thumbnails (closes #21) ([`1667b7e`](https://github.com/HsiangNianian/boardcast/commit/1667b7e66a1c7f38cbc2c7278c68105ab66e285f))
  - a strip of board thumbnails in the board's top-left corner: click one to switch, "+" starts a new board and keeps the old one
  - each board keeps its own graph, layout memory and replay steps; switching saves the current board first, clears undo and refits the view
  - the model only sees the current board; something from an earlier board mentioned again is drawn anew on this one
  - once a board holds 30 inked items, a one-time hint suggests starting a new board
  - transcript lines record their board, and the transcript shows a "第 N 张板" divider that jumps to that board
  - thumbnails are drawn from the layout (boxes, text rows, lines) rather than rendered, and are saved with the meeting
  - meetings save all boards and the current one; a meeting is still titled after its first board
  - Markdown export puts each board in its own section and splits the transcript by board
  - switching is blocked while the model is still drawing or during replay
  - the web build's status no longer says 本地 twice

**Full diff:** [v0.5.0...v0.6.0](https://github.com/HsiangNianian/boardcast/compare/v0.5.0...v0.6.0)

## 0.5.0 - 2026-10-30

### Features

- **replay:** Replay how the board grew during a meeting (closes #16) ([`6d079f1`](https://github.com/HsiangNianian/boardcast/commit/6d079f12c2805195c6c29cb283fa1af3b108b217))
  - every change to the inked board is recorded as a diff against the previous version (nodes, edges and groups added, changed or removed, plus the id counters), with a timestamp and how many transcript lines there were
  - recording diffs instead of ops keeps replay exact: pencil drafts take node ids before the model's ops, and manual edits and undo are not ops at all
  - steps are saved with the meeting; meetings saved by older versions replay as a single step
  - a replay button in the top bar starts from an empty board and writes each step in at the meeting's pace (gaps clamped to 0.9-3 s), with the transcript following along
  - the replay bar replaces the composer: play / pause (space), a draggable progress bar or arrow keys to step, 1x / 2x / 4x, and Esc to leave; export works on the moment shown
  - the current board and layout are restored on exit; nothing is autosaved or editable while replaying, and replay is unavailable while listening or while the model is still drawing

**Full diff:** [v0.4.0...v0.5.0](https://github.com/HsiangNianian/boardcast/compare/v0.4.0...v0.5.0)

## 0.4.0 - 2026-10-30

### Features

- **summary:** Render the summary as Markdown and copy it as source or plain text ([`7acf097`](https://github.com/HsiangNianian/boardcast/commit/7acf0970bbea6b88b21571d1e7ea8a516ead5e84))
  - the summary is rendered with react-markdown and remark-gfm (todos show as checkboxes); raw HTML from the model is not rendered
  - the pen button (or a double click) switches to editing the source, the check button goes back
  - "复制原文" copies the Markdown, "仅文本" copies it without markup (bullets and boxes for todos)
  - the renderer is lazy-loaded with the summary panel, so startup size is unchanged
- **voice:** Tell speakers apart by their voice (refs #10) ([`a567e44`](https://github.com/HsiangNianian/boardcast/commit/a567e441db925165f13833308a84d886700aa1b9))
  - each final sentence carries a unit-length CAM++ voiceprint (3D-Speaker, zh/en, 26 MB); sentences shorter than 0.8 s get none
  - the app clusters voiceprints online per meeting: close enough to a known voice (cosine >= 0.45) means the same person, otherwise a new one named A, B, C...
  - short sentences without a voiceprint go to the previous speaker
  - click a speaker's avatar in the transcript to rename them; renaming to an existing name merges the two voices
  - voiceprints are saved with the meeting, so speakers are still recognized after the mic is restarted or the meeting is reopened
  - a setting turns it off and falls back to the speaker tag in the composer
  - the speaker model is bundled by fetch-voice (hf-mirror, Hugging Face, then GitHub) and is optional: without it, voice input works as before
  - CI and Release cache the model, and a Rust test checks it scores the same speaker well above a different one (0.60 vs 0.06 on the 3D-Speaker test clips)

### Fixes

- **summary:** Don't show an empty summary when a thinking model runs out of tokens ([`33aec54`](https://github.com/HsiangNianian/boardcast/commit/33aec549ca8e71f12e2b9aad6269a660e3335301))
  - reasoning models such as deepseek spend max_tokens on thinking and return empty content; the summary then showed four "无" sections
  - remote models now get 4096 tokens for the summary (2048 per chunk), and an empty answer is retried once with twice as much
  - if it is still empty, say so instead of saving an empty summary (mentions thinking when finish_reason is length)
  - headings written as ## 结论, **结论**, 【待办事项】 or 结论： are recognized; output with no recognizable section is kept as is
  - remote requests time out after 180 s instead of 60 s, since long answers from thinking models can take longer
  - content: null in a reply is handled
- **summary:** Drop the model name from the summary header ([`7a4afab`](https://github.com/HsiangNianian/boardcast/commit/7a4afab27287d029811a0cca4f6ed0cbc4ae04ef))
  - the header only shows progress while generating; which endpoint is used is already in the top bar and the button tooltip

**Full diff:** [v0.3.0...v0.4.0](https://github.com/HsiangNianian/boardcast/compare/v0.3.0...v0.4.0)

## 0.3.0 - 2026-10-30

### Features

- **ink:** Draw the small icons in handwriting mode ([`c8c4259`](https://github.com/HsiangNianian/boardcast/commit/c8c42591c1ab2921371491dcdd528f64c9b59cb0))
  - icons (database, table, phone, person, server, document, cloud, chart, message) were only drawn on the pencil board
  - the same shapes are now traced as pen strokes and written in order with the rest of the board: before topic titles, after items, and inside flowchart nodes with the label shifted right
  - README roadmap updated
- **board:** Export the board as SVG ([`0367c11`](https://github.com/HsiangNianian/boardcast/commit/0367c111017e21aa9d0b6f46ee5d8ac0c2afbee4))
  - new "SVG 矢量图" entry in the export menu
  - handwriting mode saves the cropped board SVG (chalk / marker texture included, unfinished strokes completed); pencil mode uses Excalidraw's SVG export on light paper
  - PNG export now rasterizes the same SVG, so both stay identical
  - the desktop save dialog filters .svg; README updated
- **desktop:** Build for macOS (Apple Silicon) ([`13c03c9`](https://github.com/HsiangNianian/boardcast/commit/13c03c9bca662b6507e330e8bc1297a90f6f4b5e))
  - fetch-llama downloads llama.cpp's Metal build for macos-arm64 / macos-x64 and keeps only the libX.0.dylib names the binaries load
  - keep libggml-rpc on macOS: unlike Linux and Windows, llama-server and every library there link it directly
  - add Info.plist with NSMicrophoneUsageDescription, without which macOS refuses microphone access
  - CI and the Release workflow build, smoke-test and test on a macos-15 runner and upload the .dmg
  - the app is not code-signed yet; README says how to allow it on first launch
- **desktop:** Bundle the voice models in the installer ([`3c39d3b`](https://github.com/HsiangNianian/boardcast/commit/3c39d3bec3d0b00f7b9033770bc58c09a105880a))
  - users no longer have to download speech recognition separately after installing; voice input works right away
  - scripts/fetch-voice.mjs fetches SenseVoice int8, its tokens and Silero VAD (about 241 MB) into src-tauri/voice/ with sha256 checks, trying ModelScope, hf-mirror and Hugging Face like the in-app downloader, or copying from BOARDCAST_VOICE_DIR when it already has verified files
  - it runs before the Tauri CLI and in beforeBuildCommand; voice/ is shipped as a resource next to llama/
  - the app looks for the bundled models first and falls back to the downloaded ones; the model panel hides the voice entry when they are bundled
  - CI and the Release workflow restore the cached voice models before packaging, so builds copy them instead of downloading
- Save meetings automatically and reopen the last one (refs #13) ([`e62adfa`](https://github.com/HsiangNianian/boardcast/commit/e62adfa9ccb94a582d8ae66ec219d265e7425a4e))
  - each meeting (board graph, model context, transcript, remembered layout positions) is saved half a second after it changes, as versioned JSON: one file per meeting under the app data meetings/ folder on desktop (written to a temp file then renamed), IndexedDB in the web build
  - on launch the last meeting is reopened with the same layout, so you can keep talking where you left off; sentences cut off by a crash are marked as not drawn
  - the eraser button becomes "new meeting": the current one stays saved and a fresh board starts
  - meetings get a title from their first topic (or node, or sentence) until renamed
  - StableLayout can snapshot and restore its memory; meeting ids are checked so they cannot escape the meetings folder
  - fix: the Session is now created once and only its model config is updated; it used to be rebuilt on every config change, so starting the built-in model mid-meeting (remote fallback) swapped the meeting for an empty board
- **ui:** Meeting list to open, rename and delete past meetings (closes #13) ([`15c9836`](https://github.com/HsiangNianian/boardcast/commit/15c9836b5da854a6bbc61702b5e7b4feb03b4218))
  - the current meeting's name sits next to the wordmark; clicking it opens the meeting list, most recent first, with time and sentence count
  - open another meeting (the current one is saved first), rename one (a renamed meeting keeps its name instead of being auto-titled), delete with a confirmation step, or start a new meeting from the top of the list
  - deleting the meeting you are in starts a fresh one without saving it back
- **ink:** Select, rename, mark and delete on the handwritten board, with undo (refs #14) ([`0b792db`](https://github.com/HsiangNianian/boardcast/commit/0b792dbc90fe02e7b53202087fb5b79b914480d5))
  - click a flowchart node, item, topic title or edge to select it (a dashed outline); dragging still pans and clicking empty space clears the selection
  - a small toolbar above the selection: rename (node names keep the old one as an alias; edge labels can be cleared), toggle strike / circle / star, delete (a topic takes its items with it)
  - Delete removes the selection, Esc clears it, Ctrl+Z / Ctrl+Shift+Z (or Ctrl+Y) undo and redo, plus undo / redo buttons in the top bar; each model round and each manual edit is one step
  - manual edits, undo and redo are written to the transcript as edits, and the model's next sentence sees the edited board; edits do not count as sentences
  - edit.ts and hit.ts are pure modules with tests; ink pieces carry a data-key for browser tests
- **ink:** Drag flowchart nodes and topics to move them (closes #14) ([`0719033`](https://github.com/HsiangNianian/boardcast/commit/07190337e0bef46510659ca99a219461a95ec4b4))
  - press on the selected box or topic and drag; a topic carries its items along
  - the layout remembers the new spot and reroutes lines around it
  - undo/redo now restore layout positions too, so undoing a delete puts things back where they were
  - keep the floating toolbar inside the board near the edges
- **summary:** One-click meeting summary in the log panel (closes #15) ([`8a25d20`](https://github.com/HsiangNianian/boardcast/commit/8a25d2042e42d7760fe35800f9dc7ddedab187ed))
  - conclusions, decisions, todos with owners and open questions from the transcript plus the board
  - uses the active model: remote when enabled, falling back to the built-in one
  - long meetings are summarized in chunks, then merged
  - the summary is editable, saved with the meeting and put first in the Markdown export
  - Markdown export is titled with the meeting name
  - the button tooltip says when the transcript goes to a remote endpoint

### Fixes

- **ink:** Put flowchart edge labels on the line they belong to ([`56c2a65`](https://github.com/HsiangNianian/boardcast/commit/56c2a657a752d36f339230cb47eaa6f6cbc6b736))
  - labels were placed at the midpoint between the two endpoints, so on elbow or detour routes they floated away from the line and could cover a box
  - write the label along the longest segment of the route: above a horizontal segment, right of a vertical one, and on the other side when that spot covers a node
  - the placement lives in board/labels.ts with unit tests
- **engine:** Drop references to node ids that don't exist ([`d3ace36`](https://github.com/HsiangNianian/boardcast/commit/d3ace366b67adfbe10eb230f01b7cb24e22ddd33))
  - an unquoted reference that looks like an id (n12, g3) but isn't on the board used to be retried as a name, creating a node literally called "n12"
  - local models are grammar-constrained to real ids, but remote models are not and sometimes invent them
  - such references now resolve to nothing, so the op is skipped; bare words like 小程序 are still looked up by name
- **ui:** Plain startup animation that finishes writing before it fades ([`bf9f727`](https://github.com/HsiangNianian/boardcast/commit/bf9f727bcd3ec22227034e6b815c35200b2eecf2))
  - the splash timer started on mount, but the wordmark only begins writing once its glyphs load, so on a slow start it faded out after just "bo"
  - start the timer when the wordmark is ready (Wordmark gets an onReady callback), with a 3 s cap
  - drop the grainy colour clouds, which looked muddy in dark mode and were the last piece borrowed from typesafe.ai: plain theme background, the wordmark in the theme's ink, a hairline that grows while it writes
  - exit with a short fade only, no blur or scale over the half-visible app
- **desktop:** Download llama-server before tauri dev starts compiling ([`74291d2`](https://github.com/HsiangNianian/boardcast/commit/74291d2e6fe0584a51be69341b441a27de37e5d3))
  - tauri dev runs beforeDevCommand in the background and compiles Rust at the same time, so on a fresh checkout the build script failed with "resource path llama doesn't exist" while fetch-llama was still downloading
  - run fetch-llama in the "tauri" package script, before the Tauri CLI starts; beforeDevCommand is just the dev server now
  - build.rs creates an empty llama/ when it is missing, so plain cargo build / cargo test work without fetching first
- **ink:** Keep edge labels off other edges' lines ([`c719700`](https://github.com/HsiangNianian/boardcast/commit/c71970042ab3001767145286e5e697132a079e59))
  - labels only avoided nodes, so a label placed next to its own line could still be crossed by another edge (e.g. a line straight through 锁库存)
  - labels are now placed after every line is drawn, and each candidate spot (every segment, at the middle and either side, longest first) is checked against nodes, text rows, already placed labels and all edge lines
  - cross-region arc labels go through the same search, with their old outside-of-the-curve spot tried first
  - if no clean spot exists, the label gets a paper-coloured backing so it stays readable over the lines
  - tests cover avoiding another edge's line and the backing fallback
- **ui:** Hide Excalidraw's own menu, library and help on the pencil board ([`fd5afc6`](https://github.com/HsiangNianian/boardcast/commit/fd5afc6624463deb5b58b4c0ac6537b655730eaa))
  - the hamburger menu, Library button, shortcut help and canvas hint made the pencil board look like an embedded Excalidraw, and the menu duplicated (and could fight with) our own export, clear and theme controls
  - hide them; keep the drawing toolbar, zoom and undo, which editing needs
  - set Excalidraw's language to zh-CN so the remaining tooltips match the app
- **ci:** Use Windows' own tar when unpacking llama-server ([`f456726`](https://github.com/HsiangNianian/boardcast/commit/f4567263b0c10c24bb9b712f05b9e6af8db5a9e7))
  - the desktop build step ran under shell: bash, which on Windows puts Git Bash's GNU tar first on PATH; it treated D:\ as a remote host and could not unpack the llama.cpp zip
  - drop shell: bash from that step (the env var is set through env: anyway)
  - fetch-llama now calls %SystemRoot%\System32\tar.exe (bsdtar) on Windows, so it works from any shell

### Performance

- Load Excalidraw, ELK and the exporters only when they are needed ([`ceaa556`](https://github.com/HsiangNianian/boardcast/commit/ceaa556d7f6129c7b645ca0896eeeef3284033fe))
  - the startup bundle shrinks from 2.9 MB to 340 kB (111 kB gzipped)
  - the pencil board (Excalidraw, with its fonts and mermaid/katex) is a lazy component loaded when you switch to pencil mode
  - ELK is imported the first time a flowchart has to be laid out
  - scene building and the PNG / Markdown / .excalidraw exporters are imported on use

### Refactoring

- **desktop:** Pick the recommended model with rfind and keep clippy clean in CI ([`8538255`](https://github.com/HsiangNianian/boardcast/commit/8538255c144747284cf398bb43d4a670deb4ad19))
  - replace filter().last() with rfind() when choosing the largest model that fits in VRAM (clippy::double_ended_iterator_last)
  - run cargo clippy with -D warnings on the Linux desktop job so new warnings fail CI

### Dependencies

- **deps:** Drop cheerio and lodash.pick from the dev tree ([`d5b6c11`](https://github.com/HsiangNianian/boardcast/commit/d5b6c11c01718e8abd061c8c1b06afe49b23f1b1))
  - hersheytext is only used by scripts/glyphs.mjs to read its bundled font JSON, never its SVG parser
  - override hersheytext>cheerio away, which also removes lodash.pick (Dependabot alert #16, no patched version upstream)
  - the generated src/ink/hershey.json is unchanged

### Build and CI

- **release:** Don't let setup-node look for pnpm in the release-notes job ([`f3cec0a`](https://github.com/HsiangNianian/boardcast/commit/f3cec0ab811fcdd0da40a9fa788c1b4d7c2a169f))
  - setup-node v5 enables a package-manager cache from package.json's packageManager field, which failed because that job never installs pnpm
  - the job only runs node scripts/changelog.mjs, so turn the cache off there
- **release:** Only mark suffixed versions as prereleases ([`4b7ba97`](https://github.com/HsiangNianian/boardcast/commit/4b7ba97d287c066605f15e1bd40f468c4210197d))
  - 0.x releases are regular releases and become Latest
  - -rc.1 style tags stay prereleases

**Full diff:** [v0.2.0...v0.3.0](https://github.com/HsiangNianian/boardcast/compare/v0.2.0...v0.3.0)

## 0.2.0 - 2026-10-29

### Features

- **ink:** Stroke thickness setting for the handwritten board ([`91faf57`](https://github.com/HsiangNianian/boardcast/commit/91faf5761a20bcfd0cfd50ae800d5897f5a20f0f))
  - a "笔画粗细" slider (50%–200%) next to messiness in settings, remembered across launches
  - every pen stroke is scaled in one place, while the hand wobble keeps its original amplitude so thicker strokes don't shake more

### Fixes

- **board:** Route new flowchart edges around other nodes ([`6a77f55`](https://github.com/HsiangNianian/boardcast/commit/6a77f55a34b1f034c3d21979a41ee7b4fd0e0546))
  - the incremental router drew straight or elbow lines that could cut through boxes in between
  - try the direct route first, then detour through a channel below or above the nodes in the way
  - re-route kept edges when a newly placed node lands on them
  - the layout test now checks that no edge crosses another node
- **ink:** Keep chalk and marker strokes crisp at any zoom ([`4103dd4`](https://github.com/HsiangNianian/boardcast/commit/4103dd42004bf146141f8e3ca965048ecc6f6858))
  - the texture filters were defined in board units: the chalk grain was finer than a screen pixel and turned into per-pixel noise, and the edge jitter (2.2 units) was wider than the 2-unit strokes, so small text dissolved into a blur
  - scale the noise frequency and displacement with the current zoom so the grain stays about 2.5 px on screen and the jitter stays under half a stroke width
  - erode less of each chalk stroke and filter in sRGB so strokes keep their brightness
- **desktop:** Don't start or warm up the built-in model while a remote API is in use ([`495366b`](https://github.com/HsiangNianian/boardcast/commit/495366b44355a919d2b8b3e25ab00c5dde016ab2))
  - with a remote endpoint enabled, the shell now leaves llama-server on stand-by (new "idle" state) instead of loading it at launch
  - the front end warms the built-in model only when it is actually the one drawing
  - if the remote check fails or the remote is turned off, the built-in model starts then; when the remote connects, a running built-in model is stopped to free VRAM
  - a sentence the remote fails on starts the built-in model on demand, waits until it is ready and draws with it, so nothing is lost
  - llm_ensure marks the state as starting under the lock, so two quick calls start only one server
  - settings and the status chip show the stand-by state

### Build and CI

- **release:** Generate the changelog from Conventional Commits and publish releases with it ([`4933ff0`](https://github.com/HsiangNianian/boardcast/commit/4933ff047fe975062a5b7e36a2a544825b02c824))
  - add `pnpm release <version> [--push | --dry-run]`: bump the version in package.json, Cargo.toml, Cargo.lock and tauri.conf.json, prepend the new section to CHANGELOG.md, commit `chore(release): vX.Y.Z` and tag
  - scripts/changelog.mjs groups commits by type (Features, Fixes, Documentation, Build and CI, ...), keeps body bullets as sub-points, links each commit and the compare range
  - the Release workflow uses the tag's CHANGELOG section as the release body (or generates it when the section is missing), uploads the three platform builds to that release, and publishes it only after all builds succeed
  - tags can be rebuilt by hand from the Actions page
  - backfill CHANGELOG.md with the 0.1.0 entry and document the release flow in the README
- Fetch the sherpa-onnx prebuilt libs fresh on every build ([`842bf4f`](https://github.com/HsiangNianian/boardcast/commit/842bf4fc5c1bb82da8683c51b8d29cd4c72749cb))
  - rust-cache prunes target/ before saving, so a restored target/sherpa-onnx-prebuilt/<version>/lib directory can exist without the static libraries in it, and the build script then skips the download
  - stop caching that directory and delete it before building, so sherpa-onnx-sys always downloads and unpacks the libs
  - same change in the release workflow

**Full diff:** [v0.1.0...v0.2.0](https://github.com/HsiangNianian/boardcast/compare/v0.1.0...v0.2.0)

## 0.1.0 - 2026-10-29

### Features

- Text-to-board prototype with local LLM graph ops, eval harness and three-platform CI ([`1019de9`](https://github.com/HsiangNianian/boardcast/commit/1019de9a849a6e0c81e4d0eef15422fa3e12ad7e))
- Freeform boards with topics, nested items, questions, formulas, marks and icons ([`1a83da4`](https://github.com/HsiangNianian/boardcast/commit/1a83da49a780da3943094e4fbe7ac98dbc31291a))
- Handwritten board mode that writes strokes in order, whiteboard marker and blackboard chalk ([`f696ca1`](https://github.com/HsiangNianian/boardcast/commit/f696ca181c007787131e72ee0e30cc49d90d5004))
- Messiness slider for handwriting, with linked strokes and cursive lowercase ([`8d1d3cd`](https://github.com/HsiangNianian/boardcast/commit/8d1d3cdb10ef2e49b4884d3814cce0004635e6b7))
- **desktop:** Bundle llama-server and launch it on startup (refs #2) ([`b76d389`](https://github.com/HsiangNianian/boardcast/commit/b76d3895803e22a99430a7e0c0f1bb76e1bace50))
  - fetch the official Vulkan build per platform into src-tauri/llama and ship it as a resource
  - pick the discrete GPU, fall back to CPU, watch the process, kill it with the app (PDEATHSIG / Job Object)
  - the UI picks up the built-in server automatically and warms it up
  - CI smoke-tests the bundled llama-server on every platform
- **desktop:** Model downloader with resume, source fallback and sha256 checks (refs #3) ([`6c8bbd9`](https://github.com/HsiangNianian/boardcast/commit/6c8bbd9b896c718be01ef355f5ed0f3174e10ee5))
  - catalog of Qwen3.5-4B / 9B Q4_K_M with sizes and Hugging Face sha256
  - ModelScope first (direct), then hf-mirror and Hugging Face; resume across sources
  - hash while downloading, verify before use, then select and restart llama-server
  - model panel: recommended tier by VRAM, progress, pause, switch, delete
  - offline downloader tests in CI, plus an ignored real-download test
- **desktop:** Voice input with Silero VAD + SenseVoice; downloader checks disk space (refs #3) ([`8533c89`](https://github.com/HsiangNianian/boardcast/commit/8533c8961809638c1fe11d82d9ca5550625fe50d))
  - capture the mic with cpal, resample to 16 kHz, cut sentences with Silero VAD
  - live partial captions every 600 ms while speaking, final text per sentence feeds the board
  - voice models (SenseVoice int8, tokens, Silero VAD) go through the downloader as one multi-file entry
  - refuse to start a download when the disk does not have room, and say how much is missing
  - CI runs real recognition tests on all three platforms
- Settings panel, theme switch, custom OpenAI-compatible endpoint, mic selection ([`e63ca33`](https://github.com/HsiangNianian/boardcast/commit/e63ca338596ccac70cc9fac84cfa1885d9e44c0f))
  - move models, mic, handwriting messiness and demo meetings into a settings panel; the main view keeps only talking, listening and the board
  - theme button cycles system / light / dark
  - remote endpoint: base URL, API key and model name; it is tested before being enabled and preferred afterwards, and a failed sentence falls back to the local model
  - the desktop shell proxies remote requests and stores the key in remote.json (0600), never in the web view
  - send only standard fields plus best-effort no-think flags, retry without them on servers that reject unknown fields, strip <think>
  - pick a microphone; on Linux use PulseAudio so real input devices are listed instead of ALSA plugins, and fall back to the default when the chosen one is gone
  - launch the bundled llama-server with -ngl auto so a model that does not fit in VRAM is partially offloaded instead of falling back to CPU only
- **ui:** Redesign the shell, add a startup animation and force landscape on phones ([`0437354`](https://github.com/HsiangNianian/boardcast/commit/0437354d3e6c4081c1c878975f2fc869a76e688d))
  - full-bleed board with a slim top bar (wordmark, model status, board style, icon buttons) and a collapsible transcript on the right
  - floating input at the bottom center; live captions float above it, / focuses it
  - startup animation writes "boardcast" with the board's own pen over a grainy gradient; skippable and reduced-motion aware
  - neutral palette with hairlines; bundle DM Sans and JetBrains Mono locally
  - empty-board welcome; auto-fit leaves room for the input
  - fix the pencil board turning light gray in dark mode (Excalidraw inverts the canvas itself)
  - rotate the app to landscape on portrait phones and map pointer coordinates for pan and zoom
- **ui:** Slim input bar with live waveform, speaker tag and @name switching ([`a2c131a`](https://github.com/HsiangNianian/boardcast/commit/a2c131a2d68a8715037a5d570259f13c4f149732))
  - replace the card-and-tray input with a single bar: mic (with a live waveform while listening) | speaker tag + text | send
  - the speaker is a small tag in front of the input: click to cycle, or start a line with "@name " to switch (new names work too)
  - live captions become one line above the bar with a thin accent in the speaker's color
  - demo meetings move into the empty-board welcome
- **board:** Incremental layout and Markdown / PNG / .excalidraw export ([`4e01116`](https://github.com/HsiangNianian/boardcast/commit/4e011161d8e414f926e0985a02a262311170e15b))
  - StableLayout remembers where regions and flowchart nodes were placed; old content stays put as the board grows
  - new regions go to the free spot that keeps the board landscape; a region that grows only pushes later regions down
  - the flowchart is laid out by ELK once, then new nodes are placed next to the node they connect to, searching up and down for room without growing into other regions; routes whose endpoints did not move are kept
  - the view only re-fits when new content leaves the screen, on the first stroke, or after "tidy up"
  - add a "tidy up" button that forgets all positions and lays the board out again
  - export Markdown (topics as numbered lists, flowchart as Mermaid, transcript), PNG (what you see: cropped hand-drawn SVG at 2x, or Excalidraw's own export in pencil mode) and .excalidraw
  - desktop saves through the native save dialog (tauri-plugin-dialog); the web build downloads
  - a bare "@name" in the input only switches the speaker
  - tests for incremental layout stability and the Markdown export

### Fixes

- Submit on Enter in the input form; override vulnerable transitive deps from Excalidraw ([`e0aef56`](https://github.com/HsiangNianian/boardcast/commit/e0aef565d0364bcb8f6be6acc1c17bc9fd47d7c6))
- Resolve glyph script paths on Windows; end hershey.json with a newline ([`a9fc741`](https://github.com/HsiangNianian/boardcast/commit/a9fc74179a9ca46c5ca2e97ffd1bfa315a454650))
- **desktop:** Enable the windows-sys features the Job Object API needs ([`6bd9150`](https://github.com/HsiangNianian/boardcast/commit/6bd9150b0e72eb12b8f30d5a02821ed8e176a21a))
- **desktop:** Don't show a stale server address before the built-in model is ready; document voice input ([`46059b7`](https://github.com/HsiangNianian/boardcast/commit/46059b7c684afedbc1d7fc0a235e05f515f831fb))

### Documentation

- Note the 9B eval model's license caveat and link the tracking issues ([`16f7a91`](https://github.com/HsiangNianian/boardcast/commit/16f7a919e5aef10c1c063eb8131b76da2b866a69))
- Replace the eval table with licensed candidate models ([`03b8492`](https://github.com/HsiangNianian/boardcast/commit/03b84927a7930cf37e8049e1a446143aea862a5c))

### Build and CI

- Use a glob for node --test so Node 24 finds the tests ([`83bdb69`](https://github.com/HsiangNianian/boardcast/commit/83bdb69b773a2e67be5850b1256c5961fcf78168))
- Add pre-commit config for pre-commit.ci ([`102eb65`](https://github.com/HsiangNianian/boardcast/commit/102eb651e9359458c113cfd8829275f282d6f1f1))
- **release:** Install libasound2-dev for the voice input build on Linux ([`2f60ac9`](https://github.com/HsiangNianian/boardcast/commit/2f60ac948150e1fca5b7ce80eb4e77eca2221c06))
- Keep the sherpa-onnx prebuilt libs when the Rust cache is restored ([`4a0bfe4`](https://github.com/HsiangNianian/boardcast/commit/4a0bfe4d416a1381663af5f2d27be4173d9ae5e6))
  - rust-cache restored sherpa-onnx-sys's build-script output but not target/sherpa-onnx-prebuilt, so linking failed after Cargo.lock changed
  - cache that directory and clean sherpa-onnx-sys before building so its build script re-checks the libs
  - apply the same fix to the release workflow

### Maintenance

- Initial commit ([`efb8523`](https://github.com/HsiangNianian/boardcast/commit/efb8523471a623d85d3bda4884942d2ebff68fc5))
- Cargo fmt ([`9ec2bce`](https://github.com/HsiangNianian/boardcast/commit/9ec2bce02235a2299ce27ff8942059ba673adf15))
