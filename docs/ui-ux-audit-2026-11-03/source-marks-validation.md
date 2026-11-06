# C27 原文标注与持久高亮

2026-11-04，基线 `4f9ab26`。本批补上可保存的高亮、标记按钮的按下状态，以及自由编辑中的原文底图和标注投影。五种标记继续使用现有的 marks/spans、撤销和回放路径；发言提示沿用临时高亮状态。没有新增依赖、会议格式字段或原生代码。

词、句、段的工具按钮现在明确报告开关状态。由内容列表定位整段、尚未点击具体文字时，词/句范围暂不可用并说明如何选词，避免把词级操作静默应用到整段。自由编辑显示锁定的原文底图，保存用户绘制时去掉这份显示副本；同一个处理也用于 Excalidraw 导出和重新导入。短小的多行原文在整段划掉时画交叉线，避免划线落在行间。Markdown 保留被标注原文的全文，并保留流程节点、公式和其他节点的标记语义。

| 检查 | 结果与边界 |
| --- | --- |
| 旧版对照 | [7 项中 4 项不符](evidence/implementation/source-marks/before/source-marks-check.json)：缺少高亮按钮、标记开关状态、自由编辑原文底图和标注投影。真实 PDF 导入及左栏词命中在旧版已通过。 |
| 最终生产页面 | [59 项全部通过](evidence/implementation/source-marks/final/source-marks-check.json)：五标记×词/句/段的实际开关、词两端点击、双栏同词、跨行句子、断行英文、长英文、一次撤销/重做、内容列表文字语义、键盘定位、中英文 640px 工具条、双视图、绘制共存、保存重开、导出与重新导入。 |
| 输入与命中依据 | Chromium 生成[真实文字层 PDF](evidence/implementation/source-marks/final/two-column.pdf)，通过生产导入对话框读取。鼠标坐标来自独立 HTML 字符盒及实际画布变换，没有调用被测的词句定位函数来构造预期结果。材料为合成文档，没有替换 PDF.js、命中函数、存储或渲染。 |
| 实际产物 | 标注页的 [Markdown](evidence/implementation/source-marks/final/source.md)、[SVG](evidence/implementation/source-marks/final/source.svg)、[动画 SVG](evidence/implementation/source-marks/final/source-animated.svg)、[Excalidraw](evidence/implementation/source-marks/final/source-free.excalidraw)、[三页 PDF](evidence/implementation/source-marks/final/source-export.pdf)和[回放 HTML](evidence/implementation/source-marks/final/source-replay.html)均实际下载。PDF 已解析并栅格化目视核对原文、高亮、删除线和自由绘制；HTML 在新建、断网的浏览器 context 中打开并定位到最终步骤。 |
| 回归 | 双视图往返、内容列表、七格式导出 92 项、模式切换 27 项、圈选 33 项[均通过](evidence/implementation/source-marks/regressions/results.json)。最终补齐公式 Markdown 后，[七格式 92 项重新通过](evidence/implementation/source-marks/exports-final/export-check.json)，原文 59 项也在最终构建上重跑。 |
| 代码与构建 | [完整 288 项 Node/Eval 测试通过](evidence/implementation/source-marks/node-full-final.txt)，包含 6 项新增标注/高亮/底图测试。完整[生产构建](evidence/implementation/source-marks/build-final.txt)包含类型检查、播放器和主应用；`git diff --check` 通过。 |

测试编写时修正了读取尚未初始化数据库、把可重建场景当作必存内容、撤销后未重新选中、缩窗后点击画布外，以及直接点击无指针事件的 SVG/隐藏单选框等问题。这些不计为产品失败；最终操作使用真实可见工具、鼠标坐标和独立导出结果，详见[测试说明](evidence/implementation/source-marks/harness-notes.json)。

1423 开发预览和 [1430 生产预览](http://127.0.0.1:1430/)均已确认可访问。原进程退出后，经 PID、端口和连接拒绝核实才重启，见[重启记录](evidence/implementation/source-marks/preview-restart.json)。最终入口 `index-CF4hTokX.js`，SHA-256 `a830ff3a4cc6f55a031426eb1b73c877b2f47363a138f994799533031ebae427`；播放器 SHA-256 `305b4ec59714677589d98ef6967b41a24e61718ae93e60a6c46b145c8d750f59`，均与实际服务字节一致，见[构建身份](evidence/implementation/source-marks/identity-final.json)。

本批没有重建原生应用。新增标注在真实 200% 缩放、触摸/笔、读屏、真实音频/模型与 Mac 上未验；中文原文词界、旋转/竖排、扫描/OCR、表格和公式等复杂材料变体仍属于 C27 后续验收。C30 美化、C38 完整续会及既有 G3/G4 门槛继续保留，不将本批视为整个重构或 C27 全部完成。未提交或发布。

后续的[中文词句与 Unicode 批次](cjk-marks-validation.md)已修复中文词界、引号、扩展汉字及稀疏段落，并补齐 Chromium 真实 200% 缩放；[文字方向续批](pdf-directions-validation.md)又补上旋转/反向文字、竖排句界、页面 Rotate 和扫描页边界。多列竖排、表格/公式归属、原生和 G3/G4 边界仍保留。
