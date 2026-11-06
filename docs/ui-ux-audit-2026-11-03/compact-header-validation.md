# 紧凑顶栏与保存反馈

2026-11-05，落实方案 §3、C04/C67。此前在 720×480 窗口中，保存期间聚焦白板工具会把整条顶栏滚走，`Saving…` 位于 **y=-57px**，见[失败测量](evidence/implementation/compact-header/baseline.json)及[截图](evidence/implementation/compact-header/baseline.png)。现在同一状态位于 **y=13.5px**，保存中、失败和重试后的反馈均保持可见。

宽度不超过 900px 时，复盘、导入、导出、新会议、主题和设置收进「更多」，复用同一组操作及原有禁用条件。矮窗口固定会议名称和保存状态，白板工具及错误详情独立滚动，重试按钮仍可通过键盘到达。没有改变保存事务、数据格式或模型配置。产品涉及五个文件，见[本批补丁](evidence/implementation/compact-header/product.patch)。

浮动菜单显式引用持续存在的触发按钮；关闭菜单或弹窗后恢复焦点。文件输入框保留在菜单之外，打开选择器前先聚焦触发按钮，空选择取消后不会停在 `body`，此前的[失焦记录](evidence/implementation/compact-header/chooser-baseline.json)已保留。展开导入/导出时，「更多」也正确声明展开状态。

| 检查 | 结果与证据 |
| --- | --- |
| 保存状态与键盘 | 最终构建[24 组通过](evidence/implementation/compact-header/header/report.json)：中/英、亮/暗，以及四种鼠标窗口和两种触屏窗口，最小 360×240。240 次保存反馈测量通过；另检查菜单 Home/End/Escape、展开状态、弹窗返回焦点、空文件选择及跨断点关闭。 |
| 桌面与真实缩放 | [24 组通过](evidence/implementation/compact-header/visual-verified/matrix.json)，三种窗口尺寸、中/英、亮/暗与实际浏览器 100%/200% 缩放；保存状态、停止、关闭和返回焦点可达。 |
| 触屏 | [16 组、2,502 次控件测量通过](evidence/implementation/compact-header/touch-verified/report.json)，包括「更多」内六项操作；44×44px、滚动后视口边界及中心命中检查通过。 |
| 既有流程 | [14 组及本地模型通道通过](evidence/implementation/compact-header/regression-report.json)：外观、工作台、导出、美化、跟随、圈选、纠错样本、术语、模式提示、来源标记、原话阅读、导入、录音切换和生命周期；本地通道另含 10 项检查。部分使用受控原生协议，不是物理音频或 Windows GPU 验收。 |
| 单元与构建 | [328 项 Node/Eval 通过](evidence/implementation/compact-header/node-tests.log)，TypeScript、Vite 和差异格式检查通过；没有新增依赖。 |
| 用户预览 | [1430 预览](evidence/implementation/compact-header/preview.json)核对最终产物的 11,575 个文件及九个入口资源，保留旧哈希资源供已打开的页面使用。鼠标和触屏模式[实际打开准备面板、更多及设置](evidence/implementation/compact-header/preview-smoke.json)，关闭后焦点正确，无页面异常。 |

缩放与触屏矩阵运行于 `index-Bf_J_yU7.js`；最终构建 `index-COymLtxO.js` 只进一步修正「更多」的 `aria-expanded` 表达式，布局和操作逻辑相同。最终 24 组顶栏检查、美化/圈选回归、本地通道及用户预览检查使用新构建；[构建身份](evidence/implementation/compact-header/identity.json)和流程结果保留这一边界。

回归失败记录未删除：外观旧测试仍期待 0.38 的淡墨值，在改动前的 1430 构建上同样失败；现改为核对既有产品的 0.68。另有测试在响应式状态更新前立即读取旧按钮或布局，共用入口现在等待对应布局；圈选仍要求画布至少 100px，最终中英文均为 **110.47px**。见[原始失败汇总](evidence/implementation/compact-header/regression-appearance.json)、[旧构建外观失败日志](evidence/implementation/compact-header/appearance-baseline.log)和[调查说明](evidence/implementation/compact-header/investigation/notes.json)。

查看[英文保存中](evidence/implementation/compact-header/header/en-light-720x480-mouse-saving.png)、[中文暗色触屏](evidence/implementation/compact-header/header/zh-dark-720x480-touch-saving.png)。样本与浏览器上下文独立，没有使用用户会议或采集音频。空文件选择不代表真实系统对话框验收；矮窗口的详细错误仍需在工具区滚动查看。

[SHA256 清单](evidence/implementation/compact-header/SHA256SUMS)已逐项复算，见[归档核对](evidence/implementation/compact-header/verification.json)。两个私有测试预览已关闭，原 1423/1430 进程保留，见[清理记录](evidence/implementation/compact-header/cleanup.json)。

本批没有重新构建原生安装包，也没有 Windows ARM/Adreno、用户暂无设备的 Mac、VoiceOver 或物理触屏结果。[Windows ARM 原生请求修复](windows-arm-local-model-validation.md)保留在工作区，仍需新 Windows 构建在原机器复测。完整 UI、G3/G4 验收继续保留；未提交、推送或发布。
