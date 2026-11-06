# C24/C54：原话生成状态与矮窗口阅读

2026-11-05，基线 `4f9ab26`。落实方案 §5.2：每句原话先说明生成结果，耗时放入详情；接续[长会阅读](transcript-reading-validation.md)和[连续尺寸变化](transcript-resize-validation.md)。

## 改动

- 原话显示等待、生成中、已更新白板、无需修改白板或已处理。复用 Session 的活动批次，并同时核对句子 ID 和修订号；前一张板的重试也读取对应 Session。没有新增任务状态或存储字段。
- 合并批次的前序句和缺少操作详情的旧记录不会被误说成“没有改动”。取消、模型未配置、普通失败保留各自说明和恢复入口；前一张板重试成功会清除旧的本地兜底标记。
- 状态及已保存的模型来源放在原话下方，长名称和来源正常换行。耗时与操作留在可用键盘展开的详情中，包括真实测得的 0 毫秒。手动编辑不增加模型状态。
- 实际 Linux 720×480 窗口扣除菜单后有 453 px 内容高度，原来的 450 px 断点未覆盖它，固定侧栏头部会挤掉原话视口。将既有矮窗口断点改为 500 px，复用整个侧栏滚动和现有阅读锚点逻辑。

产品涉及 5 个文件，另有 2 个测试文件；见[本批差异](evidence/implementation/transcript-state/product-and-tests.patch)和[前后哈希](evidence/implementation/transcript-state/changed-files.json)。未增加依赖。

## 验证

| 范围 | 结果 |
| --- | --- |
| 旧版对照 | [生产界面](evidence/implementation/transcript-state/baseline-browser.log)仍显示毫秒，新增检查先失败；[Session 检查](evidence/implementation/transcript-state/baseline-session.log)先因缺少活动状态查询失败。 |
| Node / Eval | [322 项通过](evidence/implementation/transcript-state/node-tests.log)，新增检查覆盖活动/排队、修订号、下一批、取消及迟到完成。 |
| 生产界面 | [34 项通过](evidence/implementation/transcript-state/browser/check-transcript-state/report.json)：真实 IndexedDB、受控请求、连续三句排队、合并批次、旧板重试、来源保留、失败说明、键盘详情；中英文、明暗主题及 1280×800 / 900×600 / 720×480 / 360×240 布局。只读交互不改写浏览器中的会议。 |
| 真实浏览器缩放 | [17 项通过](evidence/implementation/transcript-state/browser-zoom.json)，临时扩展调用 `chrome.tabs.setZoom`，验证 100% / 200%、DPR、CSS 视口和 `visualViewport.scale === 1`；检查长来源、换行、重叠及键盘焦点。已查看[中文最小视口](evidence/implementation/transcript-state/zoom/zoom-zh-720-2-light.png)和[英文暗色](evidence/implementation/transcript-state/zoom/zoom-en-1280-2-dark.png)。 |
| 相关回归 | [40 板 / 2000 条原话阅读 55 项](evidence/implementation/transcript-state/browser/check-transcript-reading/transcript-reading-check.json)、[远程隐私 31 项](evidence/implementation/transcript-state/browser/check-remote-privacy/remote-privacy-check.json)、[原句重试](evidence/implementation/transcript-state/browser/check-line-retry/line-retry-check.json)、[内容与回放 31 项](evidence/implementation/transcript-state/browser/check-content-replay/content-replay-check.json)通过，包含实际导出 HTML 的离线读取。 |
| 构建 | TypeScript、[完整前端构建](evidence/implementation/transcript-state/build.log)、[Linux 原生离线锁定构建](evidence/implementation/transcript-state/native-build.log)和 `git diff --check` 通过。原生内嵌的 11575 个前端文件与当前 dist 相同，见[构建身份](evidence/implementation/transcript-state/identity.json)。本批未改 Rust 产品代码，未重复运行 Rust 测试。 |

最初使用 CSS zoom 的尝试不能代表浏览器缩放，已改用实际标签页缩放。一次远程隐私回归因测试运行器缺少合成主机的回环映射失败，补齐测试映射后通过；没有为此修改产品。失败产物和解释保存在[调查记录](evidence/implementation/transcript-state/investigation/notes.json)。

## Linux 原生窗口与冷启动

独立应用标识、XDG、Xvfb、D-Bus，真实 Tauri / WebKit / GTK；使用合成的 9 条记录和 2 张板，没有连接物理音频或模型。

先在实际窗口发现[原矮窗口布局问题](evidence/implementation/transcript-state/native/interim-run/native-narrow.png)，修改断点后重新构建。[最终 720×480 窗口](evidence/implementation/transcript-state/native/final-narrow-first.png)可以滚动查看状态、原话和重试，底部收音入口可见；详情通过 Enter 收起、再展开，零值耗时和原操作均保留。[恢复至宽窗口](evidence/implementation/transcript-state/native/final-wide.png)后来源完整换行，键盘焦点仍有可见指示。正常退出并[冷启动](evidence/implementation/transcript-state/native/cold-restored.png)，恢复同一会议和各句状态。

[13 项原生证据核对](evidence/implementation/transcript-state/native-proof.json)同时检查原话、两张板内容、窗口几何、详情键盘行为、文件与进程。需要区分两段存储结果：初始合成记录缺少历史、排版和回放信息，第一次原生退出补齐这些信息并更新保存版本；9 条原话及两张板的图内容保持相同，不能称初始文件字节不变。最终构建及之后冷启动没有再次改写文件，冷启动前后 SHA-256 均为 `b8b33b38c210d3cd457bcc63208f5f857cda33b1bf6ece8b6167311311359f0a`。

核对初始文件时过严的字节断言失败，随后冷启动覆盖了上一轮临时进程表和日志；这段缺失未重建为证据。最终窗口截图、AX、操作结果和构建身份仍保留，冷启动有独立日志及进程表。部分截图含 Cua 光标覆盖层，未编辑截图。

## 预览与边界

[预览核对](evidence/implementation/transcript-state/preview-check.json)确认原有 1423 / 1430 进程仍在运行，1430 加载当前 `index-D4dnu15D.js`，与被测产物字节相同。临时原生应用和显示服务已退出，保留的注册 PID 均不再存在。

Windows ARM 请求通道修复见[独立报告](windows-arm-local-model-validation.md)，本批没有增加其真机结论。Windows ARM、用户暂无的 Mac、物理音频、VoiceOver、签名更新及 G4 新用户试用仍待完成；完整重构未关闭。没有提交、推送、安装或发布。
