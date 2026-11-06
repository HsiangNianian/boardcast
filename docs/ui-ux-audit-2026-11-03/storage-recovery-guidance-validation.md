# 保存与导出错误的恢复提示

2026-11-04，C33/C45/C61 续批。保存失败现在按已识别的原因显示中英文恢复提示：磁盘满时释放空间或导出到其他磁盘，权限拒绝时检查写入权限或选择可写位置，浏览器配额不足时先导出副本并避免清除此网站数据。原始错误仍保留；未知错误沿用原来的处理，不标成已知原因。导出仅在写入阶段使用这些分类，生成失败与取消行为保持原有语义。

## 修改范围

[App.tsx](../../src/App.tsx) 复用 [diagnostics.ts](../../src/engine/diagnostics.ts) 的错误信号选择恢复文案，没有增加保存状态或改变存储顺序。分类补上浏览器 quota 文本及两种常见英文系统错误表达；[英文](../../src/i18n/en.ts) 与 [中文](../../src/i18n/zh.ts) 分别提供会议保存和导出文件的下一步。会议保存提示保留“重试保存/导出副本”，导出保留“重新导出”；成功后才显示已保存和文件打开入口。

测试扩展现有 [diagnostics.test.ts](../../src/engine/diagnostics.test.ts)、[导出交接检查](../../scripts/ui/check-export-handoff.cjs) 与 [工作台检查](../../scripts/ui/check-workbench.cjs)。本批产品代码改动为 App、错误信号和两份词典；没有修改 Rust 保存实现、依赖、CSS 或用户数据。

## 浏览器与逻辑验证

旧生产构建 `index-N8K0hlyP.js` 运行新增断言时，明确失败于“权限错误应说明选择可写导出目录”，见 [修复前结果](evidence/implementation/recovery-guidance/baseline/export-handoff-check.json)。修复后生产构建为 `index-YCCl-Tk3.js`。

| 检查 | 已执行范围 |
| --- | --- |
| 20 项 Node 测试 | 错误分类、诊断内容脱敏、保存顺序/失败恢复和词典一致性；命令为 `node --test src/engine/diagnostics.test.ts src/engine/meetingSave.test.ts src/engine/i18n.test.ts`。结果来自本轮工具输出，[记录](evidence/implementation/recovery-guidance/unit-result.json) 明确标注没有另外捕获原始日志。 |
| 64 项生产页面交互 | [报告](evidence/implementation/recovery-guidance/browser/export-handoff-check.json)：保留原有取消、阶段、重复点击、打开/目录、迟到回应和重试检查；新增中英文四类保存错误的原文/提示、切会保护、旧记录保留及键盘重试，三类导出错误的原因提示与成功重试。原生 IPC 在此检查中受控，不代表系统文件对话框。 |
| 小窗口键盘 | 中英文空间不足场景运行于 640×400 CSS 像素；Retry save 获得键盘焦点后位于可滚动顶栏可见范围内，回车能保存，页面无横向溢出。其余新增场景为 720×480。不是本轮重新执行完整 24 组缩放矩阵。 |
| 真实 IndexedDB | [工作台回归](evidence/implementation/recovery-guidance/browser/workbench-check.json) 在实际浏览器存储上注入 `QuotaExceededError`，消息为 `The quota has been exceeded.`；界面显示额度提示及保留网站数据的说明，阻止丢失修改的切会，移除注入后保存了原话和自由场景。未实际耗尽浏览器配额。 |
| 构建 | [生产构建](evidence/implementation/recovery-guidance/web-build.log) 含类型检查、播放器和应用；[私有原生构建](evidence/implementation/recovery-guidance/native-build.log) 成功，未重新运行全量 Node/Rust。 |

页面回归无脚本错误。检查脚本快照在归档的 `checks/` 中；原脚本仍接收独立证据目录参数，复跑请使用新的目录，以保留本次档案。

## 真实 Linux 原生验证

隔离目录 `/home/hsiangnianian/.cache/boardcast-g2-recovery-ui-4mwxzsee` 使用独立 XDG、Xvfb、D-Bus 和 user/mount namespace。v2 会议目录指向私有 32 MiB tmpfs，应用 UID 为 1000、有效 capabilities 为 0。原生二进制 SHA-256 为 `28bb4c7e0b8224233835c24287ed38b5a530a64e4c26154b29ee30c0f3e286ad`，实际嵌入上述新前端，见 [构建身份](evidence/implementation/recovery-guidance/native-build-result.json)。缓存私有项目的 Rust、Cargo 和基础构建配置与工作树一致；189 个源码/配置哈希在本批构建和运行期间未改变。

| 故障 | 实际观察与恢复 |
| --- | --- |
| 英文 ENOSPC | 填充私有文件系统直到实际 errno 28、剩余 0 字节。通过原生输入框提交 `NATIVE_FULL_GUIDANCE_LINE`，附件保存失败；界面同时显示释放空间/其他磁盘提示、原始 ENOSPC 和保留的原话。见 [窗口](evidence/implementation/recovery-guidance/native-full-failed.png)。旧 JSON/附件哈希不变；释放空间后五次记录仍为 revision 1，再点击 Retry save，成为含新原话的 revision 2。 |
| 中文 EACCES | 通过原生设置切换到中文，将私有会议目录改为 `0500`。提交 `NATIVE_PERMISSION_GUIDANCE_LINE` 后，真实 JSON 写入返回 Permission denied；显示“会议文件夹不可写，检查写入权限”，原话仍在。见 [窗口](evidence/implementation/recovery-guidance/native-permission-failed.png)。已有文件哈希不变；恢复原目录 mode 后五次记录仍为 revision 2，点击“重试保存”后为 revision 3，界面显示“已保存”。 |

[最终实际文件](evidence/implementation/recovery-guidance/native-final/space-record.json) 保留四条原话，含上述两条新增内容；六个图片引用的文件和内容哈希均匹配，见 [磁盘核对](evidence/implementation/recovery-guidance/native-final-disk.json)。输入/快照、故障前哈希、恢复权限/空间后的采样和重试前状态分别保留在 `native-full-*`、`native-permission-*` 中。原生阶段没有重新打开导出文件对话框；分类导出提示的本批证据来自生产页面与受控 IPC。

设置中的中文选项曾通过 AX 退回到偏移的合成点击，未改变语言；检查窗口后改用同一窗口的新截图坐标操作，确认界面及原生菜单已切换。没有把前两次未生效的自动化当作成功。

## 归档与边界

[verification.json](evidence/implementation/recovery-guidance/verification.json) 汇总本批核对；[manifest](evidence/implementation/recovery-guidance/manifest.json) 记录 91 份文件、约 2.8 MB 的大小和 SHA-256。应用正常 Quit，退出码 0；私有驱动、显示、namespace 进程与挂载均已清理，见 [清理记录](evidence/implementation/recovery-guidance/cleanup.json)。现有开发服务器和用户桌面未被重启，没有提交或发布。

这里验证了所列错误文本及 Linux 原生两条路径，不覆盖所有操作系统语言/错误表达。Windows 样式字符串仅为单元样本，Mac、真实音频/VoiceOver/签名安装及 G4 仍未验收。本批只改变提示；当时 `export_save` 直接写目标文件，尚未验证覆盖旧导出遇 ENOSPC 的保留行为。随后在 [覆盖导出保护续批](export-atomic-validation.md) 中实际复现旧文件损坏，完成同目录写入后替换的修复和原生故障/重试验证；权限提示也更正为“文件或文件夹不可写”。
