# C05 首次视图切换说明

2026-11-04，基线 `4f9ab26`。补齐完整方案 §4 的首次切换说明：实际从“手绘”切到“自由编辑”或反向切换时，在白板上方说明两者的用途及同一会议关系。“知道了”关闭后记住设备偏好；选择当前视图不触发说明。提示独立于错误消息，不覆盖导入或保存失败。

模式按钮补上方向键、Home/End 和单一 Tab 停靠点。小窗口从“会议详情”操作模式按钮时会显示白板。说明、圈画指引和画布分行布局，空白页引导位于剩余画布区域，关闭说明后焦点回到当前模式。中英文及加载文案同步，没有新增依赖或会议保存字段。

| 检查 | 结果 |
| --- | --- |
| 旧版对照 | [首次切换检查失败](evidence/implementation/mode-intro/before/mode-intro-check.json)，当时没有能力说明；[旧构建身份](evidence/implementation/mode-intro/identity-before.json)与四份源文件保留。 |
| 最终生产页面 | [27 项全部通过](evidence/implementation/mode-intro/final/mode-intro-check.json)：首次/同模式、错误消息保留、键盘导航与焦点、关闭/重开记忆、存储拒绝、中英/深浅及三种尺寸。640×400 下实际打开会前准备并关闭，说明与欢迎引导可见且不重叠。 |
| 相关回归 | 最终产品源码的[四组全部通过](evidence/implementation/mode-intro/regressions/results.json)：双视图往返、内容定位、应用回放、工作台。测试使用隔离存储及合成会议/模型回复。 |
| 逻辑与构建 | [词典及快捷键 9 项通过](evidence/implementation/mode-intro/node-tests.txt)，[完整前端构建通过](evidence/implementation/mode-intro/build-final.txt)，包含类型检查、播放器和主应用。`git diff --check` 通过。 |

实际截图：[英文窄窗欢迎引导](evidence/implementation/mode-intro/final/mode-compact-handdrawn.png)、[中文深色自由编辑](evidence/implementation/mode-intro/final/mode-chinese-dark.png)。早期布局重叠、测试选错窄窗面板及尚未完成显示的截图分别保留在调查目录，判定见[测试说明](evidence/implementation/mode-intro/harness-notes.json)。最终检查等待各元素实际可见，不将 ARIA 状态或坐标存在单独视为显示成功。

1423 开发预览及 [1430 生产预览](http://127.0.0.1:1430/)保留原进程。[实际入口与播放器字节已核对](evidence/implementation/mode-intro/identity-final.json)：生产入口 `index-BfacrHw-.js`，SHA-256 `7788f9f7cf769425529a51251262ff68a5b9608111c0f510a119c988ad74487e`。归档包含源码、限定本批的补丁、检查脚本及哈希清单。

浏览器拒绝偏好写入时，本次会话仍记住关闭状态，重开后可能再次提示。本批未重建原生应用，未验证真实录音/材料的全部切换组合、Mac、VoiceOver、签名更新或 G4 用户试用；不关闭 C05 全范围或 G2–G4 准入。未提交或发布。
