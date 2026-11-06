# C38 Linux 遮挡复核与正常保存

2026-11-05，工作树基线 `4f9ab26`。[前批黑色矩形](native-continue-validation.md)已复现并定位：无标题的 GTK 覆盖确认框遮住了主窗口，Cua 的主窗口截图将被遮挡区域捕获为纯黑。取消该确认框后，同一进程、同一白板立即恢复；本批没有修改产品渲染代码。另用真实键盘输入补验正常 Save，独立离线回放与证据核对共 [12 项通过](evidence/implementation/native-render/render-check.json)。

## 遮挡的直接证据

重复前批“导出图片续会，再从会议库打开节点续会”的操作后，[主窗口截图](evidence/implementation/native-render/graph2-window.png)出现黑块，而同一状态的[桌面截图](evidence/implementation/native-render/graph2-desktop.png)明确显示“render-repro.html 已存在，是否替换”。[X11 窗口记录](evidence/implementation/native-render/confirm-x11-windows.json)将它关联到同一应用 PID 和旧 Save File 窗口；外框为屏幕坐标 `(421, 408, 598, 183)`，减去主窗口客户区原点 `(80, 108)`，恰好对应黑块 `(341, 300, 598, 183)`。

该矩形在主窗口截图中的 109,434 个像素全部为纯黑，在桌面截图中没有纯黑像素。Cua [窗口列表](evidence/implementation/native-render/all-windows-with-confirm.json)遗漏了这个无标题框，但按 X11 找到的窗口 ID 获取[截图和可达性树](evidence/implementation/native-render/overwrite-dialog.json)成功。点击该框的 Cancel 后，[同一白板恢复显示](evidence/implementation/native-render/uncovered-desktop.png)，已导出 HTML 的字节、哈希和修改时间均保持。主窗口列表只有一个条目，不能单独证明原生模态框已关闭。

## 保存证据的修正与补验

前批原生 C38 和本批首次复现均使用了文件名控件的 `set_value`。这条路径已经在[此前导出报告](native-export-validation.md#自动化异常与证据修正)中记录过提前生成文件和残留覆盖框的问题。前批 900,040 字节 HTML 的内容及离线检查仍有效，但不能据此认定预定 Save 点击触发了写入。本次修正该结论；保留原始归档及其当时的检查结果。

| 正常保存检查 | 实际结果 |
| --- | --- |
| 键盘填写 | 新打开 GTK Save File，以 Ctrl+A 和实际键盘输入填写完整路径，不调用 `set_value`。[保存前截图](evidence/implementation/native-render/clean-before-save.png)与[7 次目标不存在采样](evidence/implementation/native-render/clean-before-save-absence.json)覆盖 3 秒等待。 |
| 明确提交 | 按观察到的位置点击 Save；[文件记录](evidence/implementation/native-render/clean-save-file.json)显示修改时间在该动作开始与结束之间，产物为 [941,288 字节 HTML](evidence/implementation/native-render/keyboard-continued.html)。应用显示实际路径，[X11 可见客户端](evidence/implementation/native-render/clean-saved-x11-clients.json)仅剩主窗口，无覆盖确认框。 |
| 独立离线打开 | 新建断网 Chromium 打开产物，[实际截图](evidence/implementation/native-render/keyboard-continued-offline.png)显示第 2 页来源、两个淡墨节点及正常墨迹的已编辑矩形。内嵌 SVG 解码成功，画布高度大于 400 像素，无未解析附件引用、HTTP 请求或脚本错误。 |
| 数据与退出 | 测试会议文件哈希前后相同；应用经菜单正常退出，先确认进程退出和窗口清空，再结束私有显示及 helper。应用退出码为 0、私有 socket 已删除、专用数据目录关联进程为空，见[最终状态](evidence/implementation/native-render/final-status.json)。 |

本批复用[前批相同的二进制和前端](evidence/implementation/native-render/identity.json)，独立复制测试数据，使用私有 Xvfb/Openbox/Cua 环境。关键源码、原生二进制及 1430 实际提供的入口哈希均未改变；1423/1430 原进程继续提供预览。归档包含 109 个文件、6,412,655 字节及独立[哈希清单](evidence/implementation/native-render/manifest.json)，不含应用二进制或用户数据。

本批关闭这项遮挡原因调查，补齐正常 Save 的时间顺序证据；`set_value`/GTK 提前提交的内部原因仍未定位，也未声称修复自动化工具。没有产品源码变更，未重复前批 301 项 Node/Eval、Rust 全量测试或重建应用。Mac 暂无测试机，真实音频/模型、触摸/笔、VoiceOver、签名更新及 G4 用户试用仍未验；完整 UI/UX 重构继续，未提交或发布。
