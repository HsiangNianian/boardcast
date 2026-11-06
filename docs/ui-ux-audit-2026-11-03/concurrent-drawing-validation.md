# 模型返回与自由绘图并发验收

2026-11-05，基线 `4f9ab26`，接续 C29/C31。上一批修正指针结束时过早提交；本批进一步复现模型在鼠标仍按下时更新画布，导致后半段拖动丢失的问题。最终生产页面和真实 Linux Tauri 均验证：原话继续自动保存，模型结果等当前手工编辑完成后应用，完整手势和模型增量各记一步。

## 复现与修复

旧生产构建中，请求绘制 300×200 的矩形，模型在拖动中途返回后，第一次只保存了 90×60，第一次全局撤销也没有改变图或形状。初稿只比较保存图形与历史快照，未发现它们一起被截短；增加实际拖动终点断言后，再次运行保存为 9×6 并明确失败。两个运行的尺寸不同，都是尚未完成的拖动。见[第一次数据](evidence/implementation/concurrent-drawing/before/rectangle.json)、[终点检查失败](evidence/implementation/concurrent-drawing/before-endpoint/report.json)及[对应数据](evidence/implementation/concurrent-drawing/before-endpoint/rectangle.json)。

产品只改 `App.tsx` 和 `engine/session.ts`，没有新增依赖、存储格式或原生命令：[本批产品补丁](evidence/implementation/concurrent-drawing/product.patch)。

- Session 在推理返回后、应用操作前等待直接编辑完成，然后使用当前图应用结果。等待前后均检查会话代次，取消会中止等待，旧结果不能写进新会议；主 Session 与后台页面纠错共用该边界。Eval 等未设置此回调的调用方保留原流程。
- 鼠标按下时，草稿通知和异步场景生成不替换当前画布；自动保存继续保存已接收的原话和当前场景，但不把半次手势提交到撤销或回放。明确保存、切换等原有提交边界仍保留。
- 第一版候选已保全图形，却为一次手势加一次模型结果记录了三步。移除模型回调中重复提交后，最终要求恰好两步；[中间结果](evidence/implementation/concurrent-drawing/intermediate-step-count.json)未被当作最终通过。

## 最终构建验证

| 范围 | 结果 |
| --- | --- |
| Session、保存与场景单元 | [52 项通过](evidence/implementation/concurrent-drawing/final-node.log)，包含新增的编辑等待、使用最新图及取消后的迟到结果隔离。 |
| 其他 Session 调用 | phonetic、pointing、revision、attribute 共 [21 项通过](evidence/implementation/concurrent-drawing/session-callers-node.log)，Eval 调用点另经源码核对。 |
| 并发生产页面 | [18 项通过](evidence/implementation/concurrent-drawing/production/pencil-concurrent/report.json)：矩形和自由笔迹均覆盖按下期间保存原话、模型等待、完整终点、两步历史、两次撤销/重做和重开。使用实际 App、IndexedDB 与受控 HTTP 响应。 |
| 既有生产回归 | [10 项手势批次](evidence/implementation/concurrent-drawing/production/pencil-batch/report.json)、[19 项回放浏览](evidence/implementation/concurrent-drawing/production/replay-view/replay-view-check.json)、[场景往返](evidence/implementation/concurrent-drawing/production/scene-roundtrip/scene-roundtrip-check.json)、[会话隔离](evidence/implementation/concurrent-drawing/production/lifecycle/lifecycle-check.json)、[原话重试](evidence/implementation/concurrent-drawing/production/line-retry/line-retry-check.json)均通过。六组完成后关闭测试浏览器及临时 1434 服务。 |
| 类型与构建 | TypeScript、Vite 生产构建、Linux 原生 `--locked --offline` 构建通过。Rust 未变，本批未重复 Rust 全量测试。 |

新增回归在 [scripts/ui/check-pencil-concurrent.cjs](../../scripts/ui/check-pencil-concurrent.cjs)，Session 测试在 [sessionLifecycle.test.ts](../../src/engine/sessionLifecycle.test.ts)。最终二进制 SHA-256 为 `0c6c66fc446207af1c93a9f9dbcd1b8ae175db5056149251f4e6c4c201516757`，前端为 `index-CjjnrnF5.js`；[构建身份](evidence/implementation/concurrent-drawing/identity.json)包含两个产品文件哈希。

## 真实 Linux 并发、撤销与冷启动

独立 XDG、D-Bus 和显示器中运行最终 Tauri/WebKit 二进制，使用已有测试会议和 Qwen3.5-4B-Q4_K_M 的独立副本。本批原话通过键盘输入，没有开启麦克风。

为保证真实推理恰在拖动中途返回，先核实专用 `llama-server` 的 PID、启动时间、程序路径和父进程，再仅暂停该测试进程，在原生鼠标拖动开始两秒后恢复，并在 `finally` 再次保证恢复。模型响应来自真实本地模型；此方法验证事件顺序，不用来衡量正常性能。见[操作脚本记录](evidence/implementation/concurrent-drawing/native/run-concurrent.py)和[时间记录](evidence/implementation/concurrent-drawing/native/concurrent-timing.json)。操作脚本依赖私有测试环境及辅助文件，不是单独可运行的环境安装器。

1. 原生窗口新建第三张空白板，键入 `Add a new topic called Concurrent native completion.`，在自由编辑中从 `(650,300)` 拖到 `(950,500)`，缩放为 100%。模型完成时间和会议文件快照时间均早于鼠标拖动结束。
2. 模型已返回但鼠标仍按下时，[会议文件](evidence/implementation/concurrent-drawing/native/during-concurrent-meeting.json)已保存新原话，生成耗时尚未结算，图和回放步骤保持原状。自动保存的工作场景可以包含中间形状，它没有被记为完成的回放步骤。
3. 松开后矩形完整保留为 300×200，恰好新增两步；两个步骤中矩形全部字段均与最终图形一致。真实模型新增主题，来源为 `Built-in`，此前所有原话保持。见[结果文件](evidence/implementation/concurrent-drawing/native/after-concurrent-meeting.json)与[窗口](evidence/implementation/concurrent-drawing/native/after-concurrent.png)。
4. 第一次全局撤销只去掉模型增量，完整矩形保留；第二次撤销移除整次手势；两次重做恢复完全相同的图和矩形。撤销/重做另记录四步，因此此时回放共六步。见[恢复窗口](evidence/implementation/concurrent-drawing/native/final-first.png)及[数据](evidence/implementation/concurrent-drawing/native/redone-meeting.json)。
5. 正常退出再冷启动，手绘和自由编辑均可见完整内容，没有采集流，实际预热再次完成。见[手绘窗口](evidence/implementation/concurrent-drawing/native/cold-restored.png)、[自由编辑窗口](evidence/implementation/concurrent-drawing/native/cold-free-ready.png)。浏览及最后退出后文件字节不变，SHA-256 为 `9756fcd3a0ebe1c3eacaaf0585510b03575716c42f0463de2fbe1659aa3a1dcc`。

[23 项归档证据核对](evidence/implementation/concurrent-drawing/proof.json)已通过，可运行 `python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/concurrent-drawing/verify-evidence.py` 重查归档数据；这不等于重新执行原生界面。[SHA-256 清单](evidence/implementation/concurrent-drawing/SHA256SUMS)覆盖归档文件。两个应用均正常退出，39 个登记测试进程全部结束，专用环境扫描无残留。原始测试会议哈希保持，见[清理记录](evidence/implementation/concurrent-drawing/native/cleanup-proof.json)和[源文件核对](evidence/implementation/concurrent-drawing/source-preserved.json)。

1423/1430 原预览进程保留，1430 入口字节与最终构建一致，见[预览核对](evidence/implementation/concurrent-drawing/preview-proof.json)。没有提交、推送、安装或发布。

## 剩余边界

本批不代表全部 Excalidraw 对象、文字编辑状态、触摸/触控笔、实体或系统音频、长时间会议均已验收。上一批完整音频/PDF/系统 HTML 旅程未在本次构建重跑。Windows ARM/Adreno 原机复测、新 Windows 安装包和用户暂无设备的 macOS 仍待完成；Windows 本地请求与预热修复见[独立报告](windows-arm-local-model-validation.md)。总方案 G3/G4 和其余覆盖项继续保留。
