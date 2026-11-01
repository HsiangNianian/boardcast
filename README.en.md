<div align="center">

# boardcast

**Talk, and the whiteboard draws itself.**

Local speech recognition · Live hand-drawn boards · Meeting replay and export

[Website](https://hsiangnianian.github.io/boardcast/) · [Try it online](https://hsiangnianian.github.io/boardcast/demo/) · **[Download the desktop app](https://github.com/HsiangNianian/boardcast/releases/latest)** · [Changelog](CHANGELOG.md)

[![Latest release](https://img.shields.io/github/v/release/HsiangNianian/boardcast?style=flat-square&color=c74432)](https://github.com/HsiangNianian/boardcast/releases/latest) [![CI](https://github.com/HsiangNianian/boardcast/actions/workflows/ci.yml/badge.svg)](https://github.com/HsiangNianian/boardcast/actions/workflows/ci.yml) [![MIT](https://img.shields.io/badge/license-MIT-24292f?style=flat-square)](LICENSE)

[中文](README.md) · **English**

</div>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshot-chalk-en.png">
  <img src="docs/screenshot-ink-en.png" alt="Current boardcast interface: a handwritten release checklist with a star, a rejected item and an open question beside the meeting transcript" width="1280">
</picture>
<p align="center"><sub>The current app replaying the built-in release checklist sample. Dark mode turns the whiteboard into a chalkboard.</sub></p>

boardcast turns a discussion into a growing whiteboard. Flows, notes, assignments, questions and rejected proposals are written, connected and crossed out as the conversation develops. Import a paper or screenshot to discuss and annotate the source itself.

Speech recognition and model inference run on your computer by default. Once the models are downloaded, you can work offline. You can also connect an OpenAI-compatible API for a larger model. The interface always shows whether the active model is local or in the cloud.

[Get started](#get-started) · [What you can do](#what-you-can-do) · [Models and privacy](#models-and-privacy) · [FAQ](#faq) · [Development and evaluation](#development-and-evaluation)

## Get started

### Watch a sample meeting

Open the **[web demo](https://hsiangnianian.github.io/boardcast/demo/)**, choose a sample and click “Play sample meeting.” No installation or API key is needed: these are board steps recorded in advance using a real model. You can also scroll to the bottom of the [website](https://hsiangnianian.github.io/boardcast/) to enter the same interactive demo.

To type your own content in the web app, connect a model endpoint that your browser can reach. Use the desktop app for microphone input, managed local models and system audio.

### Start a meeting on your computer

1. Download the installer for your operating system and architecture from [Releases](https://github.com/HsiangNianian/boardcast/releases/latest).
2. On first launch, download a model in the local model panel. Speech recognition models are bundled with the installer; the board model is a separate download.
3. Click “Start listening,” or type in the bottom input bar. For online meetings, choose “Microphone + system audio” in Settings → Audio input.

| System | Installer | Local inference / system audio |
|---|---|---|
| Linux x64 | `.AppImage`, `.deb` or `.rpm` | Vulkan / CPU; PulseAudio / PipeWire loopback |
| Windows x64 | `.exe` or `.msi` with `x64` in the name | Vulkan / CPU; WASAPI system audio |
| Windows ARM64 | `.exe` or `.msi` with `arm64` in the name | Vulkan / CPU; WASAPI system audio |
| macOS Apple Silicon | `aarch64.dmg` | Metal; microphone only, no system audio yet |

The macOS app is not yet Apple Developer signed or notarized. First launch may require allowing it in System Settings → Privacy & Security. In-app updates use a separate update signature check; a new stable release appears in the top bar with an option to download and restart.

## What you can do

| During the discussion | After the discussion |
|---|---|
| **Talk and draw**: flowcharts, groups, topics, nested notes, formulas, questions and action items share one board. | **Summarize and trace**: collect conclusions, decisions, owners and tasks; jump from an item to who raised it and the corresponding transcript line. |
| **Circle and refer**: circle something before saying “star this” or “connect these two,” giving the model explicit targets. | **Replay and branch**: watch the board grow, seek, change speed or fork a new board from any recorded moment. |
| **Edit together**: rename, move, mark and undo in handwriting mode, or edit freely in Excalidraw pencil mode. | **Continue a meeting**: search past meetings or start a new one from an old board; inherited content is faded while new changes use full ink. |

Handwriting mode draws Chinese characters stroke by stroke. English lowercase uses cursive by default and switches to print below 25% messiness; capitals and digits stay in print. Light mode uses whiteboard markers, dark mode uses chalk. The interface supports Chinese and English, while board labels and summaries follow the meeting language.

### Circle and refer

Click “Circle” in the top bar, draw around objects with a mouse or finger in **handwriting mode**, then speak or send a line of text:

> “Connect these two.” · “Star this paragraph.” · “Add rollback rehearsal under this topic.”

Draw multiple loops; overlapping loops merge. Undo removes the last loop, while `Esc` or Cancel clears them all. The next utterance carries up to eight circled targets, then clears the circles. Queued utterances keep their own targets even while the model is busy. Hovering and ordinary selection do not count as pointing.

### Discuss your source material

Use the import menu or drag a file onto the board:

| Source | What you can do |
|---|---|
| **PDF / papers** | Get one board per page with the original page underneath. Annotate words, sentences and paragraphs; connect regions and add margin notes. Layout analysis recognizes headings, columns, figures, tables and equations. |
| **arXiv** | Paste a link or paper ID in the desktop import menu to download the PDF. In the web app, download the PDF first, then import it. |
| **PNG / JPEG / WebP** | Use the local vision add-on to read text in a screenshot or photographed whiteboard. Download the add-on matching your active model in the model manager first. |
| **Excalidraw** | Turn text, shapes, bound arrows and groups into board content, then continue the discussion. The board is laid out again rather than keeping the original coordinates. |

PDF imports are limited to 40 retained pages. Scans without a text layer appear as page images without automatic OCR. Arrows in imported images do not become editable connections. Use handwriting mode for imported pages; pencil mode does not show their background images yet.

### Take the meeting with you

| Export | Use it for |
|---|---|
| **Markdown** | Conclusions, tasks, board content and the full transcript, with Mermaid flowcharts you can keep editing. |
| **PNG / SVG** | Boards in documents or slides, including marks and imported page backgrounds. |
| **Animated SVG** | The final board writes itself in meeting order in about 12 seconds. Pure CSS animation, embeddable in a README, with reduced-motion support. |
| **Excalidraw** | Further editing in Excalidraw. |
| **Meeting PDF** | One board per page, followed by the summary and transcript. Pages are rasterized; use Markdown for selectable text. |
| **Replay page** | A standalone HTML file containing the player, boards, transcript and assets. Open it offline in a browser without installing boardcast. |

## Models and privacy

The desktop app bundles llama.cpp's `llama-server` and manages its lifecycle. Linux and Windows use Vulkan, offloading layers to the GPU when available and keeping the rest on the CPU. macOS Apple Silicon uses Metal.

| Model available in the app | Download size | Where to start |
|---|---|---|
| **Qwen3.5-4B · Q4_K_M** | About 2.7 GB | Start here for laptops, integrated graphics or CPU-only machines. |
| **Qwen3.5-9B · Q4_K_M** | About 5.7 GB | Try a larger model; at least 6 GB of VRAM is recommended. Speed depends on available memory and CPU offloading. |

Downloads support pausing, resuming, free-space checks and SHA-256 verification, trying ModelScope, hf-mirror and Hugging Face in order. Vision add-ons are optional; speech and text boards do not require them.

- **Local by default**: audio is transcribed locally and meetings stay on your device. The desktop app needs neither an account nor an API key.
- **Cloud by choice**: configure a compatible endpoint in Settings. Before enabling a cloud address, the app asks you to confirm what is sent: utterances, speaker names, board text and records needed for summaries. Raw audio and original meeting files are not uploaded.
- **A choice for each meeting**: enable “Use only local models for this meeting.” Failed remote requests try the local model as a fallback, provided one is available.
- **Key storage**: desktop requests go through the Rust shell. The API key is stored in a local configuration file, not the OS keychain. The web app stores it in the browser and is subject to CORS and browser security restrictions.

<details>
<summary><strong>Custom models, data directories and troubleshooting</strong></summary>

Place a `.gguf` in the model directory or select one with `BOARDCAST_MODEL=/path/to/model.gguf`. Board quality still depends on the model and its chat template. For a compatible service, enter the base URL, API key and model name in Settings → Remote model; saving performs a real connectivity check.

| System | Default app data directory |
|---|---|
| Linux | `~/.local/share/games.mmstudio.boardcast/` |
| Windows | `%APPDATA%\games.mmstudio.boardcast\` |
| macOS | `~/Library/Application Support/games.mmstudio.boardcast/` |

Models live in `models/`; meeting JSON files and imported assets live in `meetings/`. Web meetings live in the current browser: clearing site data deletes them, so export first. Remote settings are stored in `remote.json` in the app configuration directory, which may differ from the data directory above.

For a bug report, preview and export a diagnostics bundle in Settings → Diagnostics, then attach it to an [issue](https://github.com/HsiangNianian/boardcast/issues). It excludes meeting content, audio and API keys.

</details>

## FAQ

<details>
<summary><strong>How do I use an existing meeting recording?</strong></summary>

There is no direct `.mp3` / `.wav` transcription import yet. On Linux or Windows, choose “System audio only” in Settings → Audio input, start listening, then play the recording in a media player. Stop listening when it ends. Capture uses the default output device and processes the recording in real time.

For an online meeting, use “Microphone + system audio.” Headphones help prevent other participants being captured twice through your speakers and microphone. macOS system audio capture is not supported yet.

</details>

<details>
<summary><strong>How are speech, speakers and terminology handled?</strong></summary>

SenseVoice transcribes Chinese, English, Japanese, Korean and Cantonese. Silero detects pauses; long utterances can end at a breath so the board need not wait for the entire sentence. Provisional captions appear while you speak. CAM++ distinguishes speakers by voice; rename or merge them in the transcript, or turn the feature off in Settings. Short or overlapping speech can still be misattributed.

For names that are often misrecognized, add a glossary entry such as `MediaCrawler = media crawler, media crowler` in Settings → Glossary. Entries can apply to all meetings or only the current one, and also correct typed input.

</details>

<details>
<summary><strong>Useful shortcuts</strong></summary>

These shortcuts mainly apply to handwriting mode and leave focused text fields and dialogs alone. Use `Cmd` instead of `Ctrl` for regular app actions on macOS.

| Key | Action |
|---|---|
| Hold `Space` | Push to talk in the desktop app |
| `/` | Focus the input bar |
| `PageUp` / `PageDown` | Previous / next board |
| `Ctrl+Shift+N` | New board |
| `Ctrl+O` | Import |
| `Ctrl+Shift+R` / `Ctrl+Shift+S` | Replay / summary |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `?` | Show all shortcuts |

The default global microphone shortcut is `Ctrl+Shift+Space`, configurable in Settings. On Wayland, bind `boardcast --toggle-mic` in your desktop environment instead.

</details>

## Development and evaluation

Use **Node.js 24+** and the repository's pinned **pnpm 11.12.0**. Desktop development also requires Rust and the [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/). Linux audio builds additionally need ALSA development libraries, such as `libasound2-dev`.

```sh
git clone https://github.com/HsiangNianian/boardcast.git
cd boardcast
pnpm install --frozen-lockfile
pnpm dev                       # http://localhost:1420; recorded samples work without a model
```

```sh
pnpm tauri dev                 # Desktop; first run downloads llama-server and speech models
pnpm site:dev                  # Website at http://localhost:1430
pnpm typecheck
pnpm test                      # Engine unit tests; no model needed
pnpm build                     # Type check, replay player and production web build
```

To use your own local model in the web app, start llama.cpp in another terminal and configure the local address in the app (it starts empty, and nothing is contacted until you fill it in):

```sh
llama-server -m /path/to/model.gguf --host 127.0.0.1 --port 8080 -c 4096 --jinja -np 1
```

<details>
<summary><strong>How it works and where to look</strong></summary>

```text
Microphone / system audio → Silero → SenseVoice ┐
                                               ├→ Entities, glossary, circled targets
Typed text ────────────────────────────────────┘               ↓
                                                   Rules + model graph operations
                                                              ↓
                                               Incremental layout → Ink / Excalidraw
                                                              ↓
                                                 Save, summarize, replay and export
```

The model emits graph operations, not images. Code maintains nodes, aliases and context; local inference uses GBNF to constrain operation syntax. Valid syntax does not guarantee correct meaning. Ordinary utterances can be batched, while circled references are processed separately. Layout tries to keep existing content still; “Rearrange” explicitly requests a full layout.

| Entry point | Contents |
|---|---|
| [`src/engine/`](src/engine/) | Sessions, prompts, operations, grammar constraints and engine tests |
| [`src/ink/`](src/ink/) / [`src/board/`](src/board/) | Strokes, circles, layout and document import |
| [`src-tauri/src/`](src-tauri/src/) | Speech, model downloads, processes, storage and updates |
| [`src/player/`](src/player/) | Standalone replay page |
| [`landing/`](landing/README.md) | Website at the Pages root, with the `/demo/` app embedded at the bottom |

</details>

<details>
<summary><strong>Evaluation, reproduction and known limits</strong></summary>

| Set | Current size | Purpose |
|---|---|---|
| [`eval/cases.json`](eval/cases.json) | 28 meetings | Development and regression, including English and a real-meeting regression |
| [`eval/holdout.json`](eval/holdout.json) | 27 meetings | Held out from prompt tuning |
| [`eval/pointing.json`](eval/pointing.json) | 18 cases | Chinese and English pointing cases with fixed starting boards |

Start a working llama-server, then run:

```sh
pnpm eval --set dev --label my-model
pnpm eval --set holdout --label my-model
pnpm eval --set pointing --repeat 3 --label my-model
# For a different server, add --base-url http://127.0.0.1:PORT
node eval/run.ts --only login-flow -v
node eval/run.ts --file path/to/corrections.cases.json
```

Reports go to `eval/reports/`. For v0.20.0, Qwen3.5-4B scored 1.000 for item F1, strike F1 and mark accuracy on the pointing set, with 18 cases repeated three times. These are synthetic, targeted cases, not a measure of accuracy on arbitrary real meetings; the timing excludes speech recognition. The general holdout still has a known failure to apply a correction. See the [pointing evaluation record](eval/pointing.md) and [changelog](CHANGELOG.md) for model, hardware, prompt version and reproduction details.

The model can omit content, connect the wrong items or misread emphasis. Recognition and segmentation also affect results; check important conclusions. After fixing something the model just drew, export correction cases or DPO / SFT JSONL in Settings → Correction samples. These samples are not uploaded automatically.

</details>

<details>
<summary><strong>CI, deployment and releases</strong></summary>

- [CI](.github/workflows/ci.yml): type checks, unit tests, web builds and desktop builds for Linux x64, Windows x64 / ARM64 and macOS ARM64.
- [Board eval](.github/workflows/eval.yml): engine or evaluation changes run the dev and pointing sets with the default 2B CPU model. These results are separate from local 4B evaluations.
- [Web demo](.github/workflows/pages.yml): builds the website and web app, then deploys them to GitHub Pages.
- [Release](.github/workflows/release.yml): a `v*` tag creates a draft, builds all four platforms, then publishes it with signed update packages and `latest.json` only if all builds succeed.

Use Conventional Commits. `pnpm release <next-version> --dry-run` previews release notes. `pnpm release <next-version>` updates the four version files, writes the changelog, commits and tags the release. Add `--push` to push the commit and tag. Replace the angle-bracket placeholder with the actual version. The release body comes from the corresponding [CHANGELOG.md](CHANGELOG.md) section.

A numeric version suffix creates a prerelease, excluded from automatic updates. Windows MSI requires a numeric suffix rather than `-rc.1`; prereleases skip RPM builds. Signing uses the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, with the public key in `src-tauri/tauri.conf.json`. Keep the private key to preserve the update path for existing installations.

</details>

## Contributing and license

Report problems or discuss ideas in [Issues](https://github.com/HsiangNianian/boardcast/issues), or send a PR. Issues and the [changelog](CHANGELOG.md) track ongoing work.

Code is licensed under [MIT](LICENSE). Chinese stroke data comes from [hanzi-writer-data](https://github.com/chanind/hanzi-writer-data), derived from Make Me a Hanzi and Arphic fonts under the Arphic Public License; its license is distributed with the generated data. Hershey glyphs come from [hersheytext](https://github.com/techninja/hersheytextjs) under MIT. The UI uses DM Sans and JetBrains Mono, and the website also includes a Noto Sans CJK subset. These fonts use SIL OFL; see the website font's [OFL.txt](landing/public/fonts/OFL.txt).
