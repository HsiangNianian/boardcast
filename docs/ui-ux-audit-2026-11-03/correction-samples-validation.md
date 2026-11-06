# C59 纠错样本导出验证

2026-11-04，工作树基线 `4f9ab26`。本批修复读取失败时仍导出部分样本，以及键盘重试后焦点落到设置外的问题。以下结论限于实际执行的 G1 浏览器和 Linux G2 路径；整体 UI/UX 重构尚未完成。

## 复现与改动

旧生产页面 `index-BgcaXh2r.js` 在真实 IndexedDB 中放入两场各有一条纠错的会议。注入单条读取失败后，页面显示 1 条，实际下载也只有 1 条，而且没有错误。列表读取失败同样只显示当前会议的 1 条。见 [原始结果](evidence/implementation/correction-samples/baseline/baseline-result.json) 和 [残缺下载](evidence/implementation/correction-samples/baseline/baseline-partial.cases.json)；这不是原生文件系统故障复现。

改动涉及 `App.tsx`、`meetings.ts`、`SettingsPanel.tsx`、两份词典和原生 `meetings.rs`：

- 汇总前固定当前会议及纠错快照；读取任何会议失败都向调用方报告，不再退回“只有当前会议”。旧版本副本保留 `legacy:` 身份，避免与当前会议混淆。
- 样本汇总使用严格原生列表：文件不可读、JSON 损坏或缺少有效 ID 会报错。普通会议库继续跳过坏记录，仍可打开其他会议；删除标记仍生效。
- 进入“维护与高级”时才统计。读取失败有明确错误与重试，实际为空才显示空状态。保存期间阻止重复提交和关闭，取消恢复操作，失败可直接重试；网页显示已交给浏览器下载。
- 键盘重试前将焦点移到仍存在的样本区域。修复前新增断言确实失败，见 [焦点反例](evidence/implementation/correction-samples/focus-baseline/samples-check.json)。修复后下一次 Tab 进入第一个导出按钮。

## 浏览器与定向回归

[可复跑检查](../../scripts/ui/check-correction-samples.cjs) 在最终生产页面 `index-a6BkM9ti.js` 上通过 **21 项**，无页面脚本错误，见 [完整结果](evidence/implementation/correction-samples/browser/samples-check.json)。

实际 IndexedDB 包含 26 场当前版本会议及一个保留的旧版本副本，共 **102 条**样本。实际下载的 [cases JSON](evidence/implementation/correction-samples/browser/samples.cases.json) 和 [偏好 JSONL](evidence/implementation/correction-samples/browser/samples.dpo.jsonl) 都有 102 个唯一 ID，保留正确标签、错误标签、原话和上下文；原始记录逐字不变。列表/单条读取失败、键盘重试、真正空集合、中英文及中文 640×400 可达性均已检查。

同一检查的独立浏览器上下文使用受控桌面 IPC，验证严格列表参数、同步重复点击、等待保存时禁止关闭/Escape、取消、注入保存失败及相同完整 JSONL 重试。这部分不是系统保存对话框或真实 ENOSPC 证据。

| 检查 | 本批结果 | 边界 |
| --- | --- | --- |
| Node：assets / corrections / i18n / meetingInfo | 31 通过，0 失败、0 跳过 | 定向检查，非完整 Node 套件 |
| Rust：`meetings::tests::` | 13 通过，0 失败、0 跳过 | 包含坏文件严格/宽松列表、删除标记及独立进程删除竞争；非完整 Rust 套件 |
| 会议库生产页面 | 14 项通过 | 在焦点修复前的 `index-D6XBBiQ-.js` 执行，之后未改会议库或存储逻辑 |
| Dialog 回归 | 最终焦点修复后通过 | 开发源上的实际 SettingsPanel / Dialog fixture |
| TypeScript / Vite / 私有 Tauri 构建 | 最终均成功 | 未安装、发布或生成正式发行包 |

原始命令、日志和结果在 [checks](evidence/implementation/correction-samples/checks/)。最初一次 Rust 检查误用了缓存源码，只运行了旧的 12 项；同步并核对 `meetings.rs` 后才取得这里计入的 13 项。浏览器证据读取辅助代码最初错误指定旧库版本 1，已改为读取实际版本并完整重跑；失败记录保留，不计为通过。

## 真实 Linux 桌面流程

隔离 Xvfb、独立 D-Bus/数据目录、真实 Tauri/WebKit 和系统保存框。应用 UID/GID 均为 1000，`CapEff`/`CapAmb` 为零。输入通过 cua-driver 作用于指定私有窗口，没有操作物理桌面。

1. 将一场合成会议文件设为 `000`，实际读取返回 errno 13。普通会议库仍显示另一场会议；维护页显示具体读取错误并禁用导出。恢复 `0600` 后在同一窗口重试，显示全部 2 条样本。
2. 实际打开 cases 保存框并取消，回到设置后按钮可用，没有成功或失败提示。
3. 用键盘输入 cases 文件路径，连续五次确认文件尚不存在，再点 Save。实际文件为 **1,189 字节、2 条**。
4. DPO 目标预先包含 20 字节原件，权限 `0400`。实际保存并确认替换后，出现“目标只读”错误；字节、SHA-256、权限均未变，也没有临时文件。这是共享导出函数的只读保护，不把它说成操作系统 EACCES。
5. 恢复目标 `0600`，直接再次导出并确认替换，得到 **18,390 字节、2 条** JSONL。两种产物 ID 都是 `qa-samples-archive-1` / `qa-samples-current-1`；cases 的 gold/forbidden 和 JSONL 的 prompt/chosen/rejected 内容已核对，两场源会议未变。

实际产物和哈希见 [文件核对](evidence/implementation/correction-samples/native/native-files.json)、[cases](evidence/implementation/correction-samples/native/exports/samples.cases.json)、[JSONL](evidence/implementation/correction-samples/native/exports/samples.dpo.jsonl)。[读取错误](evidence/implementation/correction-samples/native/native-read-error.png)、[取消后状态](evidence/implementation/correction-samples/native/native-cancelled.png)、[只读错误](evidence/implementation/correction-samples/native/native-save-error.png) 和 [成功重试](evidence/implementation/correction-samples/native/native-pairs-saved.png) 保留了实际窗口证据。

第一阶段的维护分类没有可用的 WebKit AX 点击动作，使用同窗口新截图确认的坐标进入；后续最终构建直接用 Tab / End 导航成功。覆盖确认框曾跨暂停保留，Cua 会话到期后重新建立会话、确认同一进程和未变的目标文件，再观察并点击 Replace。没有把自动化会话到期或等待时间当作应用重启、音频长会证据。

## 最终焦点复核与版本身份

真实文件导出完成后，仅 `SettingsPanel.tsx` 增加焦点保留，再重新构建原生应用。在第二个隔离环境重新制造实际文件权限拒绝，键盘进入维护、Tab 到重试按钮；恢复权限后按 Enter 得到 2 条，焦点仍位于样本区域，下一次 Tab 聚焦“Export eval cases”。见 [重试后截图](evidence/implementation/correction-samples/native-focus/retry-completed.png)、[下一次 Tab](evidence/implementation/correction-samples/native-focus/export-focus.png) 和 [6 项观察核对](evidence/implementation/correction-samples/native-focus/focus-check.json)。焦点由实际截图确认，未将 Cua 的输入送达回执当成成功。

| 阶段 | 前端入口 | 原生 SHA-256 |
| --- | --- | --- |
| 真实 cases / 取消 / 只读保护 / JSONL 重试 | `index-D6XBBiQ-.js` | `288cfcc4da8dfca5adfff6ed1f43e7949dd56fbe60b8b669bd68906f078b2d79` |
| 最终键盘、实际读取错误及重试 | `index-a6BkM9ti.js` | `8b290753fdfb5646f81780518b90673f854eac234fc2dc2c760c5c6be837f2a5` |

最终前端入口 SHA-256 为 `327c997e2843fb023fa6f8d36797ed743553fdeb3b059a40f9f032e2b37efb89`。私有 dist 留有上次构建文件，`index.html` 引用和最终浏览器实际加载的都是新入口，见 [入口身份](evidence/implementation/correction-samples/checks/final-bundle-hashes.json)。第二阶段重新核对了 189 个构建输入；两阶段只有上述设置焦点改动，真实文件导出没有在最终二进制上重复执行。

两次应用都通过菜单正常退出，退出状态 0。私有应用、Cua、窗口管理器、AT-SPI、Xvfb 和本批 1438 预览已清理；现有 1423/1430 服务保留。源码快照、构建输入、源会议、实际文件及清理结果都在 [证据目录](evidence/implementation/correction-samples/)。未提交 Git、推送、安装或发布。

[归档静态核对](evidence/implementation/correction-samples/archive-verification.json) 通过 37 项，包含实际文件内容/哈希、阶段差异、原件、退出清理和报告链接；它不重放 UI。[清单](evidence/implementation/correction-samples/manifest.json) 记录 136 份文件、5,575,148 字节，不包含清单自身。

本批没有调用真实模型执行 Eval，也没有训练 DPO/SFT；文件内容检查不代表模型质量或训练效果。未重复完整 Node/Rust、七格式导出或此前完整故障矩阵，未验证 macOS/Windows、VoiceOver、真实音频和 G4 用户试用。C58 术语流程及完整方案其余待验项保持原状态。
