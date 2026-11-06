# C45 Linux 原生 HTML 保存与系统交接

2026-11-03。实际 Tauri/WebKit 已完成 HTML 保存、取消、权限失败后直接重试，以及应用内“打开文件”“显示所在文件夹”。[26 项归档核对](evidence/implementation/native-export/native-check.json)通过；它们核对已捕获的运行证据，不会重新操作 UI，也不代表七格式或完整 G2/G3/G4 已通过。

本批产品源码没有新增修改。修正的是隔离显示 helper：Xvfb/Openbox 运行时明确设置并导出 `XDG_SESSION_TYPE=x11`、`XDG_CURRENT_DESKTOP=Openbox`、`DESKTOP_SESSION=openbox`。原先继承 Niri/Wayland 标识，Chromium 因找不到 Wayland 显示而退出；[原始启动诊断](evidence/implementation/native-export/before/handoff-launch-diagnostic.json)同时记录了 `xdg-open` 返回 0。因此应用继续使用“已请求系统打开”的表述，不能把启动器退出码当作窗口已出现的证明。

## 实际结果

| 路径 | 直接证据与结果 |
| --- | --- |
| 键盘输入文件名后保存 | 只输入路径，不发送 Enter。连续约 3 秒确认目标不存在，并在点击 Save 前再次确认；[点击时间与文件时间](evidence/implementation/native-export/b-keyboard-save-action.json)证明文件在点击后写入。系统保存框关闭，会议仍保留，应用显示实际路径。 |
| 系统保存取消 | 点击 GTK Cancel，已有文件集合与 SHA-256 [前](evidence/implementation/native-export/b-cancel-files-before.json)/[后](evidence/implementation/native-export/b-cancel-files-after.json)相同；原话保留，没有保存成功提示或打开入口。 |
| 真实写入权限失败 | 选择测试目录内已有 `0400` 文件，确认覆盖后 Rust 返回 `Permission denied (os error 13)`。[错误界面](evidence/implementation/native-export/b-keyboard-permission-result.png)保留会议及“Export again”；原文件的字节、哈希与权限不变。 |
| 直接重试 | 在该错误界面实际点击 Export again，在新系统保存框用键盘输入另一条路径。点击前目标持续不存在，点击后生成；[时间序列](evidence/implementation/native-export/b-keyboard-retry-save-action.json)与[成功状态](evidence/implementation/native-export/b-keyboard-retry-result.json)来自同一进程，未靠重启恢复。 |
| 打开最终文件 | 实际点击应用的 Open file 后，出现新的 Chromium 窗口。[最终截图](evidence/implementation/native-export/b-final-open-browser.png)显示 `keyboard-retry.html` 的本地 URL、会议名、原话、语义节点和蓝色底图。 |
| 显示所在文件夹 | 实际点击 Show in folder 后，Thunar 展示正确的绝对目录与 `native-control.html`；[窗口截图](evidence/implementation/native-export/b-reveal-folder.png)和[可达性路径](evidence/implementation/native-export/b-reveal-folder.json)一致。Linux 的当前实现打开父目录，不承诺选中文件。 |
| 文件已移动与恢复 | 将本次测试导出的文件改名后，Open file 明确显示文件不可用且保留重试入口；恢复原路径，再点同一入口出现实际 Chromium 窗口。见[失败状态](evidence/implementation/native-export/b-missing-open-result.json)与[恢复后截图](evidence/implementation/native-export/b-restored-open-browser.png)。 |

最终 [HTML](evidence/implementation/native-export/exports/keyboard-retry.html) 为 850,121 字节，SHA-256 `c51bd297e0fe0c850ccc98f45c442ab5fe82b5e9f029c83363c12b2ee041b684`。从实际文件解析出的[回放数据](evidence/implementation/native-export/export-content.json)保留一页、一句 `EXPORT-NATIVE-CONTROL`、`Retained native board` 节点及原始 SVG 底图字节。八份实际生成的 HTML 完全相同，归档仅保留最终文件；其余文件的原始大小、哈希和修改时间保留在[文件清单](evidence/implementation/native-export/export-inventory.json)，原文件仍在两个运行目录中。

## 自动化异常与证据修正

本轮先尝试给 GTK 文件名控件调用 Cua `set_value`，再分别通过桌面、窗口像素或可达性按钮保存。不能把这些尝试算作正常 Save 按钮验收：

- 首个旧目录桌面对照的文件修改时间早于预定 Save 点击约 10.7 秒。新目录的 `retry-control.html` 也在尚未发出 Save 点击时就已经生成，前置断言因此停止了点击。所有使用这条字段赋值路径的初步对照均不计入正常保存验收；文件存在和工具返回码不足以证明是哪次动作提交了保存。
- 一次前台点击返回 `foreground_unavailable`/“未发送输入”，之后文件存在。该次写入发生在调用期间；结合上述提前提交，不能据此认定工具误报，或认定那次点击实际执行。另一次窗口点击的原包装器在保存 stdout 前断言，完整错误文本缺失，未补造其内容。
- 新进程首次导出后出现了无标题覆盖确认框。Cua `list_windows` 没有列出它；X11 客户端列表和[可达性树](evidence/implementation/native-export/b-untitled-modal.json)确认它询问是否替换已生成的文件，并挂在旧 Save File 窗口下。它拦住了后续导出菜单。取消该框后，[同一进程重新打开导出菜单](evidence/implementation/native-export/b-after-modal-cancel.json)，已有 HTML 哈希不变。
- 对已结束保存框做截图或前置的早期探查没有复现重新映射。无标题框的一次截图为黑色，不作为可见内容证据；其身份来自 AX 和 X11。其他窗口截图里的黑色光标块也保留为捕获限制。

最终通过的两次保存改用真实键盘输入、等待并检查目标仍不存在，再点击实际系统 Save；记录文件修改时间及全部 X11 客户端窗口，包含无标题窗口。权限失败后的重试也使用这条路径。输入已有文件时 GTK 会出现补全列表：先关闭补全并重新观察，再提交保存，未把关闭补全的点击算成保存。

这些观察缩小了异常范围，尚未证明提前提交与残留框究竟来自 Cua 的字段写入、GTK 或两者交互；没有为此修改产品对话框实现，也不宣布此前残留问题已修复。此前仅记录保存后状态、没有保存前目标不存在证明的对照，不能单独排除这类问题。

## 构建、隔离与复查

- 使用与 [C37 原生删除](native-delete-validation.md)相同的二进制 `930aa63794e6148fab013964970be6d12187b8c277a92559775fbdfad7c8fb89`，内嵌 `index-Dcpz5Vs4.js`。开始与结束核对的 189 个产品源码/配置文件未变化；[当前身份核对](evidence/implementation/native-export/source-check.json)与[原构建核对](evidence/implementation/native-export/build-source-check.json)均归档。
- 两个独立运行目录为 `/home/hsiangnianian/.cache/boardcast-g2-export-z_6ei96q` 与 `/home/hsiangnianian/.cache/boardcast-g2-handoff-bmq_ehe9`。后者复制前者正常退出后的测试数据，记录 revision 已从 1 变为 2；不是把两个初始存储文件声称为逐字节相同。导出内容相同有实际文件哈希证明。
- 使用私有 Xvfb、D-Bus、AT-SPI、XDG 数据/配置/缓存。私有配置明确选 Chromium 打开 HTML/PDF、Thunar 打开目录；只验了 HTML 的默认应用交接，没有修改用户桌面的文件关联，也没有把 PDF 配置项当作 PDF 已验。版本见[运行版本](evidence/implementation/native-export/versions.json)。
- 所有输入均为测试会议；未启动录音或模型。默认浏览器截图证明实际打开和可见内容，不替代独立禁网测试。完整七格式、多页与自由对象的浏览器内容检查仍以 [B4 报告](b4-export-acceptance.md)为准。
- 两个原生应用经正常菜单退出，均观察到退出状态 0；已关闭本轮浏览器/文件管理器窗口，私有显示与 helper 进程清理完成，[剩余进程扫描](evidence/implementation/native-export/owned-process-scan.json)为空。没有提交、推送、发布或安装依赖。

复查命令只读归档，输出 JSON，不会重新操作原生窗口或改写证据：

```sh
python scripts/ui/check-native-export.py docs/ui-ux-audit-2026-11-03/evidence/implementation/native-export
```

本批只修改测试 helper、证据检查和文档，未重复 Node/Rust 全量测试。实际 helper 启动、两轮正常退出、Python 语法、26 项证据核对与归档哈希/链接检查通过。其余格式的原生保存/打开、磁盘满和覆盖旧导出期间中断、macOS/Windows 对话框与默认应用、真实音频/VoiceOver/签名安装及 G4 用户试用仍未完成。
