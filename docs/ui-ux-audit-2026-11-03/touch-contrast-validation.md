# 触屏命中区与渲染配色

2026-11-05，接续 C67 和方案中的视觉标准。产品改动集中在 `src/style.css`，见[本批补丁](evidence/implementation/touch-contrast/product.patch)。

触屏的小窗规则原先把录音按钮压到 34px，设置弹窗中也有 36px 按钮。现在应用、弹窗和浮动菜单中的按钮、文本输入及选择框至少为 44×44px；复选框扩大可点击标签。说话人操作横向排列，原话头像列同步加宽，窄屏隐藏输入栏中重复的设备名称，让打字区域有足够空间。规则限定 `pointer: coarse`，Excalidraw 内部控件不在此次扩展范围内。

橙色头像文字原来的对比度为 **4.420:1**，自动检查把单字头像列为“无法判定”。复核实际前景/背景色后，将文字稍微加深，达到 **4.879:1**；六种头像配色均纳入既有渲染颜色检查。

| 验证 | 结果与证据 |
| --- | --- |
| 失败基线 | [录音按钮](evidence/implementation/touch-contrast/baseline-targets/report.json)实测 135.58×34px；[配色检查](evidence/implementation/touch-contrast/baseline-contrast.log)在 4.420:1 处失败。 |
| 触屏 | [16 组通过](evidence/implementation/touch-contrast/touch/report.json)：中/英、亮/暗、1280×800 / 900×600 / 720×480 / 480×720。2,490 次控件测量均满足 44×44px，滚动后位于视口内、中心未被遮挡，壳层无水平溢出；包含菜单、设置六分类、原话/摘要/内容、说话人及复盘，并实际触发触摸导航。 |
| 桌面与缩放 | 最终产物[24 组通过](evidence/implementation/touch-contrast/visual/matrix.json)，覆盖三种窗口尺寸、中/英、亮/暗与真实浏览器 100%/200% 缩放，停止、关闭和返回焦点可达。 |
| 配色 | [30 组渲染颜色组合通过](evidence/implementation/touch-contrast/regression/theme-contrast/rendered-theme-contrast.json)：既有 18 组主题颜色，加上两种主题下的全部六种头像。 |
| 自动无障碍抽查 | [68 个最终页面状态](evidence/implementation/touch-contrast/final-axe/index.json)运行六条 axe 规则，没有确认的违规；**36 帧仍有无法判定的对比度项**，主要为单字与局部遮挡，不视为通过。 |
| 既有流程 | [五组回归通过](evidence/implementation/touch-contrast/regression-report.json)：工作台、输入栏、说话人、配色与共享弹窗；前四组使用生产页面，弹窗组件使用开发测试入口。 |
| 构建与预览 | TypeScript 和 Vite 构建通过；[构建身份](evidence/implementation/touch-contrast/identity.json)已记录。[1430 预览](evidence/implementation/touch-contrast/preview.json)与最终产物的 11,800 个文件一致，入口及九个入口引用资源逐字节核对。[实际预览启动检查](evidence/implementation/touch-contrast/preview-smoke.json)在鼠标和触屏模式均能打开准备面板并关闭，无页面异常。 |

查看[触屏输入栏](evidence/implementation/touch-contrast/touch/en-light-720x480-workbench.png)、[竖屏暗色设置](evidence/implementation/touch-contrast/touch/en-dark-480x720-general.png)和[中文复盘](evidence/implementation/touch-contrast/touch/zh-dark-480x720-review.png)。浏览器上下文和原生接口均为独立合成样本，没有使用用户会议或采集音频。竖屏沿用既有工作台旋转布局。

测试过程中还修正了两处测量问题：滚动旧原话会出现“跳到最新”，需要等布局更新后再次定位；该按钮的出现/消失会改变列表序号，探针改用稳定元素引用。第一次 CSS 扩展遗漏了浮动导入菜单的文本框，后续已覆盖。[调查记录](evidence/implementation/touch-contrast/investigation.json)保留产品问题与测试时序问题的区别。

[归档核对](evidence/implementation/touch-contrast/verification.json)及 [SHA256 清单](evidence/implementation/touch-contrast/SHA256SUMS)已保存，清单中 196 个文件均通过复算。私有 1441 测试预览已关闭，原有 1423/1430 预览进程保留，见[清理记录](evidence/implementation/touch-contrast/cleanup.json)。

axe-core 4.14.0 作为独立测试工具使用，没有加入项目依赖。[官方 API](https://www.deque.com/axe/core-documentation/api-documentation/)说明检查对象是当前渲染内容；[目标尺寸规则](https://dequeuniversity.com/rules/axe/4.11/target-size?application=RuleDescription)采用 24px 或足够间距，本项目的 44px 标准由单独探针验证。上述抽查不代表完整 WCAG 验收，也不覆盖所有弹窗内容、模型状态或画布对象。

本批没有物理触屏、Mac/VoiceOver、60 分钟现场会议或五名新用户试用，也没有重新构建原生安装包。此前的 [Windows ARM 请求通道修复](windows-arm-local-model-validation.md)仍待新 Windows 构建在原 Adreno 机器复测；不能把本轮浏览器结果当作 Windows GPU 验收。C67 与 G3/G4 的这些边界继续保留，工作区未提交、推送或发布。
