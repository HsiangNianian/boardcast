# C54 续批：窗口变化与浏览器缩放中的原话阅读

2026-11-04，HEAD `4f9ab26`。接续[长会阅读验证](transcript-reading-validation.md)，本批只修改 `src/App.tsx` 的阅读位置管理，沿用 40 张板、2000 条构造原话和每会 3 条实际输入记录。

## 修改与结果

窗口变矮后，滚动区域会从原话列表切换到整个侧栏。旧实现没有转移阅读位置；浏览器缩放还会改变段落换行，原来的像素偏移不能代表同一句话。

现在暂停追新时记住可见区域中间的元素及其内部相对位置，窗口变化后在实际滚动区域恢复。查找使用列表子元素的二分定位；原有追新状态会直接恢复到末端。隐藏原话区时延后恢复，重新展开后执行；换会清除旧锚点，焦点保持原位。普通标签切换仍使用原来的滚动位置，只有发生尺寸变化才执行这次恢复。

## 验证

同一份最终脚本对修复前后的生产构建执行：

- [尺寸变化旧构建](evidence/implementation/transcript-resize/baseline/reading/transcript-reading-check.json)55 项中 50 项通过、5 项失败；[最终构建](evidence/implementation/transcript-resize/final/reading/transcript-reading-check.json)55 项全部通过。覆盖 1280×800、900×600、720×600、720×400、640×600、640×400 的连续切换，历史来源定位、键盘焦点、追新、隐藏摘要/白板后的阅读恢复，以及原有保存和实际离线 HTML 检查。
- [真实缩放旧构建](evidence/implementation/transcript-resize/baseline/transcript-zoom-check.json)26 项中 17 项通过、9 项失败；[最终构建](evidence/implementation/transcript-resize/final/transcript-zoom-check.json)26 项全部通过。临时扩展调用 `chrome.tabs.setZoom`，两种阅读状态均按 125% → 150% → 200% → 150% → 100% 连续变化，并核对 DPR、CSS 视口和 `visualViewport.scale === 1`。基线报告另有一条汇总失败指向上面的阅读检查，不重复计数。
- 缩放检查使用真实滚轮改变历史阅读位置，再检查同一条原话仍可见、暂停状态和键盘焦点未丢失。追新检查要求末句末端可见且仍在产品既有的 32 px 跟随范围内；回到 100% 时曾测得底部偏移 5 px，末句末端仍在可见区域，未为此增加滚动补偿。
- [应用回放 31 项](evidence/implementation/transcript-resize/content-replay/content-replay-check.json)、[工作台回归](evidence/implementation/transcript-resize/workbench/workbench-check.json)、[类型检查](evidence/implementation/transcript-resize/typecheck.txt)及[应用构建](evidence/implementation/transcript-resize/build.txt)通过。没有重跑全量 Node/Rust 或原生构建。

已查看 200% 下的[历史阅读](evidence/implementation/transcript-resize/final/reading-200-percent.png)和[追新末句](evidence/implementation/transcript-resize/final/following-200-percent.png)截图。实际导出的[长会 HTML](evidence/implementation/transcript-resize/final/reading/long-meeting.html)、前后检查、脚本及增量补丁共 34 个产物、2,806,678 字节，另有[哈希清单](evidence/implementation/transcript-resize/manifest.json)。

## 预览与边界

1430 预览入口已更新为 `index-Dfzo6bWQ.js`，并与被测构建逐字节核对；播放器保持上一批版本。1423/1430 均通过新浏览器 context 的中文启动检查，无脚本错误或 HTTP 失败，见[预览检查](evidence/implementation/transcript-resize/preview-check.json)与[构建身份](evidence/implementation/transcript-resize/identity.json)。原有预览进程保留，临时浏览器、扩展和 1438 测试服务已关闭。

这是 Linux Chromium 的构造长会验证；Safari/macOS 原生缩放、真实输入法、VoiceOver、现场长会和 G4 用户试用仍待验收。会议数据格式、ASR、原生存储和播放器未改动；没有提交、推送或发布。C54 的完整平台范围及整体重构目标仍未关闭。
