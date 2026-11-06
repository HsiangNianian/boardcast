# C27 多列竖排、表格与公式标注

2026-11-05，基线 `4f9ab26`。修复多列竖排被横向拼字、同一高度点击落到邻列，以及独立公式编号没有进入命中行的问题。跨列的“会议”现在保留两个准确字框，手绘圈分别围住两个字，不穿过中间的原文。识别出的表格继续整块标注，密集的单字中文格子也归属同一张表。

改动只涉及共享的版面分析、词句命中和手绘圈生成，沿用现有 rows、spans、历史记录与导出路径，不增加持久化字段或依赖。逐字竖排按字框聚列，再使用 PDF 文字流确定列序；横向相接的字继续走普通拼行，图表文字继续进入原有图表归属处理。原文兼容字符保留，规范化只用于比较。见[本批补丁](evidence/implementation/pdf-structure/changes.patch)。

| 验证范围 | 结果与证据 |
| --- | --- |
| 旧版对照 | 初版真实 PDF 检查 [52 项有 10 项不符](evidence/implementation/pdf-structure/before/pdf-structure-check.json)，覆盖竖排词句和公式编号，未捕获脚本错误为零。原始 PDF 与当时检查器均已保存。 |
| 多列阅读与命中 | 扩充后的 [83 项生产浏览器检查全部通过](evidence/implementation/pdf-structure/final/pdf-structure-check.json)。真实 PDF 由 Chromium 生成，点击位置来自独立 HTML 字框；“保存”两字不混入邻列，“会/议”从任一端点击得到相同词，句界停在本句。 |
| 公式与表格 | `mc2` 保留上标字符，点击独立编号得到 `7`，整句包含 `(7)`。两侧英文格子与密集中文格子均选择同一整表；工具条不提供不可用的词级操作，表区包括表注和单元格，不含下方正文与公式。 |
| 五种标记与历史 | 对跨列“会议”实际添加高亮、圈、下划线、删除线和星标，均保留两个字框；撤销/重做只改变一次标记。[手绘截图](evidence/implementation/pdf-structure/final/hand-marks.png)中两个圈没有跨列连线。 |
| 自由编辑与保存 | [自由编辑截图](evidence/implementation/pdf-structure/final/free-marks.png)及实际 [Excalidraw 文件](evidence/implementation/pdf-structure/final/structure.excalidraw)核对两个圈的中心位置。刷新及重新导入后，原文行序、字框和全部标记逐字段一致。 |
| 实际产物 | 下载 SVG、动画 SVG、整场 PDF、Excalidraw 与 HTML。两种 SVG 包含与手绘界面完全相同的独立圈路径；实际 PDF 第 1 页经 Poppler 栅格化并[目视核对](evidence/implementation/pdf-structure/final/exported-pdf-1.png)。 |
| 离线回放 | [实际 HTML](evidence/implementation/pdf-structure/final/structure-replay.html)在新建断网 context 中播放至最后一步；竖排页保留两个圈，第二页保留独立的整表圈和公式编号高亮，graph 完整一致且无 HTTP 请求。见[第二页截图](evidence/implementation/pdf-structure/final/offline-table-formula.png)。 |
| 回归及构建 | 最终文字方向 52 项、英文原文 59 项、中文 27 项、七格式导出 92 项通过；[完整 310 项 Node/Eval](evidence/implementation/pdf-structure/node-tests-complete.log)及[生产构建](evidence/implementation/pdf-structure/build-complete.log)通过，包含 5 个新增逻辑测试和加强的公式编号断言。中文回归未再次运行可选的 200% 缩放扩展。 |

测试过程中的问题单独记录，未算成产品通过：圈画测试先复现长连线，单字表格测试先复现重叠区域；独立区域排序断言、纯翻页等待写入、读取上一页高亮，以及新增测试的类型推断问题已分别修正。初版 52 项与最终 83 项的范围不同，最终增加了中文表格和持久化/产物检查；见[检查器说明](evidence/implementation/pdf-structure/harness-notes.json)。

本批证据共 157 个文件、17,650,051 字节，另附[校验清单](evidence/implementation/pdf-structure/manifest.json)。所有测试浏览器 context/browser 正常关闭。1423 和 1430 保持原服务进程；1430 实际入口为 `/assets/index-CRNARnkR.js`，主包和 `/player/player.js` 与磁盘构建字节一致，见[预览身份记录](evidence/implementation/pdf-structure/preview-identity.json)。

版面修复对新导入的 PDF 生效，已保存的会议不自动重新分析或迁移批注；重新导入时应保留原会，旧批注不会自动搬过去。列序依赖 PDF 文字流，乱序文字层、其他复杂排版和字符宽度估算仍有边界。识别出的图表支持整区标注，本批不增加单元格编辑或 OCR。

本批未重建原生应用；不能将浏览器结果当作最新 Linux 原生或 Mac 验收。用户暂时没有测试 Mac，触摸/笔、真实音频/模型、VoiceOver、签名更新、完整 G2/G3 与 G4 用户试用继续保留。整体重构未完成，未提交或发布。

后续 [Linux 原生 PDF 续批](native-pdf-validation.md)已在新构建上完成本样本的真实导入、五种标记、系统导出与冷启动，并发现、修复自由编辑翻页沿用旧底图的问题；本报告的原始构建与验证边界不变。
