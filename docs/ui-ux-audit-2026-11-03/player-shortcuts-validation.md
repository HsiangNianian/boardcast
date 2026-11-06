# C51/C62 续批：离线回放页的键盘操作

2026-11-04，HEAD `4f9ab26`。本批处理导出 HTML 内的播放器；整体 C01–C68 / G1–G4 验收继续。

## 修改

原先播放器在窗口级别无条件接管空格与左右键，导致进度滑杆一次移动两步，倍速、退出和翻页按钮的空格激活被播放开关抢走。退出按钮提示 Esc，但播放器没有处理该键；无回放历史的空页也拦截空格。

现在滑杆保留原生方向键操作，空格由当前按钮处理；没有控件焦点时仍可用空格播放/暂停、左右键逐步回放。按住空格只切换一次，修饰键组合和 composition 标记继续交给浏览器。Esc 与退出按钮共用停止并恢复最终板书的操作，滑杆聚焦时也能退出。没有历史的页面不拦截播放快捷键。翻页按钮沿用应用 `PageStrip` 的 `aria-current="page"` 标记。

产品源码仅修改 `src/player/Player.tsx`。原有回放数据、布局、七格式导出和原生保存逻辑保持；扩展既有 `check-exports.cjs` 的实际离线 HTML 检查，没有新增依赖。

## 证据

从生产 App 实际导出七种格式；HTML 在新 Chromium context 中用 `file://` 打开，设置离线并拦截全部 HTTP 请求。使用真实浏览器按键检查滑杆、按钮、重复空格、翻页和 Esc；修饰键与 IME composition 隔离另用合成 KeyboardEvent 覆盖。

- [旧播放器](evidence/implementation/player-shortcuts/baseline/export-check.json)：完整执行 92 项，75 项通过、17 项失败；没有页面脚本错误或网络请求。失败涵盖滑杆双步、按钮空格、Esc、重复/修饰/合成 IME 空格，以及翻页和空页行为。
- [新播放器](evidence/implementation/player-shortcuts/final/export-check.json)：92 项全部通过。包含原有七格式内容/图片/字体/动画/布局回归与新增键盘检查；导出的会议数据在操作前后完全一致，离线请求、请求失败和页面脚本错误均为零。
- [7 项定向 Node 测试](evidence/implementation/player-shortcuts/node-tests.txt)、[类型检查](evidence/implementation/player-shortcuts/typecheck.txt)、[播放器及主应用生产构建](evidence/implementation/player-shortcuts/build.txt)通过；未扩大到全量 Node/Rust 或原生构建。

本批保留[修复前 HTML](evidence/implementation/player-shortcuts/baseline/replay.html)与[修复后 HTML](evidence/implementation/player-shortcuts/final/replay.html)，以及[窄窗截图](evidence/implementation/player-shortcuts/final/offline-compact.png)。测试脚本、补丁及文件哈希均在同一归档目录，可重跑全部格式检查。

播放器 SHA-256 为 `8c03014521e550499c6dd26032e389e436f37a9e94eb8c7331619a422fe0bd24`。验证后更新了开发/生产预览使用的播放器产物，1423/1430 返回的脚本均与本批验证文件逐字节一致；主应用构建入口未变化，服务保持原 PID，见[身份记录](evidence/implementation/player-shortcuts/identity.json)。

本批测试浏览器和临时 1438 预览已关闭。Safari/Mac 真机、实际输入法、VoiceOver、长时间会议与 G4 用户试用尚未执行；合成 composition 与 `aria-current` 检查不代表原生输入法或读屏验收。C51/C62 的完整变体与整体目标仍保留，未提交、推送或发布。
