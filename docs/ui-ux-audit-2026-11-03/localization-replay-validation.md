# C61 续批：回放语言与自由编辑切换

2026-11-04，基线 `4f9ab26`。本批修复离线 HTML 丢失导出语言的问题，并验证已有自由编辑语言切换。C61 的全路径本地化仍未完成，剩余错误文案和原生菜单/托盘继续验收。

## 实际问题与最小修复

生产版实际下载复现了两种错配：英文界面导出的页面在中文浏览器中显示中文控件，中文界面导出的页面在英文浏览器中显示英文控件；标题保留作者语言，静态 `<html lang>` 则始终为 `zh-CN`。见 [双向复现](evidence/implementation/localization/baseline-result.json)、[英文导出在中文浏览器的截图](evidence/implementation/localization/baseline-en.png)。

App 现在把导出时解析后的 `en/zh` 放入已有回放快照。HTML 的语言声明和标题使用该值，新播放器在渲染前将它应用到本次会话，不写入接收方的 localStorage。异步导出期间切换 App 语言不会改变正在生成的 HTML。会议正文、摘要和图中文字保持原文，不进行内容翻译。

`ReplayData.lang` 是可选字段。新播放器遇到缺失或未知语言时仍按接收方偏好启动；已经导出的旧 HTML 内含旧播放器，不会自动更新，需要重新导出才能获得此次修复。四个产品文件的本批差异见 [product.patch](evidence/implementation/localization/product.patch)。

## 已运行验证

| 项目 | 证据与范围 |
| --- | --- |
| 本地化生产检查 | [69 项通过](evidence/implementation/localization/localization/localization-check.json)。英文、中文、跟随系统三种真实下载；导出中暂缓播放器请求，切换 App 语言后继续下载，确认导出仍使用点击时的语言 |
| 接收方与存储 | 七个独立禁网 context：英文→中文接收方、中文→英文接收方、日文系统回退后的英文导出、延迟导出、禁止 localStorage、缺失语言、未知语言。页面语言、控件名称、标题及原文按各自规则核对；接收方偏好保持，零语言写入、HTTP 请求及脚本错误 |
| 自由编辑 | 英文可见 Rectangle 控件实际绘制并保存一个矩形；切换中文后可见“矩形”，再切回英文，对象 ID、位置与样式保持。该路径由已有 PencilBoard 实现提供，本批没有改动它 |
| 系统与长文本 | 注入 `navigator.languages` 和 `languagechange` 验证日文回退、繁体中文选择、手动英文覆盖系统。保留中英日混合会议原文与长标题，七个离线场景均在 1280×800 和 640×400 检查标题及控制条边界，无横向溢出 |
| 回归 | 最终生产构建 [七格式 68 项](evidence/implementation/localization/exports/export-check.json) 与 [真实设置组件 Dialog 回归](evidence/implementation/localization/dialogs/runner-result.json) 通过；后者运行于开发 fixture，包含六分类、焦点、嵌套弹窗与滚动恢复 |
| 单测和构建 | [10 项 Node](evidence/implementation/localization/node-tests.log) 覆盖字典、Markdown、HTML 嵌入转义与固定语言；[类型检查](evidence/implementation/localization/typecheck-result.json)、[播放器构建](evidence/implementation/localization/player-build-result.json)、[主应用生产构建](evidence/implementation/localization/main-build-result.json) 均通过 |

语言检查使用真实 App、IndexedDB、Excalidraw 和实际下载文件；图和原话是合成输入。系统语言事件是浏览器内注入，不代表在真实日文/中文操作系统中切换语言。旧数据兼容检查将新播放器的数据字段删去或改为未知值，不宣称已经修改或修复此前发出的旧 HTML。

基线辅助字段 `englishRectangle:false` 来自查询隐藏的 radio input，不是 PC 文案错误的证据。最终检查查询可见控件容器并实际绘图。首次新检查的画图起点落在样式面板，修正为实际画布坐标并增加命中检查；记录见 [脚本修正说明](evidence/implementation/localization/initial-harness-failure/note.txt)，没有为此改产品代码。

## 构建身份与下一步

最终入口 `index-Cs_WbBII.js`，SHA-256：`8034871b9703e474faf4276a0b3f3e928f157376109d3f89a850db0d0f99d00f`。播放器 SHA-256：`6fba6a7f6e52bb304a7f746587c504a90dbe8c96affa49b1378fd0802fead57d`。生产目录中的播放器与 `public/player/player.js` 字节一致，构建时源码与最终源码哈希一致；见 [身份与命令](evidence/implementation/localization/identity.json)。基线复用了 C60 已验证的生产产物，并先核对源码和播放器身份。

源码扫描仍发现 [28 处英文错误候选](evidence/implementation/localization/remaining-error-candidates.txt)，涉及保存/迁移冲突、场景校验、模型候选、绘图导入和背景导出。这是待核对清单，不是已逐项复现的 28 个缺陷。接下来应验证中文用户实际收到的提示，再在保留错误含义、原件保护和重试行为的前提下本地化。PDF/Markdown 导出过程中切换语言也未在本批验证。

本批未执行原生菜单/托盘、Mac/VoiceOver、真实操作系统语言切换、Rust/原生构建或 G4 用户试用，不关闭 C61、C67 或整个重构目标。未提交、推送或发布；测试浏览器及私有预览已清理，保留用户 1423/1430 服务，见 [清理记录](evidence/implementation/localization/cleanup.json)。
