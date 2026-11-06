# C32/C67：内容列表翻页与输入法改名

2026-11-05，基线 `4f9ab26`。接续[内容列表与复盘](contents-review-validation.md)，落实方案 §5.3 的键盘替代操作及 §9 的输入法保护。

## 修改

内容列表按会议和页码重新挂载，用于隔离改名草稿；页码选择器也随之被移除，键盘翻页后焦点落到背景。现在保留这层隔离，在下一帧将因重建而丢失的焦点交回可见的新选择器。若用户已把焦点移到别处，则不抢回焦点；隐藏的选择器也不接收恢复。

语义对象和自由对象共用的改名框补上组合输入保护：组合期间 Enter 不提交表单，Escape 不取消改名；组合结束后仍可正常保存或取消。对象类型和固定资料说明从 11px 调整到方案要求的 12px。

仅修改 2 个产品文件，新增 1 个生产界面检查脚本；`App.tsx` 的按页隔离、存储、IPC 和数据格式未变。见[本批补丁](evidence/implementation/content-keyboard/product-and-test.patch)与[文件哈希](evidence/implementation/content-keyboard/changed-files.json)。

## 浏览器验证

首次[旧构建检查](evidence/implementation/content-keyboard/baseline/check-content-navigation/report.json)执行 45 项，其中 30 项不符，覆盖翻页失焦、组合输入处理及类型说明字号；已查看[翻页后失焦](evidence/implementation/content-keyboard/baseline/check-content-navigation/failure-1.png)和[组合 Escape 提前关闭改名](evidence/implementation/content-keyboard/baseline/check-content-navigation/failure-5.png)。随后加强自由对象归属和滚动区域裁剪断言，并增加已转移焦点不被抢回的检查。

| 检查 | 最终结果 |
| --- | --- |
| 新增生产界面 | [47 项通过](evidence/implementation/content-keyboard/browser/check-content-navigation/report.json)：方向键、Home/End、末页空板、跨页同 ID、草稿取消、组合事件、正常提交、自由对象命名和实际 IndexedDB 保存。中英两种语言，1280×800 / 900×600 / 720×480 / 360×240；确认选择器在实际滚动区域内可见。 |
| 真实浏览器缩放 | [17 项通过](evidence/implementation/content-keyboard/browser-zoom.json)，使用 `chrome.tabs.setZoom`，核对 100% / 200%、DPR 和 CSS 视口；连续 End/Home 不重新聚焦也能翻页。已查看[中文 200% 小窗口](evidence/implementation/content-keyboard/zoom/zoom-zh-720-2-light.png)和[英文暗色](evidence/implementation/content-keyboard/zoom/zoom-en-1280-2-dark.png)。 |
| 原有流程 | [内容列表 4 组流程](evidence/implementation/content-keyboard/browser/check-contents/contents-check.json)及[内容与回放 31 项](evidence/implementation/content-keyboard/browser/check-content-replay/content-replay-check.json)通过，保留来源定位、撤销/重做、自由对象重开和回放只读保护。 |
| 构建 | TypeScript、[完整前端构建](evidence/implementation/content-keyboard/build.log)、[Linux 原生离线锁定构建](evidence/implementation/content-keyboard/native-build.log)及 `git diff --check` 通过。原生内嵌前端与 dist 的 11575 个文件相同，见[构建身份](evidence/implementation/content-keyboard/identity.json)。没有重复运行完整 Node/Eval 或 Rust 单测。 |

## 真实 Linux 键盘、Rime 与冷启动

使用独立应用标识、XDG、Xvfb、D-Bus 和 Fcitx5/Rime，真实 Tauri/WebKit/GTK。三张合成白板中，前两张分别包含相同 ID 的节点和自由矩形，第三张为空；没有启动物理收音或模型。

1. 从内容 tab 按 Tab 进入页码选择器，连续 Down、End、Home 切到第二页、末页、首页；每次页面重建后可继续键盘操作。缩到 [720×480](evidence/implementation/content-keyboard/native/native-compact-home-after.png) 后，Shift+Tab 返回选择器，连续翻页仍正常，底部收音入口可见。
2. 改名时真实键入 `nihao`。组合中的 [Escape](evidence/implementation/content-keyboard/native/native-ime-cancel-after.png) 保留原值和表单；[Enter](evidence/implementation/content-keyboard/native/native-ime-enter-after.png) 只提交拼音文字到输入框，没有提前改名。重新输入并按空格选出[“你好”](evidence/implementation/content-keyboard/native/native-ime-chinese-after.png)，再按一次 Enter 才保存，焦点返回重命名按钮。
3. 普通 Escape 丢弃后续草稿；通过 Tab 进入自由对象改名，保存为 `named`。实际文件确认只有第一页的名称变化，第二页同 ID 的节点图内容和原始自由矩形不变。
4. 正常退出再[冷启动](evidence/implementation/content-keyboard/native/cold-restored.png)，两种名称均恢复。查看内容并退出后的完整会议文件与冷启动前字节相同，SHA-256 为 `309c0f5735f84c3678bd38bd5c721f0888cef4b741970da1c48031e789465eca`。

[19 项原生证据核对](evidence/implementation/content-keyboard/native-proof.json)保留窗口、操作、内容和文件依据。合成初始场景只有自由矩形，正常渲染会补入派生的节点元素，因此不把完整 scene 与初始稀疏 fixture 的差异当作串页；按 ID 检查原始自由对象所有字段，并另核对图内容和完整冷启动文件。初次冷启动观察早于启动动画结束、AX 将类型与名称合并等测试观察差异见[调查说明](evidence/implementation/content-keyboard/investigation.json)。部分截图含测试光标覆盖层，未编辑截图。

## 预览与边界

原 1423 / 1430 预览进程保留，1430 已更新到 `index-CF6dViZT.js` 并与被测构建核对，见[预览记录](evidence/implementation/content-keyboard/preview-check.json)。两次原生运行的 14 个注册进程均退出，私有显示 socket 已移除。

真实输入法证据限于 Linux Rime 中文；浏览器中的日文字符串不等于日文输入法验收。Mac、VoiceOver、其他原生输入法、物理音频、签名更新和 G4 新用户试用仍待完成，完整重构继续。没有提交、推送、安装或发布。
