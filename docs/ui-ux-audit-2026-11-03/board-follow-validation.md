# 白板阅读视角与减少动态效果

2026-11-04，基线 `4f9ab26`。对照完整方案 §4 的阅读与动效规则，发现手绘和自由编辑各有一处自动适配逻辑：用户平移后，只要新内容超出画面，下一次生成就会重新居中。原先的减少动态效果只关闭手写动画，没有阻止这次视角变化。

两种视图现在保留用户平移、缩放或编辑时选择的视角，并提供“显示完整白板”。该动作重新适配当前内容，键盘操作后焦点回到白板。开启减少动态效果时，后续内容增长不自动移动视角；首次展示内容、切换会议/页和明确要求重新适配仍可正常定位。手绘的定位、圈画和拖动也进入阅读状态，擦除动画结束不会恢复手势之前的视角。离线 HTML 使用同一手绘组件；它显示当前回放步骤的完整白板，不跳到最后一步。

自由编辑沿用现有视角和 `tidy` 适配入口，在真实指针、滚轮和键盘操作时暂停跟随，没有新增依赖、会议格式或保存字段。原有场景编辑、撤销、导出和 ASR 策略保持。

证据位于 [board-follow 归档](evidence/implementation/board-follow/identity-final.json)。不同阶段保留各自身份；手绘旧版和自由编辑旧版并非同一个生产构建。以下均为真实生产页面，模型回复和会议数据是隔离的合成 fixture。

| 检查 | 结果与边界 |
| --- | --- |
| 手绘/播放器旧版 | [13 项中 8 项不符](evidence/implementation/board-follow/ink-before/board-follow-check.json)：平移/缩放被夺回、减少动态仍改视角、缺少返回入口、离线回放同样移动视角。 |
| 手绘/播放器最终版 | [16 项通过](evidence/implementation/board-follow/ink-final/board-follow-check.json)：实际鼠标平移/滚轮、生成后坐标不变、擦除结束竞争、减少动态、640×400 中英按钮与键盘焦点、翻页及实际下载的离线 HTML。 |
| 自由编辑旧版 | [8 项中 5 项不符](evidence/implementation/board-follow/free-before/free-board-follow-check.json)。手形工具真实拖动将色块移出画面，后续生成又将其拉回；减少动态下色块从 122×82 像素缩成 58×39 像素。 |
| 自由编辑最终版 | [11 项通过](evidence/implementation/board-follow/free-final/free-board-follow-check.json)：生成后保持实际画布像素位置、明确适配、窄窗键盘操作、存储中自由对象位置/尺寸不变；空白页平移后的首次生成保留已选缩放。 |
| 相关回归 | [五组全部通过](evidence/implementation/board-follow/regressions/results.json)：双视图往返、内容定位、应用回放 31 项、工作台和七格式导出 92 项。所有页面均无未捕获脚本错误。 |
| 逻辑与构建 | [282 项 Node 测试全部通过](evidence/implementation/board-follow/node-tests-final.txt)，[完整前端构建](evidence/implementation/board-follow/frontend-build-complete.txt)包含类型检查、独立播放器和主应用。 |

测试准备中的无效选择器、重复合成节点、欢迎窗口拦截手势，以及“动画结束必须删除遮罩”的错误等待条件，均在 [测试说明](evidence/implementation/board-follow/harness-notes.json)单独记录，不作为产品反例。实际播放器动画已完成，完整笔迹另经[截图](evidence/implementation/board-follow/ink-final/offline.png)确认。

1423 开发预览和 [1430 生产预览](http://127.0.0.1:1430/)保留原进程并已核对返回字节。生产入口 `index-BDnOHr0K.js`，SHA-256 `e34a66ed3ee0b22dac1d4467c0575d903bcbc32382cf2d4a8d3d7d0051428390`。源文件、补丁、实际导出文件及构建身份已归档。

本批未重建或运行原生应用，未验证触摸硬件、Mac/VoiceOver、真实音频、签名更新和 G4 用户试用。它补齐阅读视角这一项，不关闭 C01–C68 或 G2–G4 的整体验收；未提交或发布。
