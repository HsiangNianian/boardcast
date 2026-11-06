# C34/C61/C62/C67：Linux 中文输入与标题编辑焦点

2026-11-05，基线 `4f9ab26` 的当前工作树。真实 Fcitx5/Rime 检查发现：退出标题编辑会丢失键盘焦点，Linux WebKit 在取消时还可能直接终止。空标题和保存失败也会让焦点离开输入框。本批修复这些编辑路径；Mac、日文输入法和读屏验收仍保留。

只改 `MeetingsPanel.tsx` 和既有会议库检查。取消时先将焦点移到同一编辑行内的按钮，结束文本输入而不触发离开编辑行的自动保存，再移除输入框。编辑结束、保存解除忙碌后回到原“重命名”按钮；空标题和保存失败则回到保留的草稿。复用原有 Dialog、保存事务和错误提示，没有新增依赖。

| 验证边界 | 实际结果与证据 |
| --- | --- |
| 真实中文原话 | Rime 候选期间 Enter 未发送，空格选词未开始收音，下一次 Enter 才保存一句“你好”。见[候选截图](evidence/implementation/native-ime/chinese-candidates-desktop.png)、[组合输入记录](evidence/implementation/native-ime/composition-boundary.json)和[文件](evidence/implementation/native-ime/first-sentence.json)。 |
| 真实说话人输入 | 组合期间 Enter 不提前改名，第一次 Escape 取消候选，下一次关闭弹窗；选择“小明”并明确确认后更新原句说话人，焦点回到入口。见[结果](evidence/implementation/native-ime/speaker-result.json)和[保存文件](evidence/implementation/native-ime/speaker-saved-meeting.json)。这两项在改动前构建执行，对应组件本批未改。 |
| 崩溃对照 | 旧版在中文输入及关闭输入法后的普通 Escape 都出现 SIGABRT。仅在卸载后恢复焦点的第一版仍崩溃。两个最小 HTML 弹窗对照均未崩溃，未据此修改共享 Dialog 的语义。见[对照结果](evidence/implementation/native-ime/crash-control-result.json)、[普通输入栈](evidence/implementation/native-ime/ordinary-crash-main-thread.txt)和[第一版日志](evidence/implementation/native-ime/fixed-runtime/app.txt)。 |
| 最终原生取消 | 普通 Escape、组合期间 Enter、取消候选后的第二次 Escape、真实指针点击取消均完成；取消没有写入草稿，原文件仍为 revision 2。再次 Enter 可直接重开编辑器。见[结果](evidence/implementation/native-ime/input-focus-cancellation-result.json)、[焦点截图](evidence/implementation/native-ime/input-focus-escape-after.png)和[指针动作](evidence/implementation/native-ime/input-focus-pointer-cancel.json)。 |
| 保存与失败恢复 | 选词“回忆”并确认，标题保存为 revision 3；空标题提示后输入框保持焦点。临时将隔离测试目录设为不可写，真实 EACCES 保留原文件和 `nihao` 草稿；恢复权限后直接 Enter 保存为 revision 4，再次 Enter 重开编辑。见[失败截图](evidence/implementation/native-ime/input-focus-storage-error-ready.png)、[权限结果](evidence/implementation/native-ime/input-focus-storage-error-result.json)和[重试文件](evidence/implementation/native-ime/title-retried-meeting.json)。 |
| 冷启动与退出 | 最终再用 Rime 保存标题“回忆”，revision 5，原话及说话人保持。两次应用正常退出码均为 0；同一最终二进制冷启动恢复标题、原话、说话人和 Saved 状态，会议文件逐字节不变。见[冷启动界面](evidence/implementation/native-ime/cold-input-restored.png)、[结果](evidence/implementation/native-ime/cold-input-result.json)和[退出后哈希](evidence/implementation/native-ime/after-cold-quit-files.json)。最终两次运行没有 WebKit 终止日志。 |

既有浏览器会议库检查从 14 项补到 20 项。旧版先失败于取消后失焦，第一版再失败于空标题失焦，[最终 20 项](evidence/implementation/native-ime/browser-input-focus/check-library/library-check.json)通过且无页面错误。共享 Dialog 的[回归](evidence/implementation/native-ime/dialog-regression.log)在开发 fixture 的 1423 端口通过；此前误用生产端口的超时保留为测试环境错误。前端类型检查/双构建、原生 locked/offline 构建和差异检查通过。Rust 产品代码未改，没有重复无关 Rust 全量测试。

归档的 [28 项核对](evidence/implementation/native-ime/evidence-check.json)检查已有记录的一致性，也包含预期失败对照，不能替代再次运行桌面。194 个构建输入与最终产物一致，只有会议库组件改变。实际 WebKitGTK 2.52.6、GTK 3.24.52、Fcitx5/Rime 版本、产物哈希和仍运行的 1423/1430 预览见[身份记录](evidence/implementation/native-ime/identity.json)。归档不含二进制、大型构建产物或用户词典，见[清单](evidence/implementation/native-ime/manifest.json)。

调试记录保留以下限制：最初附加 GDB 被 ptrace 拒绝，随后从测试进程启动并跟踪其网页子进程，没有修改系统权限；早期跟踪全部 fork 会阻塞启动，未计为产品失败。取得了用户态 SIGABRT 栈，但缺少 WebKit 调试符号，内部根因未确定。一次未等页面就绪的探针明确无效；输入框值未包含在可访问名称、Rime 未激活时产生普通 `nihao` 草稿等断言问题，均通过截图和实际文件重新核对。两个独立 HTML 对照没有应用退出监听，测试结束时有意 SIGTERM，不作为产品正常退出证据。

所有输入、目录权限故障及词典写入均在私有测试目录和显示内；[66 个登记进程](evidence/implementation/native-ime/process-cleanup.json)已结束，没有私有环境进程残留，原预览进程保持。

**仍未验证：**其他 WebKit 输入触发、日文及其他输入法、原生 Wayland、Mac/VoiceOver、物理音频、60 分钟现场及 G4 用户试用。用户目前没有测试 Mac。本批只证明已复现的标题取消路径在该 Linux 环境中修复，不代表底层 WebKit 的全部问题消失。完整 C01–C68 目标继续，未提交、推送或发布。
