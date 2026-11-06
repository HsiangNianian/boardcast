# C61 续批：模型目录、下载恢复与长诊断

2026-11-04，HEAD `4f9ab26`。接续模型状态提示，处理模型目录与下载操作中的固定英文；C61 和整体目标仍保留。

后续实际下载行为见 2026-11-05 [C16 Linux 原生暂停与恢复](native-download-validation.md)：增加真实原生停滞取消、部分文件冷启动、故障重试和受控 TLS 备用源完整下载证据。本报告下文保留当时仅检查目录和文案的范围。

## 修改与复现

五个内置模型的显示名称、详情、配套模型名称、进度条标签和删除确认使用语言字典。原生模型 ID 不变；未知模型仍显示原始名称与详情。英文目录与 Rust 实际清单逐项核对，新增模型或修改说明后若未同步字典，契约测试会失败。

模型管理在渲染时转换 15 种固定原生消息格式，覆盖部分文件/完整文件校验、空间不足、其他下载占用、未下载、无法选择、目录/文件写入和哈希失败。容量数字、完整哈希、路径和底层错误保留。未知消息保持原文，下载错误另有本地化的重试指引。当前仍按固定原生文本识别，后端改动返回格式时必须同步前端，代码已注明限制。

窄窗检查还复现长哈希撑宽模型面板：旧生产构建的内容宽 544 px、可用宽 538 px。模型面板和删除确认现在允许长单词换行，最终中英均为 538/538，完整哈希不截断。见[旧构建记录](evidence/implementation/model-download-language/baseline-browser/check.json)、[最终记录](evidence/implementation/model-download-language/browser/check.json)和[窄窗截图](evidence/implementation/model-download-language/browser/zh-checksum-compact.png)。

旧构建有 21/40 处文案比较不符，19 项内容/操作保护断言通过，最终文案断言按预期失败；新构建全部通过。改动见[本批补丁](evidence/implementation/model-download-language/product-and-tests.patch)，未更改 Rust 下载、文件校验或会议保存逻辑。

## 已验证范围

| 检查 | 证据与范围 |
| --- | --- |
| 生产 App | [21 项断言、40 处文案比较](evidence/implementation/model-download-language/browser/check.json)：中英目录与详情、视觉组件配套名称、进度条可访问名称、两类校验、哈希失败、容量错误、未知网络诊断、删除确认/失败、窄窗换行；原生 IPC 和事件受控 |
| 恢复与数据保留 | 暂停保留部分文件大小并提供续传；预检查失败后重新读取不会自动下载；失败删除保留模型和确认窗口。下载相关操作不写会议；删除前既有保存屏障仍运行，之后标题、图、原话、页数及当前页保持，允许既有单页压缩编码与 revision 更新时间变化 |
| 活动错误切换语言 | 注入系统语言变化，当前错误即时切换，原生进度和哈希对象不变；这是浏览器系统事件模拟，不是实际操作系统语言切换 |
| 模型管理回归 | [16 项原有检查](evidence/implementation/model-download-language/models-regression/models-check.json)在最终样式下通过；保留保存屏障、重复提交保护、删除取消/重试、下载状态与订阅清理 |
| Node / 类型 | [12 项通过](evidence/implementation/model-download-language/node-tests.txt)，含实际 Rust 目录契约、未知/原型名称 ID、15 种消息、复杂路径、无效哈希近似消息保持；类型检查通过 |
| 下载底层 | [6 项 Rust 检查通过](evidence/implementation/model-download-language/rust-model-tests.txt)：空间不足、文件清单/视觉配套、本地 HTTP 源回退与续传、坏校验删除、取消保留及恢复。外网真实模型下载测试仍为 ignored |
| 构建 | 播放器、主应用及 Linux Tauri debug 构建通过；换行修复后重新构建主应用与原生二进制 |

运行器均显式关闭 context 和浏览器。检查编写中的事件先后及单页存储编码假设修正，记录在[运行说明](evidence/implementation/model-download-language/harness-notes.txt)，不据此更改产品状态机或保存行为。

## 真实 Linux 目录

在独立标识 `games.mmstudio.boardcast.g2.downloadlang`、XDG、D-Bus 和 Xvfb `:90` 中启动最终二进制，通过 cua-driver 切换中文、打开模型管理、逐项展开详情。真实新安装目录显示“语音识别模型”及两个 Qwen 板书模型；[语音说明](evidence/implementation/model-download-language/native-voice-details-zh.png)和[两个板书模型的说明](evidence/implementation/model-download-language/native-all-details-zh.png)均为中文，`voice`、`qwen3.5-4b`、`qwen3.5-9b` ID 保持原样。模型目录仍没有文件，未开始下载。见[7 项核对](evidence/implementation/model-download-language/native-check.json)。

原生后台语言点击未生效，后台滚动明确返回 `background_unavailable`；根据新截图在隔离显示器内完成前台操作，没有操作用户当前桌面。视觉配套条目在该新安装状态下未展示，其文案由生产浏览器目录场景及原生目录契约覆盖。

## 身份、清理和剩余边界

生产入口 `index-DZZGEhfs.js`，SHA-256 `fd16ffc34b608e121f006cd504bf769decb3142c1a4e003f940cd8259ada7a41`；样式 `index-BEQch6T5.css`。原生二进制 SHA-256 `db46574f4f8455a2b7193b4f73ead1367a3b0f712bdd365118604c1aaf9d098c`，确认嵌入最终入口和样式。191 个源码/构建输入保持一致，私有项目的 23 个原生文件与工作树一致，见[构建身份](evidence/implementation/model-download-language/identity.json)。

没有执行外网模型下载、真实原生下载故障、模型推理或 macOS/VoiceOver；未重跑全量 Node/Rust。其他原生错误、菜单/快捷键、G3/G4 和完整 C01–C68 验收仍继续，不能把本批当作 C61 全部完成。

未提交、推送或发布。预览、原生窗口、Cua 和私有显示器已清理，用户 1423/1430 服务保持原 PID，见[清理记录](evidence/implementation/model-download-language/cleanup.json)。
