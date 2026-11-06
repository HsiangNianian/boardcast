# Linux 资料、连续收音与自由编辑联动验收

2026-11-05，基线 `4f9ab26`。本批接续 C05/C09/C29/C31/C32/C51，使用真实 Tauri、SenseVoice/VAD 和本地 Qwen 4B，把资料导入、连续收音、自由编辑、收尾、系统保存和冷启动接起来。输入来自已有 AMI 音频，经专用虚拟麦克风播放；没有使用实体麦克风或真人现场讲话。

完整流程发现一次拖动被拆成两个回放步骤；最终图形没有丢失。修复仅调整 `App.tsx` 的指针结束回调：先结束按下状态，再使用已有计时器延后提交，让 Excalidraw 的最后一次 `onChange` 完成。其他切页、保存、退出的同步提交边界保持原状，没有增加依赖。[本批产品及测试补丁](evidence/implementation/live-meeting/gesture-product-and-test.patch)可单独查看。

## 完整流程的实际结果

| 环节 | 证据与结论 |
| --- | --- |
| 真实资料导入 | 系统文件选择器打开两页 PDF，确认添加后生成两张资料板。两页底图文件哈希保持；原件移开后仍能恢复。见[导入窗口](evidence/implementation/live-meeting/materials-imported.png)。 |
| 连续收音 | 在准备页明确选择专用虚拟麦克风。实际 CPAL 输入流在切换手绘/自由编辑、绘图、翻页和打开收尾对话框期间保持同一 stream/source；没有隐式开启采集。 |
| 真实模型增量 | 完整播放约 33.72 秒 AMI，SenseVoice 产生九条原话，Qwen 实际更新白板。第一张资料板上手画的矩形经历后续生成仍保持最终字段。见[绘图窗口](evidence/implementation/live-meeting/live-drawn.png)和[模型日志](evidence/implementation/live-meeting/live-run/llama-server.log)。 |
| 翻页 | 第一次在生成期间点击第二页没有导航成功；生成完成后重新观察并点击，仍在连续收音时切到第二页。此处不宣称支持生成中切页。见[第二页自由编辑](evidence/implementation/live-meeting/live-second-free-confirmed.png)。 |
| 结束与尾句 | 打开收尾确认后继续播放尾段，在播放未结束时确认 End and review。保存新增两条不同 ID 的尾句，最后一条 `soft=false`、`waitMs=0`；均归属第二页，本地生成完成后才进入复盘。原九条原话不变。见[时间记录](evidence/implementation/live-meeting/tail-timing.json)及[复盘窗口](evidence/implementation/live-meeting/review-finished.png)：11 条原话、两张板、零未完成项。 |
| 系统 HTML 保存 | 输入路径后连续四次确认文件不存在，明确点击系统 Save 时才产生文件。原会议字节不变。见[实际 HTML](evidence/implementation/live-meeting/exports/live-meeting.html)及[保存时序](evidence/implementation/live-meeting/saved-html.json)。 |
| 断网回放 | 新离线上下文完成 [17 项检查](evidence/implementation/live-meeting/offline-proof.json)：原话、两页图、底图字节、自由图形最终字段和步骤显示保持；没有 HTTP 请求或脚本错误。[第一页](evidence/implementation/live-meeting/offline-page-one.png)、[第二页](evidence/implementation/live-meeting/offline-page-two.png)均实际检查。 |
| 冷启动 | 移开专用 PDF 副本后，两页资料和矩形仍可见；不自动收音，真实预热完成。浏览及退出后会议 SHA-256 保持 `a2d682791992aa1069771d4c8ba71064f3d240a0b477fe123c73efdb7feaf3cc`。见[文件核对](evidence/implementation/live-meeting/cold-proof.json)。 |

上述流程复用上一批二进制 `a137c490…`，前端 `index-CMcenoVY.js`，身份见[原运行记录](evidence/implementation/live-meeting/identity.json)。[28 项数据与生命周期核对](evidence/implementation/live-meeting/proof.json)明确保留一个批次缺陷，没有将其标成全量通过：矩形在两个相隔 26 ms 的场景步骤中从倒数第二个位置变成最终位置。

## 拖动批次修复与最终构建

检查已安装 Excalidraw 的回调顺序，确认 `onPointerUp` 先于自由笔迹末点和最终 `onChange`。新增检查在旧生产构建中实际失败：[Draw 一次交互产生多个步骤](evidence/implementation/live-meeting/gesture-before/report.json)。修复后的结果如下：

- [10 项新增生产检查](evidence/implementation/live-meeting/gesture-production/pencil-batch/report.json)通过：矩形和自由笔迹各只增加一个完整步骤，全局一次撤销/重做以及重开均保持。
- [既有场景往返](evidence/implementation/live-meeting/gesture-production/scene-roundtrip/scene-roundtrip-check.json)、[19 项回放浏览](evidence/implementation/live-meeting/gesture-production/replay-view/replay-view-check.json)、[10 项本地请求通道](evidence/implementation/live-meeting/gesture-production/local-transport/report.json)通过。最后一组重新确认网页 fetch 被阻断时仍经原生请求，包含 180 秒预热超时、取消及重新启动。
- TypeScript、生产构建、Linux 原生 `--locked --offline` 构建通过；本次仅修改前端回调，没有重复完整 Node/Rust 测试。此前 Windows ARM 请求修复及其完整测试仍见[专门报告](windows-arm-local-model-validation.md)。
- 最终原生构建在独立数据副本中实际画矩形和笔迹，分别验证只记一个最终步骤、一次撤销/重做；再键入原话，由真实 Qwen 新增主题，两种图形的全部字段保持。见[最终窗口](evidence/implementation/live-meeting/gesture-native/after-model.png)。
- 最终构建冷启动恢复同一会议，文件字节不变、两图形字段一致、没有采集流，日志也记录第二次实际预热完成。见[冷启动窗口](evidence/implementation/live-meeting/gesture-native/cold-restored.png)和 [21 项最终证据核对](evidence/implementation/live-meeting/gesture-proof.json)。冷启动截图摄于加载期间，预热完成另由模型日志证明。

最终二进制 SHA-256 为 `d88078351bdba79e90e037687e0aedb048441e125d83d49a6b1437540d9b1aac`，前端 `index-Cz2AfyXG.js`，身份见[记录](evidence/implementation/live-meeting/gesture-native/identity.json)。完整音频/PDF/系统 HTML 流程没有在这次指针回调修复后重复；最终构建的原生范围为绘图批次、真实模型增量和冷恢复。

## 环境、清理与边界

首次尝试额外 user/network namespace 时，GTK 文件选择器因 glycin SVG 图标加载失败而中止。独立 GdkPixbuf 对照在相同 namespace 中复现、正常环境成功，见[对照结果](evidence/implementation/live-meeting/icon-loader-comparison.json)。随后使用正常宿主网络、独立 XDG/D-Bus/显示器；没有关闭 GTK 自身沙箱。这批原生运行不标为断网，断网结论只针对实际导出的 HTML。失败尝试、首次无效翻页及测试初稿的校正均保存在[运行说明](evidence/implementation/live-meeting/run-notes.json)。

两阶段共 66 个登记测试进程已结束，另按专用 XDG 环境扫描无残留。四次成功验收的应用正常退出；最早 namespace 尝试为异常退出。两个专用音频模块移除，宿主默认输入/输出保持。1423/1430 原预览进程保留，1430 已更新且入口字节与最终原生前端相同，见[预览核对](evidence/implementation/live-meeting/updated-preview-proof.json)。没有提交、推送、安装或发布。

Windows ARM/Adreno 真机复测、用户暂无设备的 macOS、实体/系统/双路音频、热插拔、触控笔、长时间现场会议及 G3/G4 仍保留。本批不扩大为识别准确率、全部自由对象类型或生成中切页的验收结论。
