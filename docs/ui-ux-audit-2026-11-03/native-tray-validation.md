# C14/C61/C62 续批：Linux 托盘、后台快捷键与 CLI

2026-11-04，HEAD `4f9ab26`。使用独立 Xvfb、Openbox、D-Bus、AT-SPI、真实 Tauri/WebKit 以及 stalonetray 验证。托盘宿主从 [Debian 官方包目录](https://deb.debian.org/debian/pool/main/s/stalonetray/)下载后解包到测试目录，没有安装系统包。所有键盘、鼠标和剪贴板输入均通过 Cua 进入私有桌面，未操作用户桌面。

## 修复与前后对照

在设置的全局快捷键录入框按当前的 Ctrl+Shift+Space，旧构建实际触发收音并将模型准备弹窗叠在设置上。见[原生失败截图](evidence/implementation/native-tray/baseline/record-current-shortcut-result.png)及[旧生产构建回归失败](evidence/implementation/native-tray/baseline-browser/run.txt)。现在只有应用获得焦点且焦点处于快捷键录入框时，忽略该收音事件。录入框仍保留原组合，不暂停或改写系统注册；[最终原生截图](evidence/implementation/native-tray/final/record-current-shortcut-result.png)确认没有再启动收音准备。

旧构建在窗口最小化后接收到快捷键或 `--toggle-mic`，但缺少语音模型时，恢复提示仍留在最小化窗口内。现在 `toggleMic` 返回现有开始/停止操作的结果，失败时通过一个原生命令复用已有的显示窗口函数。成功开始或停止不主动显示窗口。没有更改语音模型、录音算法、快捷键注册方式或退出保存协议。

## 真实原生结果

- 默认 Ctrl+Shift+Space 在私有桌面上触发，窗口从最小化恢复并显示语音模型准备页；即使最小化前焦点仍在快捷键录入框，也不会被误判为正在录入。见[恢复后的完整桌面](evidence/implementation/native-tray/final/background-global-settled.png)。
- Backspace 将快捷键设为 Off，原组合不再唤起窗口。此时再次执行同一应用的 `--toggle-mic` 仍返回 0，并在原实例中显示[模型准备页](evidence/implementation/native-tray/final/cli-toggle-result.png)。没有下载模型或启动真实采集。
- 将组合改为 Ctrl+Shift+K 后，旧 Ctrl+Shift+Space 不再触发，新组合可从后台唤起模型准备页。见[设置值](evidence/implementation/native-tray/final/shortcut-rebound.png)及[实际恢复窗口](evidence/implementation/native-tray/final/rebound-hotkey-settled.png)。
- 实际点击托盘“Start listening / Show window / Quit”，并切换中文后确认菜单成为“开始收音 / 显示窗口 / 退出”。旧构建的正常第二实例启动能够显示原窗口；最终构建的中文托盘“开始收音”也会在后台失败时显示[中文准备页](evidence/implementation/native-tray/final/tray-recovery-zh.png)。
- 先实际输入并保存 `TRAY_SAVED 原来已保存的内容`，再把私有会议目录设为不可写，输入 `TRAY_PENDING 退出前必须保存这一句`。真实 EACCES 下旧文件字节不变。窗口最小化后点击托盘“退出”，应用恢复窗口、保留两句内容并拒绝退出，见[保存保护截图](evidence/implementation/native-tray/final/quit-blocked-desktop.png)。恢复原目录权限，再次从托盘退出，应用正常退出，实际 JSON 保存两句原话，revision 从 1 变为 2。
- 将已退出的私有数据复制到新的隔离会话后，新的原生进程恢复两句原话、中文界面和 Ctrl+Shift+K 设置，会议文件字节相同。重新聚焦快捷键录入框，再主动点击托盘“开始收音”，[仍正常打开准备页](evidence/implementation/native-tray/reopen/tray-focused-result.png)，不会被新增焦点保护拦截。

退出重试后，第一句原话对象逐字段相同，会议 ID、标题、创建时间和白板相同。第一条 history 在保留原内容的同时补齐了该句已有的 `ms/ops/failed/boardFailure` 元数据，因此没有把 history 的完整字节不变列为通过条件。

[39 项归档核对](evidence/implementation/native-tray/native-tray-check.json)通过。[复核脚本](evidence/implementation/native-tray/verify-native-tray.py)只读取已捕获的窗口、进程、保存文件及生产检查结果，不模拟原生动作。录入框的 Off / Ctrl+Shift+K 值和中英文托盘菜单另有实际查看的截图。归档附 [SHA-256 清单](evidence/implementation/native-tray/manifest.json)。

## 构建、回归与预览

生产浏览器的原生协议检查覆盖：录入快捷键时不开始也不停止收音，成功开始/停止不显示窗口，启动失败、停止失败和缺少模型时各显示一次窗口，并继续通过既有保存失败、退出握手、文本/画布撤销重做、全选及模型删除保护。见[最终结果](evidence/implementation/native-tray/build/browser/native-shell-check.json)。这些成功采集和停止失败是受控协议 fixture，不是硬件音频证明。

类型检查、4 项 Node 快捷键单测、2 项 Rust 退出握手单测、前端生产构建、原生 debug 构建、`shell.rs` 格式及 `git diff --check` 通过。额外在 Vite dev 上尝试完整协议脚本时，新收音检查通过，但既有“新会议→会前准备”步骤超时；[开发态探测日志](evidence/implementation/native-tray/dev-probe/run.txt)保留，不计为完整 dev 回归通过。正式回归使用生产构建，当前两种预览的普通页面 smoke 均通过。

最终原生二进制 SHA-256：`af715b493b4efdad78ed76cca76abf0f5552c1615add2551e27b5a422eb6a530`。前端入口 `/assets/index-BzmO2V3X.js`，SHA-256：`bbed9c47a4e6a78db36eeea346e9792515b87d50ed31986472036a0722481376`。[构建身份及源码哈希](evidence/implementation/native-tray/build/identity.json)已归档。

1430 已更新为同一前端，保留旧哈希资源并最后替换入口。1423/1430 均通过新浏览器 context 的中文输入区与设置打开检查，没有脚本或请求失败，见[预览结果](evidence/implementation/native-tray/build/preview-check.json)。三个私有原生会话均正常退出，目录权限已恢复，进程与显示 socket 已清理；1438 测试服务关闭，1423/1430 原有服务保留。未提交、推送、安装或发布。

## 仍待验收

本批当时只验证空闲组合的关闭和替换。后续[快捷键冲突专批](native-shortcut-validation.md)已修复先注销再注册、失败偏好落盘和Linux部分占用未释放，补上真实冲突与重试及连续更改协议检查；[CLI冷启动专批](native-cli-validation.md)补上首实例 `--toggle-mic`。真实录音中的红点/停止菜单与退出尾句、原生 Wayland 及其他托盘宿主仍未覆盖。

用户已确认暂时没有测试 Mac。Mac、VoiceOver、真实音频、签名更新和 G4 用户试用继续保留；C01–C68/G1–G4 的整体目标仍在进行。
