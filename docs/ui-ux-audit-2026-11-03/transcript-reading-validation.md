# C54/C51 续批：长原话追新、来源定位与离线阅读

2026-11-04，HEAD `4f9ab26`。使用构造的 40 张板、2000 条中英日混合原话，每种窗口尺寸再从输入区追加 3 条记录，验证阅读、保存与实际 HTML 导出；不是现场录音或 60 分钟持续运行测试。

## 修改

低矮窗口的 CSS 会让整个侧栏滚动，而普通窗口由原话列表滚动。此前追新、向上阅读和来源定位只操作列表，640×400 下会失效。现在这些操作共用实际滚动区域，来源定位按可见区域计算位置；侧栏捕获对应区域的滚动，隐藏面板和其他标签页不改变原话跟随状态。

离线页有大量页码时，按钮被 flex 压缩，键盘焦点仍可能落在裁切区域。现在页码保持自身宽度，获得焦点时完整滚入页栏。回放到最终步骤也会显示最后一句；首次打开及切板仍保留阅读位置。

产品改动仅 `App.tsx`、`player/Player.tsx` 和 `player/player.css`，没有更改会议格式、ASR 或原生存储。

## 验证

[旧构建](evidence/implementation/transcript-reading/baseline/transcript-reading-check.json)完整执行 41 项，34 项通过、7 项失败：640×400 的初始追新、向上暂停、回到最新、后续追新和来源定位，离线第 40 个页码可见性，以及回放结束时末句可见性。

[最终构建](evidence/implementation/transcript-reading/final/transcript-reading-check.json)41 项全部通过：

- 1280×800、720×600、640×400 中，滚轮/键盘阅读旧句不被新句打断，未读数依次累积，侧栏标签与窄屏白板/详情切换保留阅读位置，显式追新后继续跟随。
- 每个用例等待实际保存，再确认原有 2000 个记录 ID、追加记录与 40 张板都在；追加记录走“板书模型未配置”失败路径，原话仍保留。
- 从第 20 张板的内容列表定位到第 951 条原话，目标完整出现在当前滚动区域；高亮消失后仍保持阅读位置。
- 实际导出 HTML，在断网的新 Chromium context 中打开；40 张板和 2003 条原话完整，Tab 能到达并完整显示第 40 个页码，晚期回放显示正确的历史前缀，退出后末句进入视野，外层文档不被滚动。没有 HTTP 请求或脚本错误。

[补充播放器检查](evidence/implementation/transcript-reading/player-reading-probe.json)另通过 4 项：首次打开从原话开头阅读，切板前后旧阅读位置均为 124037，自动播放自然结束后末句可见，且全程离线无脚本错误。代码对首次打开/切板和回放位置变化作了区分。

[七格式及离线键盘 92 项](evidence/implementation/transcript-reading/exports/export-check.json)、[应用回放 31 项](evidence/implementation/transcript-reading/content-replay/content-replay-check.json)、[工作台回归](evidence/implementation/transcript-reading/workbench/workbench-check.json)均通过；[7 项 Node](evidence/implementation/transcript-reading/node-tests.txt)、[类型检查](evidence/implementation/transcript-reading/typecheck.txt)、[主应用和最终播放器构建](evidence/implementation/transcript-reading/build.txt)通过。未重跑全量 Node/Rust 或原生构建。

早期探针未等待自动保存便更换 fixture，后续窗口继续向旧会议追加，出现 2009 条记录；这些计数失败不作为数据丢失证据。正式脚本等待“已保存”，每次重载确认恰好 2000 条初始记录。最初的 40 项检查也未检查末句是否可见，截图复核后补上该项，以上 41 项前后对照包含此修正。

已查看[窄窗追新截图](evidence/implementation/transcript-reading/final/reading-640.png)和[离线末句截图](evidence/implementation/transcript-reading/final/offline-long-meeting.png)。[最终实际 HTML](evidence/implementation/transcript-reading/final/long-meeting.html)、修复前 HTML、脚本、补丁与哈希均已归档。

## 构建与边界

入口 `index-DVXS0_a6.js`，SHA-256 `b5207fffa0e9e57fa77eb44047393997eb4530e9c40950caafdfde48dc8850fe`；播放器 SHA-256 `415581f40b73c517a2e0dac6004c75f38a05b920c83764a2405995c8da60df8f`。已更新 1423/1430 本地预览，并核对所提供的主入口/播放器字节，见[身份记录](evidence/implementation/transcript-reading/identity.json)。服务保持原 PID，1438 临时预览与测试浏览器已关闭。

本批是三个固定 CSS 窗口尺寸下的 Chromium 验证。连续缩放/调整高度时的阅读锚点、Mac/Safari、真实输入法、VoiceOver、现场长会及 G4 用户试用仍需复核。C54/C51 的完整变体及整体目标未关闭；未提交、推送或发布。
