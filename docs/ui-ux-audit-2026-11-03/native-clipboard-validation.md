# C29/C61/C62 续批：Linux 原生剪贴板与画布全选

2026-11-04，HEAD `4f9ab26`。在独立 Xvfb、Openbox、D-Bus 和 AT-SPI 环境中运行真实 Tauri 二进制，使用系统剪贴板、原生菜单、鼠标和键盘验证。所有输入都是测试文字；用户桌面及其剪贴板未参与测试。

## 修复

旧构建的原生“粘贴”没有效果，Ctrl+V 可以正常输入。已分别通过可访问菜单调用和实际鼠标点击复现。当前 Tauri/Muda 的 GTK 预定义剪贴板命令需要可选的 libxdo 功能；启用后复制、粘贴和全选有效，但鼠标点击“剪切”仍无效，直接 Ctrl+X 则正常。[Tauri 功能说明](https://docs.rs/tauri/latest/tauri/)与锁定依赖源码支持该功能边界；[失败对照](evidence/implementation/native-clipboard/baseline/paste-desktop.png)和[剪切对照](evidence/implementation/native-clipboard/libxdo-attempt/cut-mouse.png)保留在归档中。

最终 Linux 的 Cut/Copy/Paste 调用现有 WebKit 的[原生编辑命令](https://webkitgtk.org/reference/webkit2gtk/stable/method.WebView.execute_editing_command.html)。Cargo 只把已经被 Tauri 使用的 `webkit2gtk` 声明为 Linux 直接依赖，没有升级包版本或引入新的传递依赖；最终二进制不链接 libxdo。Mac/Windows 继续使用原有预定义菜单。

全选复用应用已有的撤销/重做分发：输入框内选择文本，弹窗外的自由编辑模式交给 Excalidraw。这也修复了中间版本直接执行 WebKit SelectAll 时把 Ctrl+A 变成整页文字选择的回归。[失败截图](evidence/implementation/native-clipboard/webkit-attempt/canvas-keyboard.png)与[最终画布选择](evidence/implementation/native-clipboard/final/canvas-keyboard.png)均已查看。没有新增剪贴板读写 IPC 或自建键盘注入服务。

## 最终构建的实际结果

- 画布中保留两块独立绘制的矩形，真实 Ctrl+A/C 和中文原生“全选/复制”均得到包含这两个对象的 `excalidraw/clipboard` 数据。每次复制前先写入不同哨兵，排除了旧剪贴板造成的假通过。
- 中文原生剪切后实际会议文件中存活对象数为 0；粘贴后为 2，撤销一次为 0，重做一次恢复这两个粘贴对象。原话始终只有原来的一条。
- 英文原生粘贴、全选、复制、剪切完整保留 `CLIPBOARD_第一行 €120` 与第二行中文、表情；剪切后发送按钮禁用。输入框获得焦点后，这些操作没有删除仍被选中的画布对象。
- 鼠标点击英文原生 Paste 恢复未发送草稿。会议标题弹窗内全选、复制、剪切、Ctrl+V 和 Enter 保存正常，剪贴板只有标题内容；保存只更新标题及保存元数据，原话、历史、画布均保持一致。
- 关闭标题弹窗后，用真实 Ctrl+A/C 读回的底部草稿逐字一致。Ctrl+X、原生 Undo/Redo、Ctrl+V 依次清空、恢复、清空、恢复输入框，[最终草稿截图](evidence/implementation/native-clipboard/final/final-draft.png)与实际会议文件均已核对，没有误发草稿或改动画布。

[41 项归档核对](evidence/implementation/native-clipboard/native-clipboard-check.json)全部通过。[复核脚本](evidence/implementation/native-clipboard/verify-native-clipboard.py)只读取已捕获的系统剪贴板、窗口状态和会议 JSON，不模拟原生结果，也不重新驱动应用。`baseline`、`libxdo-attempt`、`webkit-attempt` 保存失败对照；`final` 保存最终构建与完整操作结果。

归档共 196 个产物、4,512,225 字节，另有[SHA-256 清单](evidence/implementation/native-clipboard/manifest.json)。

## 构建、回归与预览

新增的生产浏览器检查在旧构建上因输入框未全选失败，在最终构建上通过；同时验证画布全选后删除、一次撤销恢复，以及既有的撤销/重做、退出和保存保护流程。见[旧构建结果](evidence/implementation/native-clipboard/baseline-browser/run.txt)与[最终结果](evidence/implementation/native-clipboard/final/browser/native-shell-check.json)。类型检查、前端构建、原生 debug 构建、4 项快捷键单测、2 项 Rust 退出握手单测通过。

修改的 `shell.rs` 格式检查与 `git diff --check` 通过。全仓 `cargo fmt --check` 仍报告 `arxiv.rs`、`llm.rs`、`meetings.rs`、`models.rs` 的既有格式差异，本批未改动这些文件。

应用标识 `games.mmstudio.boardcast.g2.nativemenu`，不打包。最终二进制 SHA-256 为 `def683c3d228404ee4da321d3675c0f13af930ca768e0bb6f9645fcb2d47416f`；前端入口 `index-wWyi3ShH.js`，SHA-256 为 `696c3295397a9885068a18e538ac19526f5ae345d13db0f56ce53e2318b5df0d`。[构建身份](evidence/implementation/native-clipboard/final/identity.json)、原生 24 份源文件哈希与本批修改文件快照已归档。

1430 已更新为相同前端，旧哈希资源保留、入口最后替换。1423/1430 均通过新浏览器 context 的中文输入区和设置打开检查，无脚本或请求失败，见[预览记录](evidence/implementation/native-clipboard/final/preview-check.json)。四轮隔离桌面均正常退出，进程和 socket 已清理；1438 测试服务关闭，1423/1430 原有进程保留。没有提交、推送、安装或发布。

## 未验证范围

这是 Linux X11 的文本、两对象画布及单个标题弹窗验收，未覆盖原生 Wayland、图片/文件剪贴板、全部输入字段、真实 IME 或辅助技术。用户已确认暂时没有测试 Mac；Mac、VoiceOver、真实音频、签名更新和 G4 用户试用继续保留。

未发送草稿只验证同会内操作隔离，没有新增重启持久化承诺。C01–C68/G1–G4 的整体目标仍在进行，不能据此关闭完整平台或无障碍验收。
