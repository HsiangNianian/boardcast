# C25/C51 板书图种、数学字形与离线标题

2026-11-06，基线 `4f9ab26` 的当前工作区。通过生产界面导入合成记录，补查中、英、日文内容下的七种图节点、分组、连线、长标题与密集板书，发现并修复两处可见内容丢失。本批为浏览器 G1 验收。

## 修复与范围

- 原来的手绘字形没有希腊字母、上下标和部分数学符号。实际模型响应 `α` 只画出空圈，`x² + y₂ ≤ r²` 变成 `x + y r`，积分号也消失；存储文字仍完整。[修复前截图](evidence/implementation/graph-kinds/baseline/math/math.png)、[失败回归](evidence/implementation/graph-kinds/baseline/browser/report.json)。
- 扩展已有字形生成器，优先复用依赖内的 Excalifont，再用随 Excalidraw 分发的 Cascadia；未增加运行时依赖。字形数据从 95 个增到 263 个，原有 95 个 ASCII 对象逐项不变。常用希腊字母、比较符号、积分、数字及字母上下标离线可用，在潦草模式下保持印刷形状。[生成核对](evidence/implementation/graph-kinds/glyph-generation.json)、[修复后截图](evidence/implementation/graph-kinds/math-fixed/math.png)。
- 主页面修复后，离线回放仍试图联网加载自动标题“补充”，失败后标题为空。标题不是保存的节点文字，原来的字形收集漏掉了它。布局与回放资源收集现在共用原有标题常量，额外打包七个不同汉字。[中间失败记录](evidence/implementation/graph-kinds/intermediate/offline-failure.json)。
- 新增字体数据许可，离线播放器沿用已有许可注释机制，把两份许可随单文件输出。生成器保留仅含一两个像素的点状符号；曾尝试保留所有短片段，但发现会改变旧 ASCII，最终只在没有常规笔画时保留点，旧字形比较通过。

变更涉及共享字形读取、既有生成器及数据、自动标题与回放收集、播放器许可、两处单元测试及新生产检查。调用范围包括实时手绘、静态/动态 SVG、PNG、PDF 与离线播放器；[本批差异](evidence/implementation/graph-kinds/batch.diff)不包含工作树内其他批次改动，生成数据另由前后哈希和 [ASCII 原数据](evidence/implementation/graph-kinds/ascii-before.json)记录。

## 验证

[`check-graph-kinds.cjs`](../../scripts/ui/check-graph-kinds.cjs) 使用实际导入、IndexedDB、双视图编辑器和下载入口，主页面拒绝字形 CDN，导出的 HTML 在新上下文中阻断全部 HTTP。每种语言包含 topic/item/node/formula/question/note/region 七种节点、分组、双端绑定箭头、原文底图与标记；日文样本增至 30 个节点。

[最终预览的 28 项检查](evidence/implementation/graph-kinds/browser/report.json)通过：

- 三种语言的七类节点、长标题、分组和连线可见；`α` 独立公式具有实际笔画。
- SVG 的全部路径位于导出边界内；Markdown 保留完整语义文字；Excalidraw 保留图数据、标签、分组与箭头两端绑定。
- PNG、动画 SVG、实际 PDF 和离线 HTML 成功下载；PDF 经 `pdftoppm` 解码并目视核对。[日文 PDF](evidence/implementation/graph-kinds/browser/ja-pdf.png)、[自由编辑](evidence/implementation/graph-kinds/browser/ja-free-edit.png)。
- 离线 HTML 显示数学与日文字形、自动标题，图数据完全一致，无 HTTP 请求并包含两份字形许可。[离线画面](evidence/implementation/graph-kinds/browser/offline-replay.png)。
- 切换视图、导出和冷重载保持当前会议身份与图数据，无未捕获页面错误。

相关生产回归分别通过连线 10 项、外观 82 项、语言 69 项，见 [相关日志](evidence/implementation/graph-kinds/related/browser.log)。字形、线宽、布局和回放数据的 [15 项 Node 测试](evidence/implementation/graph-kinds/node-tests-final.log)、TypeScript、播放器及主应用构建通过。旧版新增数学测试和生产检查先明确失败；先前 27 项生产检查通过后，仅补上播放器许可，并在最终构建重新执行上述 28 项。构建仍有既有大 chunk 提示。

```sh
node --test src/engine/glyphs.test.ts src/engine/inkWeight.test.ts src/engine/layout.test.ts src/engine/replayHtml.test.ts
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-graph-kinds.cjs
```

1430 已更新至 `index-DNHLI2pG.js`，静态资源先写入、入口最后替换，原有 1,262 个资源文件保持；实际 HTTP 响应与最终产物哈希一致，见 [同步记录](evidence/implementation/graph-kinds/publication.json)。1423/1430 原预览进程保留，私有测试服务器已结束。

## 保留边界

字形覆盖限于已有中日数据和内置字体可生成的字符，`ℝ`、`∈`、`⊂` 等仍未覆盖；没有新增 LaTeX 解析。日文验收指内容保留，界面及自动标题仍沿用现有中英语言选择。样本不能证明任意 Unicode、极端密度和所有视口组合。

本批没有重建或测试原生安装包，未重复全量 Node/Rust 测试。Windows ARM 源码修复及既有 Linux 实际模型验证见 [本地模型报告](windows-arm-local-model-validation.md)，新 Windows ARM 安装包和 Adreno 原机预热尚未验证。Mac 实机、物理音频、VoiceOver 和完整 G3/G4 仍保留。
