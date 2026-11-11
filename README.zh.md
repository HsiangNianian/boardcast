<div align="center">

# boardcast

**说话，板书自己长出来。**

本地语音识别 · 实时手绘板书 · 会议回放与导出

[官网](https://hsiangnianian.github.io/boardcast/) · **[下载桌面版](https://github.com/HsiangNianian/boardcast/releases/latest)** · [更新日志](CHANGELOG.md)

[![最新版本](https://img.shields.io/github/v/release/HsiangNianian/boardcast?style=flat-square&color=c74432)](https://github.com/HsiangNianian/boardcast/releases/latest) [![Eval](https://github.com/HsiangNianian/boardcast/actions/workflows/eval.yml/badge.svg)](https://github.com/HsiangNianian/boardcast/actions/workflows/eval.yml) [![MIT](https://img.shields.io/badge/license-MIT-24292f?style=flat-square)](LICENSE)

[English](README.md) · **中文**

</div>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshot-chalk.png">
  <img src="docs/screenshot-ink.png" alt="文件上传流程、待办事项和星标，与右侧会议记录并排显示" width="1280">
</picture>
<p align="center"><sub>旧版应用里的一张板。现在的应用画出的板书一样，外面的窗口有所不同。</sub></p>

boardcast 把讨论变成一张持续生长的白板：流程、要点、分工、问题和临时否掉的方案，都能随对话写上去、连起来、划掉。你也可以导入论文或截图，一边讨论，一边在原文上圈画。

默认在本机完成语音识别和板书模型推理。模型下载好后可以离线开会；需要更大的模型时，也能连接 OpenAI 兼容接口。

桌面版是用 Rust 和 [GPUI](crates/README.md) 重写的原生应用。之前的 Tauri 版已经不再发布，它的一些功能[现在的应用还没有](#现在的应用还没有)。

[开始使用](#开始使用) · [能做什么](#能做什么) · [快捷键](#快捷键) · [模型与隐私](#模型与隐私) · [开发](#开发与评测)

## 开始使用

1. 从 [Releases](https://github.com/HsiangNianian/boardcast/releases/latest) 下载安装包。各个包里的应用相同，区别只在内置的语音模型：

   | 安装包 | 能识别 | 语音模型 |
   |---|---|---|
   | `…-macos.zip`、`…-linux.tar.gz`、`…-windows.zip` | 日语、英语和中文，各用最合适的模型 | 约 1.1 GB |
   | `…-ja` | 日语，用 ReazonSpeech | 约 160 MB |
   | `…-en` | 英语，用 Parakeet | 约 660 MB |
   | `…-macos-none.zip` | Mac 自带识别器支持的语言 | 无 |

   提供 Apple Silicon / Intel 的 macOS、x64 的 Linux 和 Windows 安装包。

2. **macOS：** 应用只做了临时签名，第一次打开请按住 Control 点击 › 打开。**Linux：** 解压后运行 `./install.sh`（装到 `~/.local`，不需要 root；`--uninstall` 卸载并保留数据）。**Windows：** 解压 zip 后运行 `bin\boardcast.exe`。
3. 第一次启动时，设置会停在**板书模型**：下载一个在本机运行的模型，或者设置远程模型。语音识别已经内置。
4. 点**开始收音**（或按 `空格`），也可以在转写下方的输入框里打一句话，回车上板。

Linux 安装版启动后不久会检查新版本，在后台安装，等你重启后生效，不会打断正在开的会。macOS 和 Windows 暂时需要手动下载新包。

## 能做什么

| 开会时 | 开完会 |
|---|---|
| **边说边画**：流程、话题、层层嵌套的条目、问题、公式和批注都在同一张板上。 | **摘要**：把结论、决定和待办整理成摘要，可以修改、勾选和复制。 |
| **圈着说**：先圈住东西，再说「把这两个连起来」或「这个打星」，模型就知道你指的是什么。 | **回放与分叉**：看板书一步步长出来，可以拖动、变速，或者从任一时刻另起一张板。 |
| **改正板书**：选中内容后可以划掉、圈出、下划线、星标、高亮，也可以改字或删除，全都可以撤销和重做。 | **追溯谁说了什么**：鼠标移到说话人上，板上属于他们的部分会突出；从条目可以跳到对应的原话。 |
| **按声音区分说话人**，并改成真名；同一个声音之后都沿用这个名字。 | **找回会议**：在会议列表里找到之前的会，接着开。 |

板书按笔顺一笔一笔写出中文、日文和英文。浅色是白板马克笔；系统为深色时，画布变成粉笔黑板。界面语言是英文或中文，跟随系统或在设置里选择；板书和摘要跟随会议本身的语言。

新会议可以从周会、方案评审、复盘或站会模板开始，也可以把当前板的话题存成自己的模板；摘要会按模板话题组织。在设置里填写术语表后，中英日名字可以按读音纠错，并把规范写法交给板书模型；识别原文仍会保留。

### 圈着说

点底部工具条的套索，然后在板上拖动画圈。可以画多个圈；中心落在圈里的东西都会被圈住，最多 8 个。下一句话（说出来或打出来）会带着这些对象交给模型，然后圈消失。按 `Esc` 或再点套索取消。圈不会进入板书，也不会出现在导出里。

### 带着资料讨论

用**导入**，或者把文件拖到板上：

| 资料 | 可以做什么 |
|---|---|
| **PDF / 论文** | 每页一张板，原页面垫在下面。标注词、句和段落；把区域连起来，写批注。 |
| **arXiv** | 贴链接或论文编号，下载 PDF。 |
| **PNG / JPEG / WebP / GIF** | 用本地看图组件读出截图或白板照片里的文字。 |

### 把会议带走

| 导出 | 用途 |
|---|---|
| **Markdown** | 要点、Mermaid 流程图和会议记录。 |
| **PNG / SVG** | 当前这张板，和画面上一样；SVG 放大不糊。 |
| **动态 SVG** | 当前这张板，按会议顺序逐笔写出。 |
| **GIF** | 板书逐笔写出的动图，可以直接贴进群聊和文档。 |
| **回放网页** | 一个 `.html` 文件，发给没参会的人，双击就能看。 |
| **整场会 PDF** | 每张板一页，后面是摘要和会议记录，文字可以选中复制。 |

无论窗口是什么主题，导出都是白纸马克笔。

### 线上会议和录音

在**设置 › 语音 › 收音来源**里选择麦克风；在 Linux 上也可以选播放会议声音的扬声器或耳机对应的 **Monitor of …** 输入，这样就能收到线上会议里对方的声音，或者播放器里正在放的录音。v0.27.0 安装包一次只收一个来源。在 Mac 上，任何安装包都可以在同一处改用 Apple 自带的识别器。

### 发布状态和剩余工作

[#206](https://github.com/HsiangNianian/boardcast/pull/206) 恢复了 Mac 麦克风、系统及双路收音，增加两路独立的 Apple 语言选择和有限容量的收音诊断。这些尚未进入 v0.27.0；真实 Mac 的授权、双路转写和长会仍需按[验收计划](eval/macos/README.md)核对。「系统语言」跟随操作系统，不是自动检测正在说的语言。全局收音快捷键和完整诊断包仍未迁回。Excalidraw 自由编辑和导出已从 GPUI 版移除。

相对最新正式版 **v0.27.0**，`main` 还增加了 PDF 选页导入（[#202](https://github.com/HsiangNianian/boardcast/pull/202)）和单句改说话人（[#201](https://github.com/HsiangNianian/boardcast/pull/201)）；这两项尚未进入该版本的安装包。改单句暂不修改对应板书内容的归属，说话人的声纹统计会随会议保存；逐句声纹只在会议打开期间保留，因此重新打开后改旧句子无法重算对应的声纹统计。

## 快捷键

焦点在画布上时生效，不影响输入框。macOS 上用 `Cmd` 代替 `Ctrl`。

| 按键 | 作用 |
|---|---|
| `空格` | 开始 / 停止收音 |
| `/` | 输入一句话 |
| `Esc` | 清除圈选或选中，或者关闭面板 |
| `Ctrl+Z` / `Ctrl+Shift+Z` | 撤销 / 重做 |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | 放大 / 缩小 / 适应屏幕 |
| `Ctrl+Shift+R` | 回放；回放中 `空格` 播放或暂停，`←` `→` 逐步，`Esc` 退出 |

## 模型与隐私

应用内置 llama.cpp 的 `llama-server` 并替你管理：Linux 用 Vulkan，macOS 用 Metal，必要时退回 CPU。在**设置 › 板书模型**里选择：

| 模型 | 下载大小 | 说明 |
|---|---|---|
| **Qwen3.5-4B · Q4_K_M** | 约 2.7 GB | 默认；笔记本和集成显卡都能用。 |
| **Qwen3.5-9B · Q4_K_M** | 约 5.7 GB | 更大；建议 6 GB 以上显存。 |
| **MiniCPM5-2B**、**MiniCPM5-1B**、**MiniCPM-V 4.7 1B** | 更小 | 适合配置较低的电脑；各自的局限在应用里写在旁边。 |

下载可以暂停和续传，会检查剩余空间并校验 SHA-256，依次尝试 ModelScope、hf-mirror 和 Hugging Face。第一次启动会为显卡编译着色器，大约半分钟，只需一次。

- **默认本地**：音频在本机识别，会议也留在本机。不需要账号或 API key。
- **按需远程**：在**远程模型**里填 OpenAI 兼容的地址、模型和 key，或者一键填写 DeepSeek，然后**测试连接**并**保存**。发给该服务的是每句话、说话人名字和板书文字，不发送音频。
- **key 的保存**：key 存在设置目录的 `remote.json` 里，只有你自己能读，也不会写进日志。

| | 会议与模型 | 设置（`remote.json`） |
|---|---|---|
| Linux | `~/.local/share/games.mmstudio.boardcast/` | `~/.config/games.mmstudio.boardcast/` |
| macOS | `~/Library/Application Support/games.mmstudio.boardcast/` | 同一个目录 |
| Windows | `%APPDATA%\games.mmstudio.boardcast\` | 同一个目录 |

可以用 `BOARDCAST_DATA` 和 `BOARDCAST_CONFIG` 改到别处。

## 开发与评测

桌面版是一个 Cargo workspace，各 crate 的分工见 [crates/README.md](crates/README.md)。在 Linux 上请安装 CI 用到的这些包（Debian / Ubuntu）：`pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev libvulkan-dev libasound2-dev libssl-dev libzstd-dev libfontconfig-dev libfreetype-dev`。

```sh
git clone https://github.com/HsiangNianian/boardcast.git
cd boardcast
cargo run --release -p boardcast-ui     # 桌面版
cargo test --workspace
sh packaging/linux/package.sh           # target/package/boardcast-<version>-<arch>-linux.tar.gz
```

全部代码都是 Rust，官网也一样：用 [Trunk](https://trunkrs.dev) 构建（`cd landing && trunk serve`），见 [landing/README.md](landing/README.md)。

<details>
<summary><strong>评测</strong></summary>

| 用例集 | 当前规模 | 用途 |
|---|---|---|
| [`eval/cases.json`](eval/cases.json) | 28 场会 | 开发与回归，包含英文和一场真实会议的回归 |
| [`eval/holdout.json`](eval/holdout.json) | 27 场会 | 不参与提示词调整的留出集 |
| [`eval/pointing.json`](eval/pointing.json) | 18 个用例 | 中英文圈着说，起始板固定 |
| [`eval/revisions.json`](eval/revisions.json) | 3 个用例 | 转写修订，必须保留节点身份和无关内容；等 GPUI 版支持转写修订后再跑 |

摘要可信度和模板结构使用[重复模型评测](eval/summary.md)。术语表实录对照从[自己的录音](eval/asr/own-voice/README.md)开始；Mac 长会和目标用户试用分别使用[验收计划](eval/macos/README.md)及[试用材料](docs/trials/README.md)。

先启动可用的 llama-server，然后运行：

```sh
cargo run --release -p boardcast-tools -- agent-eval . board.json --llm http://127.0.0.1:8080 --set dev --label my-model
cargo run --release -p boardcast-tools -- agent-eval . board.json --llm http://127.0.0.1:8080 --set holdout --label my-model
cargo run --release -p boardcast-tools -- agent-eval . board.json --llm http://127.0.0.1:8080 --set pointing --repeat 3 --label my-model
```

语音识别有自己的评测：

```sh
cargo build --release -p boardcast-asr-eval
target/release/asr-eval download . --suite quick   # 下载并校验录音，生成试听页面
target/release/asr-eval run . --suite quick        # 用新版自己的识别流程跑，按语言打分
target/release/asr-eval run . --suite meetings     # 9 段约 10 分钟、跨行业的会议录音；需要 ffmpeg
```

各配置、参考文本质量和[数据集许可](eval/asr/LICENSES.md)见[语音评测](eval/asr/README.md)。CI 在 [Eval](.github/workflows/eval.yml) 工作流里用 Qwen3.5-4B 运行这些评测。模型可能漏掉内容、连错对象或看错重点，识别质量也会影响结果，重要结论请再核对。

</details>

<details>
<summary><strong>发布</strong></summary>

提高 workspace 版本号并合并，然后推送一个带注释的 tag，注释就是发版说明：`git tag -a v<version> -m "…"`，再 `git push origin v<version>`。[Release](.github/workflows/release.yml) 工作流构建 Linux（all、ja、en）和 Mac（Apple 芯片与 Intel，各 all、ja、en、none）安装包、写出更新清单，并把 `v<version>` 发布为最新版本。Windows x64（all、ja、en）单独构建并追加附件，Windows 构建失败不会阻止其他包发布。带预发布部分的版本（如 `0.23.0-1`）发布为预发布版，不会推送给已安装的用户。提交信息使用 Conventional Commits；新版发版说明在 GitHub Releases，[CHANGELOG.md](CHANGELOG.md) 保留旧应用截至 0.21.0 的记录。

</details>

## 参与与许可

在 [Issues](https://github.com/HsiangNianian/boardcast/issues) 里报告问题或讨论想法，也欢迎直接提 PR。

代码使用 [MIT](LICENSE) 许可。中文笔画数据来自 [hanzi-writer-data](https://github.com/chanind/hanzi-writer-data)，源自 Make Me a Hanzi 和文鼎字体，采用 Arphic Public License，许可证随生成的数据一起分发。日文假名和补充汉字来自 Ulrich Apel 的 [KanjiVG](https://kanjivg.tagaini.net/)，改为中心线后以 [CC BY-SA 3.0](assets/licenses/kanjivg-CC-BY-SA-3.0.txt) 分发。默认的英文和数学符号笔画源自 Excalifont，采用 [SIL OFL](assets/licenses/handwritten-OFL.txt)；另有部分数学符号来自内置的 Cascadia Code 字体，见其[附带许可](assets/licenses/handwritten-cascadia.txt)。连笔 Hershey 字形来自 [hersheytext](https://github.com/techninja/hersheytextjs)，采用 MIT。桌面版内置 DM Sans 和 JetBrains Mono，采用 SIL OFL（见 [crates/ui/fonts](crates/ui/fonts)）。官网使用 DM Sans、JetBrains Mono 和未修改的 Excalifont 拉丁子集，均采用 SIL OFL，见 [landing/public/licenses](landing/public/licenses)。
