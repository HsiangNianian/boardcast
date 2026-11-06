# C38 Linux 原生续会、附件与恢复

2026-11-05，工作树基线 `4f9ab26`。在当前前端构建上重新编译 Linux Tauri debug 应用，实际执行选板、自由编辑、磁盘故障、系统保存和进程重开。[28 项证据核对](evidence/implementation/native-continue/native-continue-check.json)及[5 项独立离线 HTML 检查](evidence/implementation/native-continue/native-html-offline-check.json)通过。后续[遮挡与保存复核](native-render-validation.md)定位了局部黑块的原生对话框遮挡原因，并修正、补齐正常 Save 的证据；以下保留本批原始过程，不将单项结果等同完整 G2 准入。

应用使用私有标识 `games.mmstudio.boardcast.g2.nativecontinue`、独立数据目录、Xvfb/Openbox 和私有 Cua driver。操作作用于实际 Tauri 窗口，保存走真实 Rust 文件存储；没有注入模拟的 Tauri API。三页源会议是受控夹具，节点和自由矩形取自前批实际 UI 生成的内容。没有修改用户会议、安装应用、连接真实音频或下载模型。

| 检查 | 结果与证据 |
| --- | --- |
| 多页选择与取消 | 选择器有 3 张板，打开后第一张有焦点；取消后返回原“Continue on it”按钮，未生成新文件。见[选板](evidence/implementation/native-continue/picker.png)、[取消](evidence/implementation/native-continue/cancel.png)及[原件哈希](evidence/implementation/native-continue/cancel-files.json)。 |
| 独立续会与来源 | 选择第 2 张板生成新的单页会议，语义图和底板均对应所选页；继承“仅本地”和术语表。原会议名、页码与淡墨说明出现在[手绘](evidence/implementation/native-continue/choose2-saved.png)和[自由编辑](evidence/implementation/native-continue/free-ready.png)中。 |
| 编辑、撤销与保存 | 原生指针选择矩形，键盘移动，Undo/Redo 分别显示 68/100 的投影透明度；实际文件始终保存原始 100，未写入临时显示标记。最终矩形位置相对源图移动 5 个场景单位，原语义节点保持。见[移动](evidence/implementation/native-continue/shape-moved.png)、[撤销](evidence/implementation/native-continue/shape-undone.png)、[重开后的文件](evidence/implementation/native-continue/final-20261105-002034-t2g8.json)。 |
| 缺图与恢复 | 暂时移走隔离源会议的 PNG，选择第 3 张板时显示[明确错误](evidence/implementation/native-continue/missing-image-error.png)，保留当前会议和选板器，文件数量不变。补回原图后同一入口成功，旧错误消失。 |
| 附件独立与删除 | 第 3 张板只有资料图和自由矩形。新会议附件位于自己的 ID 目录，PNG SHA-256 与原图一致。通过会议库确认删除源会议后，原 JSON/附件目录消失，续会图片仍在；[删除前](evidence/implementation/native-continue/before-source-delete.json)、[删除后](evidence/implementation/native-continue/after-source-delete.json)记录了实际文件哈希。此前原 JSON 和原 PNG 均未被续会或编辑改写。 |
| 真实写入失败 | 将专用会议目录设为不可写后移动矩形，实际收到 EACCES。应用显示[恢复说明](evidence/implementation/native-continue/save-failed.png)，保留当前修改，并[阻止切换会议](evidence/implementation/native-continue/failed-switch-blocked.png)。三份旧 JSON 的字节均保持。恢复权限、关闭会议库后自动保存成功，未点击额外重试按钮；保留的 1 个单位位移落盘，revision 从 1 变为 2。 |
| 退出与冷启动 | 第一次从应用菜单正常退出、重新启动后，删除原件的图片续会仍可见。两份续会的最终内容与重开后的 JSON 完全一致。[最终冷启动自由编辑](evidence/implementation/native-continue/third-reopen.png)和[手绘](evidence/implementation/native-continue/third-hand-stable.png)均显示完整内容。 |
| 原生导出与离线文件 | 操作真实 GTK [保存对话框](evidence/implementation/native-continue/html-name-ready.png)后获得 [900,040 字节 HTML](evidence/implementation/native-continue/native-continued.html)，应用显示实际保存路径。新建断网 Chromium 打开该文件，图片可解码且与原图逐字节一致，矩形、来源说明和可用画布均存在，没有 HTTP 请求或脚本错误。见[离线截图](evidence/implementation/native-continue/native-html-offline.png)。本次文件名使用 `set_value`，不能证明预定 Save 点击触发写入；可靠的键盘输入、保存前不存在及写入时间证据由[后续复核](native-render-validation.md)补齐。 |

显示异常的复核：第二次运行中，使用后台可达性操作从会议库打开节点续会后，手绘区出现[黑色矩形](evidence/implementation/native-continue/reopened-graph-clean-session.png)。结束该输入会话后仍在；一次前台指针输入后，在视图未改变的情况下[恢复显示](evidence/implementation/native-continue/reopened-graph-hand-after-pointer.png)。后续使用相同二进制复现，桌面截图和 X11 窗口几何证明它来自无标题覆盖确认框遮挡；Cua 主窗口截图将对应区域捕获为纯黑，窗口列表却遗漏了该确认框。取消确认后同一白板立即恢复，文件未改变。见[直接证据](native-render-validation.md#遮挡的直接证据)。没有据此修改产品滤镜，原始异常和当时未决的检查记录均保留。

测试工具边界：部分 tab 的后台 `select` 点击没有改变界面，均通过新截图识别，后续使用已有编辑入口或前台输入。第二次结束测试时，清理器早于退出状态确认发送了 SIGTERM，不能作为正常退出证据；数据当时已保存，随后第三次冷启动重新核对，并等待进程退出和窗口消失后才清理。第一次和最终应用退出码均为 0，见[第一次清理](evidence/implementation/native-continue/first-run/cleanup.json)、[最终退出](evidence/implementation/native-continue/third-quit-process.json)及[最终清理](evidence/implementation/native-continue/cleanup.json)。所有带本次专用数据目录的进程已结束，目录权限已恢复。

[构建身份](evidence/implementation/native-continue/identity.json)记录了原生二进制、关键源码和前端入口的 SHA-256；[构建日志](evidence/implementation/native-continue/build/native-build.log)对应 locked/offline、debug、no-bundle 构建。当前 1423/1430 预览继续运行，1430 实际返回的入口字节仍与测试构建相同。本批未改产品源码，因此没有重复前批 301 项 Node/Eval 或全量 Rust 测试。归档含 241 个文件、12,780,337 字节及独立[哈希清单](evidence/implementation/native-continue/manifest.json)。

本批与后续复核补上 Linux C38 核心原生数据、恢复和正常保存证据。全方案其余 G2 项、Mac、真实音频/模型、触摸/笔、VoiceOver、G3/G4 继续待验。整体重构未完成，未提交或发布。
