# 原生七格式导出与会议库布局续验

2026-11-04，Linux Tauri/WebKit，独立 Xvfb 桌面和数据目录。本批修复两个实际界面问题：PDF 导出脚本被识别为 PDF 文件而无法加载，以及会议库标题、时间覆盖操作按钮。七种格式均通过实际系统保存窗口落盘，文件内容检查 22 项通过；这不代表 C01–C68 或 G2–G4 全部完成。

## 修复及复现

原生 PDF 导出在保存框出现前报错：`'application/pdf' is not a valid JavaScript MIME type.` 见 [失败窗口](evidence/implementation/native-formats/pdf-picker.png) 和 [失败状态](evidence/implementation/native-formats/pdf-result.json)。原构建的 PDF 模块在前 1024 字节内含有 PDF 文件头；锁定依赖中的 `infer 0.22.0` 会在此范围搜索签名，Tauri 按内容判断 MIME 后将 JavaScript 当作 PDF 返回。[pdf.ts](../../src/board/pdf.ts) 改为写入四个字节再接剩余文件头，保持生成 PDF 的字节内容不变，避免脚本源码出现完整签名。

[MIME 对照](evidence/implementation/native-formats/mime-before.json) 使用归档的 [最小签名样例](evidence/implementation/native-formats/pdf-signature-fixture.js)，由实际锁定版本的 Tauri MIME 函数判为 PDF；它是合成复现，不是原错误构建的完整脚本。最终构建全部 197 个 JS/MJS 文件通过同一函数检查，见 [最终结果](evidence/implementation/native-formats/mime-final.json)。没有修改 WebView 安全策略或依赖版本。

会议库的自动 grid 列按长文本撑开：浏览器复现中，标题右边界约 672.64px，所属按钮右边界只有 503.50px，覆盖了从约 511px 开始的操作区。[style.css](../../src/style.css) 使用 `minmax(0, 1fr)` 限定文本列，给标题区域合理的 flex 基准，并允许操作区和删除提示换行。见 [原生问题截图](evidence/implementation/native-formats/library-layout-finding.png)、[浏览器修复前几何](evidence/implementation/native-formats/library-layout-before.json)、[六组修复后几何](evidence/implementation/native-formats/library-layout-after.json) 与 [最终原生截图](evidence/implementation/native-formats/library-layout-native-final.png)。六组为中英文各 1280/720/640px，含长标题、40 板/2000 句/未完成数量及删除确认；640px 仅为浏览器检查，最终原生复核为 1280px。原有会议库交互回归也 [通过](evidence/implementation/native-formats/library-regression.log)。

## 实际保存及内容

样本复用已有导出验收的丰富内容，见 [输入](evidence/implementation/native-formats/fixture-input.json) 与 [原生记录](evidence/implementation/native-formats/fixture-record.json)：共三页，分别为语义节点/连线与自由对象混合页、仅自由对象页、空页；含中英文、底图、PNG/SVG 图片、矩形、笔画、自由文字、摘要及两位说话人的原话。五份资源按真实内容哈希写入隔离原生会议目录。当前停在空页时，四种当前板书格式禁用并说明原因，整场会议格式仍可用，见 [空页菜单](evidence/implementation/native-formats/empty-board-menu.png)。

| 原生输出 | 内容核对 |
| --- | --- |
| [Markdown](evidence/implementation/native-formats/exports/meeting.md) | 三页标签、中英文节点/连线、摘要及两条原话；明确提示不包含自由绘图等视觉内容 |
| [PDF](evidence/implementation/native-formats/exports/meeting.pdf) | 三张 A4 横向页：两张非空板书及摘要/原话；底图、自由对象、两类图片和文字渲染通过 |
| [HTML](evidence/implementation/native-formats/exports/meeting.html) | 三页与原始图片字节完整，新离线浏览器中显示摘要和原话，Home/End 键恢复初始/最终状态，无 HTTP 请求或脚本错误 |
| [PNG](evidence/implementation/native-formats/exports/board.png) | 2112×1255，底图、矩形和 PNG/SVG 图片颜色通过真实解码核对 |
| [SVG](evidence/implementation/native-formats/exports/board.svg) | 1056×627，底图原始字节及对象颜色保留 |
| [动画 SVG](evidence/implementation/native-formats/exports/board.anim.svg) | 静态底图/对象/图片保留，等待 14 秒后语义笔画实际增加 |
| [Excalidraw](evidence/implementation/native-formats/exports/board.excalidraw) | 自由图形/文字/笔画/图片对象、原始 PNG/SVG 字节、语义图及可见底图对象保留 |

七次保存都用实际键盘输入完整目标路径，不使用文件名 `set_value`。每次明确点击 Save 前，记录七次目标不存在的采样及保存框截图；点击后核对文件时间、哈希、Saved 路径/Open file 状态和 X11 客户端窗口，见 [保存核对](evidence/implementation/native-formats/native-save-check.json)。原始 `*-input.json`、`*-absence.json`、`*-before-save.png`、`*-save.json`、`*-saved.json` 保留在同目录。PDF 的 Open file 已实际打开系统浏览器，见 [窗口](evidence/implementation/native-formats/fixed-pdf-open.png)；其他格式没有逐个验证默认查看器。

[22 项内容报告](evidence/implementation/native-formats/native-file-content-check.json) 来自实际解码和浏览器运行。[check-export-files.cjs](evidence/implementation/native-formats/check-export-files.cjs) 接收证据目录参数，需要现有 Playwright、`/usr/bin/chromium`、`pdfinfo` 和 `pdftoppm`；它会运行浏览器并更新报告、截图及 PDF 栅格文件，不是只读档案检查，也不会重放原生 GUI。复跑应先复制证据目录，保留原档案与 manifest。

PDF 修复构建另通过 [68 项生产浏览器导出回归](evidence/implementation/native-formats/browser-regression/export-check.json)。现有 [PDF 单测](evidence/implementation/native-formats/pdf-unit-test.log) 通过，检查文件头、尾、页数、xref 偏移及 Unicode 标题；本批两次生产构建（含类型检查）及两次私有原生构建通过。最终仅增加会议库 CSS 修复后，复核六组布局、会议库交互、197 个脚本 MIME 及实际原生窗口，没有重复七格式全套。

## 构建身份、归档和边界

| 阶段 | 原生 binary SHA-256 | 前端入口与实际范围 |
| --- | --- | --- |
| 修复前 | `930aa63794e6148fab013964970be6d12187b8c277a92559775fbdfad7c8fb89` | `index-Dcpz5Vs4.js`；Markdown 保存及 PDF 失败复现 |
| PDF 修复 | `eb695adf6c65312fa1c15676055cd6d491ea1da9160d3fed1c0789e928c11c82` | `index-DBmB4Tmr.js`；其余六格式保存、PDF 打开、内容检查和 68 项浏览器回归 |
| 会议库 CSS 修复 | `01f9aedfc009dac29ccf91416acfc313fc85382d341998d612d9b94b13e167c7` | `index-N8K0hlyP.js`；最终原生会议库布局复核 |

最终 [189 个源码/配置哈希](evidence/implementation/native-formats/source-hashes.json) 与当前工作树一致；阶段差异分别只有 `src/board/pdf.ts` 和 `src/style.css`，见 [最终核对](evidence/implementation/native-formats/final-check.json)。[manifest](evidence/implementation/native-formats/manifest.json) 记录 217 份文件的大小和 SHA-256，包括七个原生输出、截图、原始观察、构建日志及检查结果。完整临时工作目录保留在 `/home/hsiangnianian/.cache/boardcast-g2-formats-hdsqoi8m`；浏览器回归完整输出留在其 `browser-regression` 子目录，仓库仅归档该批报告、离线截图及输出清单。

三个原生阶段均通过应用 Quit 正常退出，随后关闭各自的驱动、窗口管理器和显示服务；PDF 查看器也已关闭。[清理记录](evidence/implementation/native-formats/cleanup.json) 显示本批进程及 X90 socket 已消失，没有遗留使用该私有数据目录的进程。没有提交 Git、安装或部署。

未重复完整 Node/Rust 套件；本批七格式是一个丰富样本，不覆盖所有大图、长会、主题、默认查看器及错误组合。此前 HTML 取消、EACCES 重试、打开目录和自动化提前提交的边界继续见 [原生 HTML 报告](native-export-validation.md)。ENOSPC、附件准备阶段崩溃、Mac 真实音频/VoiceOver/签名安装，以及 G4 用户试用仍待验收。
