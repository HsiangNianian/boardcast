# C27 中文词句与 Unicode 定位

2026-11-04，基线 `4f9ab26`。中文原文点击“词”现在按词选择，跨行的“会议”仍是同一个词；句末引号和连续问叹号留在所属句子。修复扩展汉字、大小写展开导致引用位置偏移的问题，并兼容 PDF 中的汉字兼容字符、连字和组合重音。留白较大的独立段落不再被当作普通行距合并。

改动集中在现有 `spans.ts` 和 `paper.ts`：手动标注、模型引用和 PDF 导入共用这些路径。分词使用项目已采用的 `Intl.Segmenter`；英语连字符及下划线标识符保持完整。Unicode 规范化只用于匹配，保存的原文、标注文字和坐标仍保留原样，没有新增依赖、会议字段或原生代码。段落识别修复只作用于新导入，已有会议不会被重新切段或迁移。

| 检查 | 结果与边界 |
| --- | --- |
| 旧版对照 | 旧生产构建完整执行 [26 项检查，其中 19 项不符](evidence/implementation/cjk-marks/before/cjk-marks-check.json)。包括中文误选整串、跨行插空格、引号落入下一句、扩展汉字引用偏移及稀疏段落合并。 |
| 最终中文流程 | [30 项全部通过](evidence/implementation/cjk-marks/final/cjk-marks-check.json)：真实 PDF 导入、词/句鼠标命中、两行同词、中英混排、引号/连续标点、原文段落身份、受控模型引用、保存重开、手动识别已有标记及一次撤销。 |
| 缩放与键盘 | 使用 Chromium `chrome.tabs.setZoom(2)`，验证实际缩放因子 2、DPR 2、CSS 视口 640×400、`visualViewport.scale` 为 1。中文词句段及五个标记按钮均在视口内，键盘 Enter 可开关高亮；见[测量记录](evidence/implementation/cjk-marks/final/cjk-marks-check.json)、[截图](evidence/implementation/cjk-marks/final/cjk-200-percent.png)和[浏览器生命周期](evidence/implementation/cjk-marks/final/browser-lifecycle.json)。这不是触摸、读屏或原生 WebKit 验收。 |
| 实际导出 | 下载并检查 [Markdown](evidence/implementation/cjk-marks/final/cjk-marked.md)、[Excalidraw](evidence/implementation/cjk-marks/final/cjk-marked.excalidraw)、[PNG](evidence/implementation/cjk-marks/final/cjk-marked.png)和[回放 HTML](evidence/implementation/cjk-marks/final/cjk-replay.html)。可恢复绘图保留完整原文与标注，HTML 在新建断网 context 打开并跳至最终步骤，确认原文底图和保存的高亮。 |
| 相邻回归 | 最终构建的[英文双栏原文 59 项](evidence/implementation/cjk-marks/source-regression/source-marks-check.json)和[导入任务 14 项](evidence/implementation/cjk-marks/import-regression/import-task-check.json)全部通过。英文检查包含五标记、词句段、断词、双视图、撤销和各导出产物。 |
| 代码与构建 | [40 项定向测试](evidence/implementation/cjk-marks/node-after.txt)、[完整 297 项 Node/Eval](evidence/implementation/cjk-marks/node-full-final.txt)及[完整生产构建](evidence/implementation/cjk-marks/build-final.txt)通过。新增 8 项词句测试和 1 项稀疏段落测试；日语词界、混排 `foo-bar`、组合重音与兼容连字另有 Node 断言。`git diff --check` 通过。 |

PDF 由独立 HTML 文本盒生成，鼠标坐标来自这些盒子及页面实际 SVG 变换。测试没有替换 PDF.js、词句定位、存储或渲染，仅控制本地模型 HTTP 回复。测试脚本先后修正了源节点身份查找和全角标点预期归一化问题；它们与产品失败分开保留，见[测试说明](evidence/implementation/cjk-marks/harness-notes.json)。复核旧版原始范围后，修正预期归一化并未改变旧版 19 项失败的结论。

证据归档包含 79 个文件、9,768,031 字节及独立[哈希清单](evidence/implementation/cjk-marks/manifest.json)，包含改动前后源码、失败对照、实际产物和清理记录。最终入口 `index-ycdy3Oup.js`，SHA-256 `37daa21309a1de319f1153a427df74559ad4f4eb1fe2456a619d3068f8f472ae`；播放器保持 `305b4ec59714677589d98ef6967b41a24e61718ae93e60a6c46b145c8d750f59`。二者均与实际服务字节一致，见[构建身份](evidence/implementation/cjk-marks/identity-final.json)。1423 开发预览和 [1430 生产预览](http://127.0.0.1:1430/)保留原进程，可直接查看。

本批未重建原生应用。后续[文字方向与扫描页](pdf-directions-validation.md)已修复旋转正文、180° 命中框和竖排句界，完成 52 项生产检查，扫描页保持无自动 OCR；多列竖排、表格/公式归属及其他文字层变体继续待验。字框仍采用既有文字层宽度估算。用户暂时没有测试 Mac；Mac、VoiceOver、真实音频/模型、触摸/笔及 G3/G4 继续保留。本批不代表整个重构完成，未提交或发布。
