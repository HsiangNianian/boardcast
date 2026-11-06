# C15/C62 续批：按住空格收音的失焦与组合键

2026-11-04，HEAD `4f9ab26`。本批处理按住空格收音的生命周期与帮助说明；完整键盘、原生音频和 G3/G4 范围继续保留。

## 修改

原实现只在收到空格释放或切换画布模式时停止本次按住启动的收音。切离窗口可能收不到释放事件，尚未完成的启动也会在后台继续；Ctrl/Alt/Meta/Shift 加空格同样进入该路径，并可能与全局收音开关冲突。

现在仅裸空格进入按住说话。窗口失焦或页面隐藏时，清除本次按住/释放等待状态并调用既有停止流程；迟到的启动沿用已有取消标记完成清理。普通松开仍保留原有 300 ms 收尾时间。只有本次按住拥有的启动会被取消，按钮或全局开关启动的持续收音保持不变。中英文帮助补上切离窗口停止的说明。

产品改动仅 `App.tsx` 的按键 effect 和两条帮助文案，没有更改 ASR、原生采集、保存或全局快捷键注册。见[本批补丁](evidence/implementation/hold-focus/product-and-tests.patch)。

## 验证与复现

独立 Xvfb/Openbox 和 D-Bus 中运行可见 Chromium，通过 `connectOverCDP({noDefaults:true})` 连接新建空配置的默认 context，避免测试工具强制所有页面保持焦点。切到另一个浏览器页面后，记录实际 `document.hasFocus() === false`、`blur` 和 `visibilitychange → hidden`，没有注入这两个焦点事件。测试使用受控原生 IPC 和收音事件，不打开音频设备。

[旧构建](evidence/implementation/hold-focus/baseline-browser/check.json)完整执行 21 项，有 8 项不符：四种修饰键误触、失焦不停止、失焦后迟到启动、与全局开关事件冲突，以及缺少帮助说明。[最终构建](evidence/implementation/hold-focus/browser/check.json)21 项全部通过，包含：

- 裸空格只启动一次，重复键不重复启动，普通松开停止。
- 失焦无需等待 keyup 即停止；迟到启动被清理，之后 keyup 不重复停止。
- 释放等待阶段切离可收尾，再回来允许新的显式按住。
- 按钮启动的持续收音切离后仍保持；组合键与注入的原生 `mic-toggle` 事件共同到达时仍为一次持续收音。
- 输入框、弹窗按钮、合成 IME composition 标记和自由编辑画布中的空格不触发按住收音；原会议内容保持。
- 帮助说明可见，没有生产 JavaScript 错误。已查看[最终帮助截图](evidence/implementation/hold-focus/browser/hold-help-en.png)。

[原生外壳协议回归](evidence/implementation/hold-focus/shell-regression/native-shell-check.json)通过，包括退出前停止/保存屏障、尾句保存、文本/手绘/Excalidraw 撤销重做、输入预览退出和 IME 隔离；这仍是生产 App 的受控桥接验证。[Dialog 回归](evidence/implementation/hold-focus/dialog-regression.txt)也通过。15 项定向 Node、类型检查、播放器、主应用和 Linux Tauri debug 构建通过，未重跑全量 Node/Rust。

初始验证中，headless 和普通 Playwright context 强制焦点、额外 CDP session 无法撤销该设置、首次浏览器配置启动、单选按钮定位以及一次显示器分配竞争都曾使检查提前终止。这些尝试未计入最终结果；最终前后构建使用同一脚本，按顺序运行并清理独立显示器。详细运行条件见[说明](evidence/implementation/hold-focus/harness-notes.txt)及归档的运行器。专用脚本命名为 `probe-hold-focus.cjs`，避免被普通 headless `check-*` 批次误执行。

## 构建与边界

入口 `index-DjOT0n2G.js`，SHA-256 `4462809fea40bcf93a958022f6f0eb1d1411b4287230b69adb1e0aec35fdccd6`；原生二进制 SHA-256 `6925a6d18de0e502ee1316c4273b29f3b3805c8095adce099e84f04524383ca3`，确认嵌入最终入口和样式。193 个源码/配置输入一致，24 个原生文件与私有构建项目一致，见[身份记录](evidence/implementation/hold-focus/identity.json)。

实际焦点证据来自 Chromium 页面切换，不等同于 Tauri/macOS 窗口切换。全局开关是注入事件，IME 是合成 composition 标记；本批没有启动原生窗口、注册实际系统快捷键、测试真实输入法或采集音频。其余快捷键、键盘布局、VoiceOver、设备/权限与 G3/G4 仍需完整验收，C15/C62 不作全部完成结论。

私有浏览器配置、显示器、窗口管理器和预览已清理，额外扫描未发现本批环境下的残留进程；用户 1423/1430 保持原 PID。见[清理记录](evidence/implementation/hold-focus/cleanup.json)。未提交、推送或发布。
