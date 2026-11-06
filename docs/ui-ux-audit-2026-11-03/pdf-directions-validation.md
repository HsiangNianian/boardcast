# C27 PDF 文字方向与扫描页

2026-11-05，基线 `4f9ab26`。修复旋转正文被当成水印丢弃、180° 文字命中框落到页外，以及竖排句号未结束整句选取的问题。90°、−90°、180°、25° 斜排及页面 `/Rotate` 现在保留文字方向；词级标注在手绘、自由编辑、保存重开及导出中使用相同坐标。扫描页仍保留底图和区域标记，不新增 OCR。

改动沿用 PDF.js 文字层和现有段落分析、marks/spans、撤销与导出路径。文字行及标注框新增可选的 `angle`；没有该字段的旧会议沿用原坐标，不迁移已有标注。原文和竖排标点的字节保持，规范化只参与匹配与句界判断。页边 arXiv 水印在拼好文字碎片后过滤，底图中的原文不被擦掉。没有新增依赖或修改原生存储代码。具体差异见[本批源码补丁](evidence/implementation/pdf-directions/changes.patch)。

| 检查 | 结果与证据 |
| --- | --- |
| 旧版对照 | 同一生产预览更新前，[27 项检查有 9 项不符](evidence/implementation/pdf-directions/before-final/pdf-directions-check.json)，涵盖旋转文字丢失、反向命中框、竖排句界和页面旋转。没有未捕获脚本错误。 |
| 实际导入和选词 | [最终 52 项通过](evidence/implementation/pdf-directions/final/pdf-directions-check.json)。Chromium 生成真实文字层 PDF，点击位置来自独立 HTML 字框；验证六种方向的文本、范围、方向与命中，过滤不同字体碎片组成的水印。[手绘截图](evidence/implementation/pdf-directions/final/hand-marks.png)。 |
| 五种标记与历史 | 对旋转词实际添加圈、下划线、高亮、删除线和星标，撤销/重做只改变一步。单列竖排“保存”跨两个字框，句级选取在竖排句号处结束，保留原文。 |
| 自由编辑及重开 | [自由编辑截图](evidence/implementation/pdf-directions/final/free-marks.png)与实际 [Excalidraw 文件](evidence/implementation/pdf-directions/final/directions.excalidraw)验证图形方向和中心仍对齐原 PDF 词的位置。刷新和重新导入后，graph、原文行、方向和全部标记逐字段一致。 |
| 页面旋转 | 用已安装的 qpdf 给同一 PDF 写入 `/Rotate +90`，再走实际导入；四种原有文字方向的点击仍选到准确单词。见[旋转页面截图](evidence/implementation/pdf-directions/final/page-rotation.png)。 |
| 实际产物 | 下载 PNG、SVG、动画 SVG、整场 PDF、Excalidraw 和回放 HTML。导出 PDF 经 Poppler 栅格化并[目视核对](evidence/implementation/pdf-directions/final/exported-pdf-page1.png)；[HTML](evidence/implementation/pdf-directions/final/directions-replay.html)在新建断网 context 中播放至最后一步，六种来源高亮均存在，未发 HTTP 请求，见[离线截图](evidence/implementation/pdf-directions/final/offline.png)。 |
| 无文字层扫描页 | 实际导入图片组成的 PDF，提交前提示无文字层；底图可解码，不创建伪文字行或不可用的“词”操作，区域高亮可保存，并实际导出回放 HTML。[扫描页截图](evidence/implementation/pdf-directions/final/scan-marked.png)。 |
| 回归和构建 | 最终构建的英文原文 59 项、中文原文 27 项、七格式 92 项和续会 43 项全部通过，见[汇总](evidence/implementation/pdf-directions/results.json)。[完整 305 项 Node/Eval](evidence/implementation/pdf-directions/node-full.txt)及[类型检查、播放器和主应用构建](evidence/implementation/pdf-directions/build.txt)通过。新增 4 项定向测试覆盖方向、句界、完整水印和绘制几何。 |

测试工具问题单独保留：首次扫描断言错误地读取了会议顶层 `underlay`，改为实际解码页面图像后重新跑旧版对照，仍有上述 9 项产品失败。开发中的水印过滤曾只移除前缀，新的碎片测试先失败，再改为整行过滤；最终真实 PDF 也使用混合字体水印。相关原始记录和与初版检查器的差异见[测试说明](evidence/implementation/pdf-directions/harness-notes.json)。本轮中文回归未再次运行可选 200% 缩放扩展，不将 27 项当作此前含缩放的 30 项。

归档含 140 个文件、17,977,456 字节及独立[哈希清单](evidence/implementation/pdf-directions/manifest.json)。所有测试浏览器均已关闭；1423/1430 保留原进程。1430 当前入口 `index-BOu7kCL6.js`，SHA-256 `374175a77a03d288a9bf6479dc59eed3d2acd643a9975bdc8a1a7a28d76baa90`；播放器 SHA-256 `1a7cb239b1ad7dcb5601c9749d34b4eac0da16f96ceaedddae3d72018462c197`。实际服务字节与构建文件一致，见[构建身份](evidence/implementation/pdf-directions/identity-final.json)。

本批未重建原生应用。新旋转标注需使用理解 `angle` 的当前应用或随文件内嵌的新播放器，旧版绘制器不会应用该方向。后续[多列竖排、表格与公式续批](pdf-structure-validation.md)已补上跨列词句与圈画、编号命中和中文表格整区归属的 83 项浏览器检查。字符宽度仍使用既有估算，乱序文字层、其他复杂排版及原生输入设备继续待验。用户暂时没有测试 Mac，真实音频/模型、VoiceOver、签名更新及 G3/G4 仍未完成。整体重构继续，未提交或发布。
