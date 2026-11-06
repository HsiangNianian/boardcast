# 输入区、说话人和切会草稿保护

本批核对方案 C06–C08：文字输入先成为原话、输入区折叠和焦点、说话人输入边界。实际修复范围为 `App` 的忙碌状态传入 `Composer`，没有更换输入组件、模型或保存协议。

## 发现并修复的问题

切会正在等待保存确认时，原输入框仍能接收回车。`App.say` 会拒绝这个时间点的手动输入，但 `Composer.send` 仍清空草稿，导致文字既没有进入会议，也不再留在输入框。[修复前报告](evidence/implementation/composer-busy-before.json) 保留了该失败、空草稿和旧生产入口；[截图](evidence/implementation/composer-busy-before.png) 保留现场。

现在沿用 `transitionBusy`，在切会、退出或维护操作等待期间让输入框只读，发送、收音与姓名操作禁用；发送处理本身也检查禁用状态，回车不会清空草稿。折叠和展开仍可用。结束等待后恢复原有操作：切会成功可把保留的草稿发送到新会议；保存失败则在原会议继续编辑。已经接收的原话仍按现有保存和重试流程保留。

## 当前验证结果

生产预览 `http://127.0.0.1:1430`，最终入口 `assets/index-DGcN1TIQ.js`。实际 React App 通过受控 Tauri 存储和模型回复运行，不写用户会议目录。完整源码哈希与结果身份见 [构建记录](evidence/implementation/composer-build.json)。

| 范围 | 实际验证 | 结果 |
| --- | --- | --- |
| 折叠与焦点 | 折叠保留按钮焦点，`/` 展开并聚焦，未发送草稿在折叠中保留，展开/收起偏好各自重载保持 | 通过 |
| 模态与快捷键 | 输入框里的 `/` 是普通文字；姓名 Dialog 中不能偷焦点或改草稿；Escape 关闭后回到触发按钮 | 通过 |
| 说话人边界 | 点击轮换、ASCII/全角 @、12字日文名、13字前缀按原文保留、空格分隔、A + CRLF复制格式、仅全角名字不添加空原话 | 通过 |
| 空输入与多行 | 空白禁发、空白回车不清内容，Shift+Enter不发送，Enter把多行作为一条原话保存 | 通过 |
| 组合输入 | Chromium `Input.imeSetComposition` 创建中文/日文组合文本；受控 `isComposing` 回车不发送；提交组合后下一个正常回车只发一次 | 通过，非原生输入法验收 |
| 长文本 | Chromium剪贴板粘贴49,499字英文/中文/日文，输入框有高度上限和内部滚动，完整原话保存无截断 | 通过 |
| 紧凑窗口 | 720×480与640×400下长输入不横溢，键盘可聚焦且看到发送按钮；输入高度分别120/56px | 通过 |
| 原话与模型 | 延迟模型回复期间原话已保存；模型失败保留原话；逐句重试更新同一个ID、不重复追加 | 通过 |
| 切会草稿 | 暂停保存回复后回车保留草稿；成功后仅新会议接收；保存失败后留在原会并恢复编辑；存储恢复后原话和草稿都能保存 | 通过 |

[机器报告](evidence/implementation/composer-check.json)：**40项通过，0个浏览器脚本错误**。附图：[长输入](evidence/implementation/composer-long-paste.png)、[保存期间只读草稿](evidence/implementation/composer-saving.png)。

同一入口复跑收音切会27项、原生外壳协议、逐句失败重试和内容回放；均通过。[完整运行日志](evidence/implementation/composer-ui.txt) 与 [构建日志](evidence/implementation/composer-build.txt) 已归档；类型检查、脚本语法和diff空白检查通过。

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-composer.cjs check-recording-switch.cjs check-native-shell.cjs check-line-retry.cjs check-content-replay.cjs
```

## 验证边界

这些检查没有使用 macOS 原生中日文输入法或候选窗口、VoiceOver、真实麦克风或真实模型。Chromium合成组合按键不能证明特定平台的候选确认事件时序，C06/G3这一部分仍未完成。未发送草稿仍是当前窗口内的内容；此修复不增加关闭应用或刷新后的草稿持久化，也不把保存协议夹具等同于磁盘故障验收。其他C01–C68变体与G2/G3/G4门槛继续按完整方案推进。
