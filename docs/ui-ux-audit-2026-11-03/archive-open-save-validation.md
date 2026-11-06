# C33/C34 归档打开后的保存状态

2026-11-05，基线 `4f9ab26`。继续调查 [Linux 主题验收](native-theme-validation.md) 中首次打开归档持续显示 Unsaved changes 的现象，确认并修复了保存状态竞态。该提示不是“样本历史尚未补齐”的可靠说明：同一原件在浏览器正常打开，在快速返回的原生协议及真实 Linux 窗口中却会停留在未保存状态，原文件实际没有变化。

[生产响应中的跟踪](evidence/implementation/archive-open/trace-production-raw.json)显示：新会议已设置为 Saved，随后旧 render 的 effect 才执行，此时它持有旧 `meeting.id`，而 live ID、内容引用及 loaded 标记已经指向新会议。它将共享保存器标成 dirty；下一次 render 记住载入内容并清理旧定时器，没有再恢复状态。[修改前的原生截图](evidence/implementation/archive-open/native-raw.png)和[六次状态观察](evidence/implementation/archive-open/native-raw-report.json)均保留了这一结果。

[产品修改只有一行](evidence/implementation/archive-open/product-change.patch)：自动保存 effect 发现 render 的会议 ID 与当前 live ID 不同时立即返回，沿用已有会议隔离引用。保存器、磁盘写入、原件历史重建和 ASR 路径均未修改。

| 验证 | 结果与证据 |
| --- | --- |
| 新增回归先失败 | 在旧生产构建中，[新增原生协议检查](evidence/implementation/archive-open/before/check-native-shell/native-shell-check.json)因“打开归档仍是未保存”失败。原有会议库检查没有断言这条状态路径。 |
| 快速与延迟读取 | 最终 [原生外壳回归](evidence/implementation/archive-open/final-browser/check-native-shell/native-shell-check.json)通过：两种读取速度均显示 Saved，不写回原件，也不误置 beforeunload 保护；随后实际键入的新原话和模型 fixture 生成的节点能保存，旧原话保留。原有启动退出、保存失败拒绝退出、尾句保存顺序及撤销重做检查继续通过。这里的 IPC、模型与存储响应受控。 |
| 真实 Linux 打开 | 新二进制使用与修改前逐字节相同的两个测试文件；从会议库打开目标归档后，六次观察均显示 [Saved](evidence/implementation/archive-open/fixed-native/native-raw.png)，两文件哈希与初始值相同，见 [原生记录](evidence/implementation/archive-open/fixed-native/native-raw-report.json)。 |
| 原生编辑与冷启动 | 在真实输入框键入新句子，保存为 [两条原话](evidence/implementation/archive-open/fixed-native/native-edited-meeting.json)，原图不变。没有安装板书模型时明确保留 `unavailable`，未伪造新板书。正常退出后，[新进程恢复两句和 Saved](evidence/implementation/archive-open/fixed-native/cold-restored.png)；以首次正常退出后的文件为基准，冷启动与再次正常退出后两个会议均逐字节不变。 |
| 启动与任务隔离 | 最终 [58 项启动检查](evidence/implementation/archive-open/final-startup/check-startup/startup-check.json)通过，包括慢恢复期间保存、清空会议保护及草稿保留；[异步模型、摘要和导入隔离](evidence/implementation/archive-open/final-default/check-lifecycle/lifecycle-check.json)通过。 |
| 既有会议库回归 | [14 项检查](evidence/implementation/archive-open/final-library/check-library/library-check.json)通过，覆盖未保存工作副本搜索、改名失败与恢复、跨页命中、回放/导出和未来版本错误。 |
| 测试与构建 | [311 项 Node/Eval](evidence/implementation/archive-open/node-tests.txt)、[完整前端构建](evidence/implementation/archive-open/build.txt)及 [Linux locked/offline debug 构建](evidence/implementation/archive-open/fixed-build/native-build.txt)通过。[14 组归档核对](evidence/implementation/archive-open/archive-open-check.json)验证上述状态、文件与清理证据。 |

启动回归第一次运行时，临时 runner 将整个流程设为减少动态，原有断言在输入框下一帧获取焦点之前读取了状态。[独立复核](evidence/implementation/archive-open/reduced-keyboard.json)测得立即检查为 false、26ms 后实际聚焦成功。检查现已等待同一个输入框获得焦点，超时 1 秒；产品焦点逻辑未改，完整 58 项随后通过。相关失败、开发模式与生产模式的时序差异、跟踪脚本修正见 [notes.json](evidence/implementation/archive-open/notes.json)。

修改前的诊断窗口因 Cua 会话过期未能正常调用 Quit，之后被私有 launcher 终止，退出码为 -15；它只证明此前已观察到的状态，不作为正常退出证据。修复后的两次正式运行均通过实际 Quit，退出码为 0，私有进程和显示已清理，见 [清理记录](evidence/implementation/archive-open/cleanup-final.json)。

最终二进制 SHA-256 为 `28757fbac8f370ef28eafcfdaf9bb0f641bdea90c4a3c6727f4e00f1d559e47e`。1430 的 `/assets/index-DT59PRI8.js` 与磁盘及原生冻结前端一致，1423/1430 均 HTTP 200，见 [预览身份](evidence/implementation/archive-open/preview-identity.json)。[源码哈希](evidence/implementation/archive-open/source-final.json)、[源码快照](evidence/implementation/archive-open/source/)、[核对脚本](evidence/implementation/archive-open/check-evidence.py)和[归档清单](evidence/implementation/archive-open/manifest.json)一并保留。

未执行真实音频、模型推理、Mac/VoiceOver 或 G4 用户试用；用户暂无测试 Mac。其余完整验收继续，未提交、推送或发布。
