# Changelog

All notable changes to boardcast. Generated from Conventional Commits by `pnpm release`.

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
