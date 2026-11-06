# C66/C68：Linux 界面异常恢复与退出

2026-11-05，基线 `4f9ab26` 的当前工作树。接续 [C42 的 WebView 退出观察](native-arxiv-cancel-validation.md)：网页进程停止后，旧版只剩原生菜单和最后一帧，退出仍等待已经消失的前端保存确认。本批为 Linux 增加原生恢复提示，可重新打开已保存的会议，也可正常退出。

复用既有 WebKit 终止信号、原生 Dialog、请求表和停止收音方法：界面停止时取消已登记的远程板书/arXiv 请求，清除待处理的收音切换，停止收音；选择恢复后重载界面并丢弃失效的退出确认。提示明确说明尚未保存的修改可能丢失。没有新增依赖或另一套存储机制，仅修改 `shell.rs`、`remote.rs` 和初始化调用。

后续 [Linux 输入法与焦点检查](native-ime-validation.md)扩展了触发边界：不传元素 token、关闭输入法的普通标题 Escape 同样可重现 SIGABRT，并取得 WebKit 用户态栈。取消前在编辑行内结束输入会话后，该标题路径在最终构建不再重现；其他输入触发和 WebKit 内部根因仍未关闭。下文保留本批当时的构建身份与观察，不把旧对照解释为只有 token 路径会崩溃。

[材料导入续批](native-import-focus-validation.md)进一步确认元素输入路径仍可崩溃，错误记录落在文字送达前的 AT-SPI GrabFocus；普通窗口按键及系统文件选择器对照完成，未把焦点返回修复当作底层崩溃修复。

| 实际操作 | 结果与证据 |
| --- | --- |
| 旧版重现 | 在隔离源码副本仅增加终止原因日志。Cua 按 Web 输入元素 token 发送 Enter 后，WebKit 记录 `Crashed`，可访问树只剩 6 个原生菜单；选择退出后宿主仍存活，最终定向终止该测试进程。见[观察](evidence/implementation/native-webprocess/probe-exit-observation.json)和[日志](evidence/implementation/native-webprocess/token-runtime/app.txt)。 |
| 输入路径对照 | 同一诊断构建用窗口范围的 Enter，不传元素 token，两次均进入真实下载状态并取消，未记录网页进程终止。见[两次结果](evidence/implementation/native-webprocess/plain-key-results.json)。这定位了可复现的触发路径，尚不能确定 Cua/WebKit 内部根因。 |
| 英文恢复 | 最终产品构建再次遇到同样的崩溃后，显示[原生警告](evidence/implementation/native-webprocess/fixed-recovery-visible.png)。选择恢复，同一宿主进程重新显示原会议；已保存文件字节不变，未发送草稿消失，符合警告。恢复后实际发送第二句并保存成功，见[恢复界面](evidence/implementation/native-webprocess/fixed-restored.png)和[两句记录](evidence/implementation/native-webprocess/two-sentence-meeting.json)。 |
| 中文与请求清理 | 切换中文，用窗口范围 Enter 启动原生 arXiv 请求，在代理收到 CONNECT 后仅终止该应用的 WebKit 子进程。连接在本次信号后约 9 毫秒关闭，[中文原生提示](evidence/implementation/native-webprocess/network-recovery-dialog.png)显示恢复、退出及未保存警告。两句会议文件不变。见[请求与进程记录](evidence/implementation/native-webprocess/network-crash-result.json)。该时间不是平台性能承诺。 |
| 异常后退出 | 点击中文“退出”，实际宿主退出码为 0，会议文件不变。依据[子进程退出记录](evidence/implementation/native-webprocess/fixed-runtime/cleanup.json)，不以 Cua 的动作送达报告代替退出结果。 |
| 冷启动与正常退出 | 用同一最终构建冷启动，恢复两句原话、中文“已保存”和“开始收音”，两个会议文件字节不变；原生退出菜单正常结束进程。见[冷启动界面](evidence/implementation/native-webprocess/cold-restored.png)、[文件核对](evidence/implementation/native-webprocess/after-cold-files.json)和[退出记录](evidence/implementation/native-webprocess/cold-runtime/cleanup.json)。 |

[5 项外壳测试](evidence/implementation/native-webprocess/shell-tests.log)、[11 项远程请求测试](evidence/implementation/native-webprocess/remote-tests.log)通过，既有外部模型服务测试仍跳过。新增检查覆盖失效退出令牌、已运行/待启动请求全部取消，以及恢复后新请求不受旧取消影响。原生构建、改动文件格式检查和差异检查通过；[20 项证据核对](evidence/implementation/native-webprocess/evidence-check.json)通过。`check-evidence.py` 只核对记录，不会重演桌面操作。

194 个构建输入已与当前工作树核对，最终原生二进制、WebKitGTK 2.52.6 / GTK 3.24.52 / Cua 0.34.0、三个源码差异及预览身份见[身份记录](evidence/implementation/native-webprocess/identity.json)和[归档清单](evidence/implementation/native-webprocess/manifest.json)。前端没有变化，1430 返回的脚本哈希仍与前次产物一致，因此未重复无关浏览器全量回归。

原生提示的静态文字存在于 Cua 的 `tree_markdown`，不在可操作 `elements` 中；早期断言只检查后者，曾在点击恢复前误报。随后使用新快照和实际截图核对，恢复成功证据来自真正点击之后。当前任务 cgroup 的 OOM 计数为零，未取得内核崩溃栈，不能据此宣布系统根因已修复。

全部测试使用独立显示、数据和测试原话。代理仅监听回环地址，本次在真实原生请求的 CONNECT 回复前等待；不替换产品下载地址。24 个登记的测试进程及代理均已停止，没有遗留私有环境子进程，原 1423/1430 预览继续运行。

**仍未验证：**物理音频的停止与尾句、停止线程/重载 API 失败分支的故障注入、Mac/G3 与 G4 用户试用。用户目前没有测试 Mac。本批不恢复崩溃前未保存的草稿，也未消除底层 WebKit 崩溃；它补上 Linux 宿主的恢复和退出路径。整体目标保持进行中，未提交、推送或发布。
