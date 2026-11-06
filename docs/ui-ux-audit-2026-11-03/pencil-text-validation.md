# 自由编辑文字的保存、模型并发与撤销

2026-11-05，基线 `4f9ab26`，接续 C29/C31/C62。[上一批](concurrent-drawing-validation.md)保护了按下鼠标时的拖动，本批将保护延伸到仍然打开的文字编辑框：停顿、自动保存和模型返回不再把中间草稿提交为完整白板步骤。完成文字编辑后，文字修改和模型增量各记一步。

## 实际缺陷与改动

旧生产构建中，先把 `Initial topic` 改为 `My first draft`，保持编辑框打开，再让模型返回新主题。文件已记录中间文字和模型；继续改字并结束编辑后，又增加了重复历史。第一次全局撤销会退回中间草稿。见[模型返回时的窗口](evidence/implementation/pencil-text/probe/editing-after-result.png)、[对应数据](evidence/implementation/pencil-text/probe/editing-after-result.json)及[旧构建回归失败](evidence/implementation/pencil-text/before/report.json)。

产品只改 `App.tsx`，复用既有 Session 等待回调、编辑批次和自动保存计时器，没有增加依赖、IPC 或存储格式。见[本批产品补丁](evidence/implementation/pencil-text/product.patch)。

- 从 Excalidraw 的文字编辑状态识别一次完整输入。编辑框打开时保留焦点和草稿，等待用户结束；空文字取消也能释放等待中的模型请求。
- 文字及已接收原话在停顿后继续自动保存，编辑中的工作场景可以持久化，但不提前加入回放或全局撤销历史。文本框内的系统撤销仍由文字编辑器处理。
- 没有待提交手势时，不把尚未更新的画面配上新图记录历史。异步画面生成完成后，还需确认其图仍是当前图，才能回写和记录。
- 初始 React 状态与主 Session 共用同一个图，确保上述版本检查也适用于首次打开空白板。

第一版候选的文字检查通过，但旧绘图回归出现三步：完整手势、带旧画面的新图、最终画面。补上图版本检查后，又暴露两个初始空图身份不同、首次自由编辑无法就绪的问题；统一初始图后修复。失败候选与最终通过结果分别保留，见[三步数据](evidence/implementation/pencil-text/intermediate-production/pencil-concurrent/draw.json)、[初始化失败](evidence/implementation/pencil-text/initialization-candidate-production/pencil-text/report.json)和[运行说明](evidence/implementation/pencil-text/run-notes.json)。

## 最终生产构建

| 范围 | 证据 |
| --- | --- |
| 新增文字旅程 | [25 项通过](evidence/implementation/pencil-text/production/pencil-text/report.json)：语义对象改字、自由文字、编辑中保存和模型等待、焦点、系统撤销/重做、Esc 结束、直接切换视图、两次全局撤销/重做、空文字取消与重开；模型建议仍可恢复，用户文字保持。 |
| 绘图与场景 | [18 项模型并发](evidence/implementation/pencil-text/production/pencil-concurrent/report.json)、[10 项手势批次](evidence/implementation/pencil-text/production/pencil-batch/report.json)、[场景往返](evidence/implementation/pencil-text/production/scene-roundtrip/scene-roundtrip-check.json)通过。 |
| 保存与其他入口 | [19 项回放浏览](evidence/implementation/pencil-text/production/replay-view/replay-view-check.json)、[会话隔离](evidence/implementation/pencil-text/production/lifecycle/lifecycle-check.json)、[原话重试](evidence/implementation/pencil-text/production/line-retry/line-retry-check.json)、[64 项导出交接](evidence/implementation/pencil-text/production/export-handoff/export-handoff-check.json)和[58 项启动](evidence/implementation/pencil-text/production/startup/startup-check.json)通过。 |
| 单元与构建 | [45 项 engine、自由场景、保存及快捷键单元](evidence/implementation/pencil-text/node-tests.log)通过；最终 TypeScript、Vite 生产构建和 Linux 原生 `--locked --offline` 构建通过。Rust 未变，本批未重跑 Rust 全量测试。 |

全部九组生产页面回归结束后，测试浏览器和临时 1434 服务均已关闭，见[总记录](evidence/implementation/pencil-text/production-report.json)。新增回归在 [check-pencil-text.cjs](../../scripts/ui/check-pencil-text.cjs)。浏览器使用真实 App/IndexedDB 和受控模型响应；导出交接组模拟原生命令，不代替操作系统文件选择器验收。

系统文本撤销的分组由平台决定：本次 Chromium 留下末尾空格，原生 WebKit 一次撤销回到编辑前文字，二者都能重做恢复。测试不要求它们采用同一种分组。语义文字重生成会改变 `version/versionNonce/updated`，比较恢复属性时只排除这三项；自由文字比较全部字段，没有把内容或几何差异排除。

## 真实 Linux 桌面

最终 Tauri/WebKit 构建运行于独立 XDG、D-Bus 和显示器，使用既有测试会议与模型的独立副本。真实 Qwen3.5-4B-Q4_K_M 在第四张板生成 `Native text seed`，随后提交另一个主题请求，并保持原文字编辑框打开。

为固定事件顺序，只短暂暂停已核实 PID、启动时间、程序路径与父进程的专用 `llama-server`；编辑框写入草稿后即恢复，并由 `finally` 保证恢复。模型响应来自真实本地模型，未替换为模拟回复。这是并发时序检查，不是性能测量。见[操作脚本记录](evidence/implementation/pencil-text/native/check-text-wait.py)及[时间记录](evidence/implementation/pencil-text/native/text-timing.json)；脚本依赖私有测试环境及辅助文件。

1. 模型完成推理后，编辑框仍打开并保留插入点；文件已保存 `Native user draft` 和新原话，模型结果尚未应用，回放步数不变。见[实际窗口](evidence/implementation/pencil-text/native/native-model-waiting.png)和[文件](evidence/implementation/pencil-text/native/during-text-meeting.json)。
2. 原生 Ctrl+Z 回到 `Native text seed`，Ctrl+Shift+Z 恢复 `Native user draft completed`；两次操作均保持模型等待，未增加白板步骤。见[撤销](evidence/implementation/pencil-text/native/native-text-undone.png)、[重做](evidence/implementation/pencil-text/native/native-text-redone.png)。
3. Esc 结束编辑后，真实模型新增 `Concurrent native text`，原话来源为 `Built-in`，此前原话保持，恰好新增两步。见[完成数据](evidence/implementation/pencil-text/native/after-text-meeting.json)。
4. 第一次全局撤销只移除模型增量；第二次恢复编辑前的图；两次重做恢复最终图和文字属性。原先一步加上本次两步及四次撤销/重做，最终共七步。前三张板除重生成的版本元数据外，内容、几何和历史保持。见[最终窗口](evidence/implementation/pencil-text/native/final-first.png)。
5. 正常退出再冷启动，手绘和自由编辑均显示完整内容，没有该应用的采集流，实际模型预热再次完成。浏览和最后退出后文件字节不变。见[手绘](evidence/implementation/pencil-text/native/cold-hand.png)、[自由编辑](evidence/implementation/pencil-text/native/cold-free-ready.png)及[冷启动核对](evidence/implementation/pencil-text/native/cold-proof.json)。

首次 20 秒观察窗口结束时模型仍在加载；确认同一进程继续运行后等待，没有重启或更换模型配置。两次模型进程均在约 95 秒后完成真实预热，日志见[首次运行](evidence/implementation/pencil-text/native/first-run/llama-server.log)和[冷启动](evidence/implementation/pencil-text/native/cold-run/llama-server.log)。加载和预热推理耗时分开保留，不将观察超时当作应用失败。

## 身份、清理与边界

最终二进制 SHA-256 为 `ea6e537ce4b387977d4c98b8dcff1eeef09070e8e371e6ad01407a8fcdb81302`，前端 `index-DwHKnJrG.js`，身份见[记录](evidence/implementation/pencil-text/identity.json)。最终会议 SHA-256 为 `4ec91a7542023816f60bda40c4b026a3ad9ccb3bb25ba6f5026c72e2bf903d19`。原始测试会议不变；两个应用正常退出，40 个登记测试进程全部结束，专用环境无残留，见[清理](evidence/implementation/pencil-text/native/cleanup-proof.json)和[源文件核对](evidence/implementation/pencil-text/source-preserved.json)。

[27 项归档证据核对](evidence/implementation/pencil-text/proof.json)通过；可运行 `python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/pencil-text/verify-evidence.py` 重查已有数据，该脚本不重新执行原生界面。[SHA-256 清单](evidence/implementation/pencil-text/SHA256SUMS)覆盖归档文件。1423/1430 原预览进程保留，1430 已更新且入口字节与最终原生前端一致，见[预览核对](evidence/implementation/pencil-text/preview-proof.json)。没有提交、推送、安装或发布。

全部自由对象与其他编辑状态、此文字框中的原生中日输入法、触摸/触控笔、实体/系统音频、长时间现场会议、Windows ARM/Adreno 与 macOS 实机、G3/G4 仍待完整验收。本批未重跑完整音频/PDF/系统文件保存旅程，也未生成 Windows 安装包；[Windows 本地请求修复](windows-arm-local-model-validation.md)的实机边界保持。
