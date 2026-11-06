# C63 诊断预览与原生安全导出

2026-11-03。已用实际 Linux Tauri/WebKit、Rust IPC 和系统保存对话框完成敏感样本、取消、权限失败及重试验证。归档的 [18 项证据检查](evidence/implementation/native-diagnostics/native-check.json) 通过。本报告覆盖 C63 的本批 G1/G2 路径，不替代整份方案的 Mac/G3/G4 验收。

## 发现与修复

原有 `cleanLog()` 只删除含非 ASCII 字符的行，实际原生导出的 [修复前 ZIP](evidence/implementation/native-diagnostics/before.zip) 因此包含英文会议句子、没有前缀的续行和 JSON 请求正文。中文样本被删除、凭据被替换，仍不能证明会议内容安全。输入均为 [合成样本](evidence/implementation/native-diagnostics/log-input.txt)，只写入独立测试实例的模型日志，未使用用户会议或真实服务凭据。

现在诊断包保存以下文件，全部可在导出前展开预览：

| 文件 | 内容 |
| --- | --- |
| `info.json` | 应用、系统、模型和输入状态；错误字段转为类型、固定信号和数值错误码；路径复用现有脱敏函数 |
| `timing.json` | 句数和耗时统计，不含原话 |
| `errors.json` | 最近错误的时间、受限来源、类型及代码，不含错误原文或堆栈 |
| `llama-server.json` | 有模型日志时才加入；只保留行数、固定信号、上下文大小和数值耗时，不保留日志原文 |

不再以语言判断某行是否可能属于会议。`error`、`message` 和 `appleError` 等字段统一生成错误摘要；`appleError` 曾在第一版摘要规则中遗漏，新增样本先 [复现失败](evidence/implementation/native-diagnostics/apple-error-before.txt)，修复后通过。字符串在 JSON 转义前脱敏，补上 Windows 路径转义后无法匹配的问题；结构中的 API Key/Authorization 字段直接替换。

本地最近错误的详细文本与复制入口保留原来的行为；诊断 ZIP 使用独立摘要。代价是导出包不再包含完整错误堆栈或原始模型日志。中英文说明明确列出包含/排除内容，技术 JSON 默认折叠。保存失败文案也避免重复的 `Save failed: Save failed`。

## 实际桌面证据

| 路径 | 验证结果 |
| --- | --- |
| 原生菜单打开诊断 | 显示包含/排除说明与四个折叠文件，展开后显示 JSON 摘要 |
| 系统保存取消 | 回到同一份预览，保留文件名称/字符数及展开的日志摘要，没有成功提示，导出可再次操作 |
| 写入权限失败 | 选择测试目录内已有的 `0400` 文件并确认覆盖，Rust 返回 `Permission denied (os error 13)`；预览保留，导出按钮可重试，原文件字节与权限不变 |
| 更换位置重试 | 重新选择新路径并保存，系统对话框关闭，显示实际文件路径的成功提示，诊断预览关闭，原会议两句仍在 |
| 解包最终产物 | ZIP CRC 通过，只包含上述四个 JSON；会议、日志、凭据和用户名样本均未进入文件；展开过的日志摘要与 ZIP 字节完全相同 |
| 数值信息 | 保留两句会议的耗时统计（最大 375 ms）、日志上下文 4096 tokens、模型计时 12 ms / 4 tokens；本轮原生错误数组为空 |

关键证据：[预览](evidence/implementation/native-diagnostics/final2-preview.png)、[展开摘要](evidence/implementation/native-diagnostics/final2-expanded.png)、[取消后](evidence/implementation/native-diagnostics/final2-cancelled.png)、[权限失败](evidence/implementation/native-diagnostics/clean-permission-result.png)、[成功保存](evidence/implementation/native-diagnostics/clean-saved.png)、[最终 ZIP](evidence/implementation/native-diagnostics/final-clean.zip)。另检查了实际保存凭据不在 ZIP 中，以及样本文件与 Rust 读取的原生日志逐字节相同，见 [隐私上下文核对](evidence/implementation/native-diagnostics/privacy-context.json)。

非空错误、Apple 错误、Windows 路径及三次取消/失败/成功导出的全文件字节一致性由实际 `DiagnosticsPanel` 配合受控 native bridge 验证；不将这些 fixture 结果称为真实 Apple/Windows 运行。640×400 中英窗口也检查了无横向溢出和可滚动到导出按钮。

## 构建与复查

沿用独立测试目录 `/home/hsiangnianian/.cache/boardcast-g2-remote-r3g2f6ci`，应用标识 `games.mmstudio.boardcast.g2.remote`，独立 XDG 数据、私有 D-Bus/AT-SPI/Xvfb。原生操作经 Cua 0.34.0；未调用物理麦克风，测试进程的外部代理指向不可连接的本地端口。本批没有更改 Rust 业务代码。

- 修复前二进制为上一批的 `33f5cc04255d4e4473c77bf73bc02f1ff4b612a287fbb0b32722ee60e894482a`，嵌入 `index-DxrjKKf1.js`。
- 本批最终前端为 `index-CSwxDvd6.js`，原生二进制为 `71a90df2ba19066bad98daef9634188895b19ea38a9cc5204a6b89749389bfd4`。取消、权限失败和保存重试均在此最终构建复核。[192 项源码/配置哈希](evidence/implementation/native-diagnostics/final-source-hashes.json) 与当时工作树一致，私有 Rust 源码及二进制内嵌资源也已 [核对](evidence/implementation/native-diagnostics/source-check.json)。
- 最终 ZIP 为 2,124 bytes，SHA-256 `7fc54b291fcb760f832a188c4b3e960b063f34b3a88cb29207800ffeb0019613`。两轮最终构建保存的文件逐字节相同。
- 完整 [Node 检查](evidence/implementation/native-diagnostics/node-tests.txt) **259 项通过**；类型检查、[维护/Dialog 浏览器检查](evidence/implementation/native-diagnostics/panels.txt)、生产构建、[独立 Tauri 构建](evidence/implementation/native-diagnostics/final-build-native.txt) 通过。既有体积提示仍存在，没有安装依赖。

证据复查命令：

```sh
python scripts/ui/check-native-diagnostics.py docs/ui-ux-audit-2026-11-03/evidence/implementation/native-diagnostics
```

该脚本解包已保存产物并核对 Cua 快照，不会重新操作应用。组件检查继续使用 `check-maintenance.cjs`，运行方式见 [scripts/ui/README.md](../../scripts/ui/README.md)。

## 自动化与平台边界

一次背景输入/保存序列出现了两个重叠覆盖确认框，且无标题框没有出现在 Cua 的 `list_windows` 中。通过 X11 的只读窗口元数据找到残留框，取消后继续同一应用进程。随后用前台键盘输入、单次提交和精确窗口点击重走权限失败→直接重试→保存成功，没有依赖重启恢复。这个现象的根因未确定，保留 [残留框快照](evidence/implementation/native-diagnostics/final2-extra-dialog.json)；不能据此声称所有平台对话框均已验证。截图中的黑块/光标来自本次私有显示捕获，原图保留，文字状态同时以 AX 快照和真实文件核验。

没有执行真实 macOS/Windows 保存对话框、Apple 错误来源、VoiceOver、真实签名安装、真实音频或 G4 用户试用；本批 ZIP 验收也不替代七种会议导出格式的原生打开流程。原生应用经正常菜单退出，私有显示、Cua、AT-SPI 和窗口管理器均已停止，见 [清理记录](evidence/implementation/native-diagnostics/cleanup.json)。构建及证据保留，未提交、推送或发布。
