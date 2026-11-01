<div align="center">

# boardcast

**说话，板书自己长出来。**

本地语音识别 · 实时手绘板书 · 会议回放与导出

[官网](https://hsiangnianian.github.io/boardcast/) · [在线体验](https://hsiangnianian.github.io/boardcast/demo/) · **[下载桌面版](https://github.com/HsiangNianian/boardcast/releases/latest)** · [更新日志](CHANGELOG.md)

[![最新版本](https://img.shields.io/github/v/release/HsiangNianian/boardcast?style=flat-square&color=c74432)](https://github.com/HsiangNianian/boardcast/releases/latest) [![Eval](https://github.com/HsiangNianian/boardcast/actions/workflows/eval.yml/badge.svg)](https://github.com/HsiangNianian/boardcast/actions/workflows/eval.yml) [![MIT](https://img.shields.io/badge/license-MIT-24292f?style=flat-square)](LICENSE)

[English](README.md) · **中文**

</div>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshot-chalk.png">
  <img src="docs/screenshot-ink.png" alt="boardcast 当前界面：文件上传流程、待办事项和星标，与右侧会议记录并排显示" width="1280">
</picture>
<p align="center"><sub>当前版本的真实界面，回放内置的「流程图和待办混在一起」示例；深色主题下切换为粉笔黑板。</sub></p>

boardcast 把讨论变成一张持续生长的白板：流程、要点、分工、问题和临时否掉的方案，都能随对话写上去、连起来、划掉。你也可以导入论文或截图，一边讨论，一边在原文上圈画。

默认在本机完成语音识别和模型推理。模型下载好后可以离线开会；需要更大的模型时，也能自行连接 OpenAI 兼容接口。在设置中查看和调整模型配置。

[开始使用](#开始使用) · [能做什么](#能做什么) · [实现原理](#实现原理) · [模型与隐私](#模型与隐私) · [常见问题](#常见问题) · [开发与评测](#开发与评测)

## 开始使用

### 先看一场示例

打开 **[在线演示](https://hsiangnianian.github.io/boardcast/demo/)**，选一场示例会议，点「播放示例会议」。不需要安装、不需要 API Key；播放的是事先用真实模型录好的板书步骤。也可以从[官网](https://hsiangnianian.github.io/boardcast/)一路往下滚，直接进入同一个演示。

想在网页里输入自己的内容，需要连接可从浏览器访问的模型接口。麦克风、本地模型管理和系统收音请使用桌面版。

### 在自己的电脑上开会

1. 从 [Releases](https://github.com/HsiangNianian/boardcast/releases/latest) 下载对应系统和架构的安装包。
2. 首次打开，在「本地模型」面板下载一档模型。语音识别模型已随安装包提供，板书模型需另行下载。
3. 点「开始收音」，或直接在底部输入框打字。线上会议在「设置 → 收音」选择「麦克风 + 系统声音」。

| 系统 | 下载的安装包 | 本地推理 / 系统声音 |
|---|---|---|
| Linux x64 | `.AppImage`、`.deb` 或 `.rpm` | Vulkan / CPU；支持 PulseAudio / PipeWire 回采 |
| Windows x64 | 文件名含 `x64` 的 `.exe` 或 `.msi` | Vulkan / CPU；支持 WASAPI 系统声音 |
| Windows ARM64 | 文件名含 `arm64` 的 `.exe` 或 `.msi` | Vulkan / CPU；支持 WASAPI 系统声音 |
| macOS Apple Silicon | `aarch64.dmg` | Metal；目前只支持麦克风，不支持系统声音 |

macOS 安装包目前未做 Apple 开发者签名与公证，首次打开可能需要在「系统设置 → 隐私与安全性」里允许。应用内更新使用独立的更新签名校验；有正式新版时，顶栏会提示下载并重启。

## 能做什么

| 讨论时 | 讨论之后 |
|---|---|
| **边说边画**：流程图、分组、话题、多层要点、公式、问题和待办可以放在同一张板上。 | **摘要与溯源**：整理结论、决定、负责人和待办；从板上的内容跳回是谁、在哪一句提出来的。 |
| **圈着说**：先圈住对象，再说「这个标星」「把这两个连起来」，让指代有明确目标。 | **回放与分支**：按会议中的步骤重放、拖动进度、倍速观看，也能从某一刻另起一张板。 |
| **一起改板书**：手写模式下改字、移动、圈画、撤销；铅笔模式用 Excalidraw 自由编辑。 | **接着上次开会**：搜索旧会议，或拿旧板开启新会；继承的内容用淡墨，新改动用正常墨迹。 |

手写模式逐笔写出中英文，英文笔迹源自 Excalidraw 的 Excalifont。默认潦草度为 0、笔画粗细为 65%；潦草度达到 60 时，英文小写切换为连写体。浅色是马克笔白板，深色是粉笔黑板。界面支持中文和英文，板书与摘要会跟随会议语言。

### 圈着说

点顶栏的「圈选」，用鼠标或手指在**手写板**上画圈，再说话或发出一条文字：

> 「把这两个连起来。」　「这一段标个星。」　「在这个话题下面补一条：回滚演练。」

可以画多个圈，重叠的圈会合并；「撤销」退掉刚画的圈，`Esc` 或「取消」清空。下一句话带上圈内最多 8 个对象，说完就清掉。模型忙时，带圈选的句子各自保留目标，不会和下一句混在一起。鼠标悬停和普通点选不算圈选。

### 带着资料讨论

从顶栏导入，或把文件拖到白板上：

| 资料 | 可以怎样使用 |
|---|---|
| **PDF / 论文** | 每页一张板，保留页面底图；识别标题、段落、双栏、图表和公式，在原文的词、句、段上做标记或连线，批注写在旁边。 |
| **arXiv** | 桌面版可在导入菜单粘贴链接或论文编号，直接下载 PDF；网页版先下载文件再导入。 |
| **PNG / JPEG / WebP** | 用本地视觉组件读出文字，把截图或拍下来的白板变成可标注的资料。先在模型管理里下载与当前模型匹配的「看图组件」。 |
| **Excalidraw** | 把文字、形状、连接箭头和分组转成板书内容，再接着讨论；会重新排版，不保留原图坐标。 |

PDF 最多导入 40 张有效页面；没有文字层的扫描件只显示底图，不会自动 OCR。图片里的箭头不会自动转成可编辑连线。查看和标注导入页请用手写模式，铅笔模式目前不显示底图。

### 把会议带走

| 导出格式 | 适合 |
|---|---|
| **Markdown** | 结论、待办、板书和完整会议记录；流程图用 Mermaid 表示，方便继续编辑。 |
| **PNG / SVG** | 把当前板书放进文档或幻灯片，保留标记与导入页底图。 |
| **动态 SVG** | 约 12 秒把最终板书按会议顺序逐笔写出来；纯 CSS 动画，可嵌入 README，支持减少动态效果。 |
| **Excalidraw** | 在 Excalidraw 中继续编辑。 |
| **整场会 PDF** | 每张板一页，附摘要和记录，适合分发；页面为图片，可复制文字请用 Markdown。 |
| **回放网页** | 一个独立 HTML 文件，包含播放器、板书、记录和素材；接收者用浏览器离线打开，无需安装 boardcast。 |

## 实现原理

核心仍是「规则先给反馈，模型再确认」。现在语音、打字和圈选共用一条发言入口，导入资料与手写板上的修改也会进入同一份图状态：

```text
麦克风 / 系统声音 ──► Silero 断句 ──► SenseVoice 转写
                                             │ 语音定稿 / CAM++ 声纹
文字输入 ────────────────────────────────────┤
手绘圈选（最多 8 个目标）────────────────────┤
                                             ▼
                                  术语纠正 · 绑定本句圈选
                                             │
                  ┌──────────────────────────┴────────────────────────┐
                  ▼                                                   ▼
          快车道（本地规则）                                慢车道（单请求队列）
      提及高亮 / 明确连线先出草稿                        本地 llama.cpp / 可选兼容 API
                  │                                      读实体表、前两句、本句圈选
                  │                                            输出几行图操作
                  │                                                   │
                  └────────────────► 当前图状态 ◄─────────────────────┘
                                        ▲  │
      资料导入 / 手写板手动修改 ────────┘  ├──► 下一轮模型上下文
                                           │
                          ┌────────────────┴─────────────────┐
                          ▼                                  ▼
                      增量排版                           本地保存
              要点分区 / ELK 流程图               发言记录 + 板书变化 + 素材
                          │                                  │
                          ▼                                  ▼
               手写笔迹 / Excalidraw                   逐步回放 / 导出
```

- **语音定稿才画板书。** 临时字幕只用于显示；一句话定稿或发送文字后，先做术语纠正，再把这次圈住的目标附上。纯翻页口令直接切板，不走模型。
- **模型读上下文，输出图操作。** 输入包括当前实体表、别名与导入原文、最近两句、术语表和本句圈选；连线、条目、标记等操作由代码解析应用。快车道先出的草稿被确认后转成墨线，未确认的草稿会清掉。
- **排队不会混掉指代。** 同时只执行一个板书模型请求。普通发言可以合批，带圈选的发言逐句处理，近期上下文也保留各句原来的圈选目标。
- **本地与远程有不同约束。** llama.cpp 使用 GBNF 限定输出语法，并复用提示词前缀缓存；自选兼容 API 不带本地语法约束，失败时尝试可用的本地模型兜底。语法合法不代表语义一定正确。
- **位置和笔迹由代码决定。** 要点按区域摆放，流程图首次用 ELK 排版，之后尽量固定旧节点、增量放入新节点；导入页的原文位置保持固定。手写模式逐笔绘制，铅笔模式交给 Excalidraw。
- **回放不重新问模型。** 会议保存发言和板书变化记录，可重放或导出；摘要则单独读取会议记录与各张板书，再调用当前模型生成。

## 模型与隐私

桌面版自带 llama.cpp 的 `llama-server`，自动启动和退出，不用手动开终端。Linux / Windows 使用 Vulkan，可用时把模型层放到 GPU，其余留在 CPU；macOS Apple Silicon 使用 Metal。

| 内置可下载模型 | 下载大小 | 选择建议 |
|---|---|---|
| **Qwen3.5-4B · Q4_K_M** | 约 2.7 GB | 先从这一档开始，适合笔记本、核显或 CPU。 |
| **Qwen3.5-9B · Q4_K_M** | 约 5.7 GB | 可尝试更大的模型；建议至少 6 GB 显存，实际速度取决于可用显存与 CPU 分担。 |

下载支持暂停、续传、磁盘空间检查和 SHA-256 校验；依次尝试 ModelScope、hf-mirror、Hugging Face。看图组件按需下载，不影响普通语音和文字板书。

- **默认本地处理**：音频在本机识别，会议保存在本机；桌面版不需要账号或 API Key。
- **云端由你开启**：在设置里填写兼容接口。启用云端地址前会确认发送范围：发言、说话人、板书文字，以及生成摘要所需的记录；不会上传原始音频或原始会议文件。
- **每场会独立选择**：可以勾选「这场会只用本地模型」。远程请求失败时，会尝试本地模型兜底，前提是本地模型可用。
- **密钥的存放**：桌面版由 Rust 外壳代发请求，Key 存在本机配置文件中（不是系统密钥链）；网页版存在浏览器里，且受服务端 CORS 和浏览器安全限制。

<details>
<summary><strong>自定义模型、数据目录与排错</strong></summary>

可在模型目录放入 `.gguf`，或用 `BOARDCAST_MODEL=/path/to/model.gguf` 指定文件；是否能正确生成板书仍取决于模型及其模板。兼容服务在「设置 → 远程模型」里填写 Base URL、API Key、模型名，保存前会做一次真实请求检查。

| 系统 | 默认应用数据目录 |
|---|---|
| Linux | `~/.local/share/games.mmstudio.boardcast/` |
| Windows | `%APPDATA%\games.mmstudio.boardcast\` |
| macOS | `~/Library/Application Support/games.mmstudio.boardcast/` |

模型在 `models/`，会议 JSON 和导入素材在 `meetings/`。网页版会议保存在当前浏览器中，清理站点数据会删除它们，请先导出。远程配置 `remote.json` 在应用配置目录，与上表的数据目录不一定相同。

遇到问题可在「设置 → 诊断」预览并导出诊断包，再附到 [Issue](https://github.com/HsiangNianian/boardcast/issues)；诊断包不包含会议内容、音频或 API Key。

</details>

## 常见问题

<details>
<summary><strong>怎么使用一段现成的会议录音？</strong></summary>

目前没有直接导入 `.mp3` / `.wav` 转写的入口。在 Linux 或 Windows 桌面版中，将「设置 → 收音」切为「只收系统声音」，点开始收音，再用播放器播放录音，结束后停止收音。采集的是系统默认输出设备，需要正常播放，按音频实际时长处理。

在线会议选「麦克风 + 系统声音」，戴耳机能避免扬声器的声音被麦克风再次录入。macOS 尚不支持系统声音采集。

</details>

<details>
<summary><strong>语音识别、说话人和术语怎么处理？</strong></summary>

启动软件后会在后台预加载语音模型，开麦时复用；预加载不会打开麦克风，退出软件时会结束语音线程并释放模型。SenseVoice 负责中文、英文、日文、韩文、粤语转写；Silero 检测停顿，长句会尝试在换气处提前切开，避免一直等到说完。说话过程中有临时字幕。CAM++ 按声音区分说话人，可以在记录里改名、合并，也能在设置里关闭；短句和重叠发言仍可能分错。

项目代号或专有名词经常识别错时，在「设置 → 术语表」里写 `MediaCrawler = 媒体爬虫, 美的爬虫`。术语可全局使用，也可只跟着这一场会保存，打字输入同样适用。

</details>

<details>
<summary><strong>常用快捷键</strong></summary>

这些快捷键主要用于手写模式，输入框或对话框获得焦点时不会抢键；macOS 的常规操作用 `Cmd` 替代 `Ctrl`。

| 按键 | 操作 |
|---|---|
| 按住 `Space` | 桌面版按住说话 |
| `/` | 聚焦输入框 |
| `PageUp` / `PageDown` | 上一张 / 下一张板 |
| `Ctrl+Shift+N` | 新建一张板 |
| `Ctrl+O` | 导入 |
| `Ctrl+Shift+R` / `Ctrl+Shift+S` | 回放 / 摘要 |
| `Ctrl+Z` / `Ctrl+Shift+Z` | 撤销 / 重做 |
| `?` | 查看完整快捷键 |

全局收音快捷键默认是 `Ctrl+Shift+Space`，可在设置里修改。Wayland 下请把 `boardcast --toggle-mic` 绑定到桌面环境的快捷键。

</details>

## 开发与评测

需要 **Node.js 24+** 和仓库锁定的 **pnpm 11.13.1**。桌面开发还需要 Rust 与 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)；Linux 的音频构建还需要 ALSA 开发库（如 `libasound2-dev`）。

```sh
git clone https://github.com/HsiangNianian/boardcast.git
cd boardcast
pnpm install --frozen-lockfile
pnpm dev                       # http://localhost:1420，可直接回放内置示例
```

```sh
pnpm tauri dev                 # 桌面版；首次会下载 llama-server 与语音模型
pnpm site:dev                  # 官网，http://localhost:1430
pnpm typecheck
pnpm test                      # 引擎单元测试，无需模型
pnpm build                     # 类型检查、回放播放器和网页生产构建
```

只跑网页时，若要用自己的本地模型，可另开终端运行 llama.cpp，再在应用里配置本地地址（默认留空，不填就不会去连本地模型）：

```sh
llama-server -m /path/to/model.gguf --host 127.0.0.1 --port 8080 -c 4096 --jinja -np 1
```

<details>
<summary><strong>代码入口</strong></summary>

| 入口 | 内容 |
|---|---|
| [`src/engine/`](src/engine/) | 会话、提示词、图操作、语法约束、评测相关测试 |
| [`src/ink/`](src/ink/) / [`src/board/`](src/board/) | 笔迹、圈选、排版、文档导入 |
| [`src-tauri/src/`](src-tauri/src/) | 语音、模型下载、进程管理、存储、更新 |
| [`src/player/`](src/player/) | 独立回放网页 |
| [`landing/`](landing/README.md) | 官网；Pages 首页为官网，底部嵌入 `/demo/` 演示 |

</details>

一键下载共享的中日英音频，在本地试听和评测：

```sh
pnpm eval:audio                # 下载固定版本 WAV、校验哈希，生成可离线打开的 HTML
pnpm eval:listen               # 按需下载后启动 http://127.0.0.1:4175
pnpm eval:asr                  # 通过桌面版实际语音引擎跑快速回归；需要 Rust
pnpm eval:asr --suite full     # 包含完整会议片段
pnpm eval:asr --vad off        # 对照组：停用 VAD，按固定窗口识别
```

模型对比、降噪实验、参考转写质量见[语音评测说明](eval/asr/README.md)，素材条款见[数据集许可](eval/asr/LICENSES.md)。日文使用真人朗读样本；重叠会议整段和待复核的中文转写不计入普通错误率汇总。

<details>
<summary><strong>评测数据、复现与已知边界</strong></summary>

| 集合 | 当前规模 | 用途 |
|---|---|---|
| [`eval/cases.json`](eval/cases.json) | 28 段 | 开发与回归，含英文和真实会议回归样本 |
| [`eval/holdout.json`](eval/holdout.json) | 27 段 | 不参与提示词调参的留出评测 |
| [`eval/pointing.json`](eval/pointing.json) | 18 段 | 中英文圈选指代专项，使用固定初始板书 |

先启动可用的 llama-server，再运行：

```sh
pnpm eval --set dev --label my-model
pnpm eval --set holdout --label my-model
pnpm eval --set pointing --repeat 3 --label my-model
# 服务不在默认的 8080 时，加 --base-url http://127.0.0.1:端口
node eval/run.ts --only login-flow -v
node eval/run.ts --file path/to/corrections.cases.json
```

报告写入 `eval/reports/`。v0.20.0 的 Qwen3.5-4B 圈选专项（18 段 × 3 遍）要点 F1、划掉 F1、标记命中均为 1.000；这是合成用例的局部结果，不是任意真实会议的准确率，也不含语音识别耗时。通用留出集仍有纠正没生效的已知失败。模型与硬件、提示词版本和复现说明见[圈选评测记录](eval/pointing.md)及[版本记录](CHANGELOG.md)。

模型会漏内容、误连线或误判强调，识别和分段也会影响结果，重要结论仍需核对。手动修正模型刚画完的内容后，可在「设置 → 纠错样本」导出评测用例或 DPO / SFT JSONL；这些样本不会自动上传。

</details>

<details>
<summary><strong>CI、部署与发版</strong></summary>

- [Eval](.github/workflows/eval.yml)：统一执行类型检查、单元/Rust 测试、网页/桌面构建、文本到板书评测和中日英 ASR 回归。每周或手动完整评测比较 SenseVoice / Whisper base，以及原音 / GTCRN 降噪、VAD 开 / 关，共八组对照。见[语音评测说明](eval/asr/README.md)。
- [Web demo](.github/workflows/pages.yml)：构建官网与网页演示，部署至 GitHub Pages。
- [Release](.github/workflows/release.yml)：推送 `v*` tag 后生成草稿，四个平台都成功才正式发布，并提供签名更新包和 `latest.json`。

使用约定式提交。`pnpm release <下一版本> --dry-run` 预览更新日志；`pnpm release <下一版本>` 更新四处版本号、生成 CHANGELOG、提交并打 tag；加 `--push` 会推送提交和 tag。尖括号部分需要替换为实际版本号。Release 正文取自 [CHANGELOG.md](CHANGELOG.md) 对应章节。

带数字后缀的版本是预发布，不推送给自动更新；Windows MSI 要求数字后缀，不能写成 `-rc.1`。预发布不打 RPM。签名使用仓库的 `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，公钥在 `src-tauri/tauri.conf.json`，请保留签名私钥以维持既有安装的更新能力。

</details>

## 参与与许可

发现问题或想讨论下一步，欢迎提 [Issue](https://github.com/HsiangNianian/boardcast/issues) 或 PR；功能进展以 Issues 和 [CHANGELOG](CHANGELOG.md) 为准。

代码采用 [MIT](LICENSE)。中文笔画数据来自 [hanzi-writer-data](https://github.com/chanind/hanzi-writer-data)（源自 Make Me a Hanzi 与 Arphic 字体，Arphic Public License，随生成的数据分发许可文本）；英文默认笔迹由 Excalifont 提取，沿用 [SIL OFL 许可](public/licenses/handwritten-OFL.txt)；连写体 Hershey 字形来自 [hersheytext](https://github.com/techninja/hersheytextjs)（MIT）。界面使用 DM Sans、JetBrains Mono，官网另有思源黑体子集；这些字体使用 SIL OFL，官网字体许可见 [OFL.txt](landing/public/fonts/OFL.txt)。
