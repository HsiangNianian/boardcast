# C28 圈选反馈与排队目标

2026-11-04，基线 `4f9ab26`。此前发送已限制为最多 8 个目标，界面却没有明确显示引用数量、超限保留规则或上级目标合并，窄窗提示还会省略文字。本批补齐这些反馈，保留原有命中、重叠圈合并、最多三圈及最近八个目标的选择规则。

提示现在显示实际将发送的目标数量；大范围圈选发生层级精简时，说明子项由圈中的上级目标代表；总目标超过 8 个时，说明只保留最近 8 个。提示单独占一行，不盖住画布。长名称只缩短预览，完整名称仍在会议中。模型回复删除尚未发送的目标后，数量提示会更新为空，下一句也不携带失效 ID。键盘取消后焦点回到可见的圈选工具。

| 检查 | 结果与边界 |
| --- | --- |
| 有效旧版对照 | [24 项中 6 项不符](evidence/implementation/circle-feedback/before/circle-feedback-check.json)：数量、超限和父级合并提示缺失，以及中英文窄窗反馈。命中、合并、排队归属与跨页重试在旧版即通过，本批没有重写这些逻辑。 |
| 最终生产页面 | [33 项全部通过](evidence/implementation/circle-feedback/final/circle-feedback-check.json)：真实鼠标画圈、空圈、重叠/分离圈、撤销/取消、单圈及多圈八目标限制、层级精简、长名称、排队成功/失败、失效目标、跨页重试、切换视图清理、保存重开及键盘焦点。 |
| 目标归属 | 同一生产 App 中捕获 8 次模型调用参数，以受控传输暂停、回复或返回 503。每句及其保存记录持有自己的目标；新圈不污染旧请求，失败重试仍指向原页且不重复原话。调用参数证明接入与归属，不证明真实模型理解或识别质量。 |
| 中英文窄窗 | 640×400 下完整显示数量及限制，提示与画布分行；实际“显示完整白板”保留圈选，中文键盘取消可返回工具。[英文截图](evidence/implementation/circle-feedback/final/limit-compact-en.png)、[中文截图](evidence/implementation/circle-feedback/final/limit-compact-zh.png)。 |
| 回归与构建 | 最终构建的[模式切换 27 项、手绘/离线播放器 16 项通过](evidence/implementation/circle-feedback/regressions/results.json)；[定向 Node 23 项通过](evidence/implementation/circle-feedback/node-tests.txt)，[完整前端构建](evidence/implementation/circle-feedback/build-final.txt)包含类型检查、播放器和主应用；`git diff --check` 通过。 |

最初测试错误地从历史上下文读取了当前目标，另一次在视图尺寸未稳定时采样坐标，画出的圈只包围一排节点；这些不计为产品回归。最终检查读取新句前紧邻的目标行，并等待画布与 SVG 尺寸一致，每次手势及显示结果都已记录，详见[测试说明](evidence/implementation/circle-feedback/harness-notes.json)。

1423 开发预览及 [1430 生产预览](http://127.0.0.1:1430/)保留原进程。[入口和播放器字节已核对](evidence/implementation/circle-feedback/identity-final.json)：生产入口 `index-cY44W7JE.js`，SHA-256 `457b74f6688b769ec61de37840283aa38d3c69909a875c659ee4fb4d2c68d55e`。源码、限定本批的补丁、检查脚本与哈希清单已归档，没有新增依赖或会议格式字段。

本批未重建原生应用，未验证触摸/笔硬件、真实语音与模型、Mac/VoiceOver、签名更新和 G4 用户试用。C27 原文词句段/PDF 标注、C30 美化变体及其余准入项继续保留；不将本批浏览器证据视为 C28 的 G3 验收。未提交或发布。
