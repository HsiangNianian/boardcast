# C56/B0 Linux 原生请求取消续批

2026-11-05。真实 Linux Tauri/WebKit 与本机受控 HTTP 服务复现并修复：点击摘要“停止”后，原生请求仍等待服务端回复，界面持续显示“整理中”。修复后等待响应头、接收部分正文均可中断，已有摘要保持；并发板书、结束复盘、原句重试、候选摘要选择和冷启动均已验。[25 组证据核对](evidence/implementation/native-summary/evidence-check.json)通过。

## 修复与构建身份

原先 `src/remote.ts` 的桌面分支没有转发 `AbortSignal`，`makeSummary()` 必须等原生请求结束才执行清理。修复落在共用远程传输层：Rust 先登记并返回请求 ID，再启动该请求；取消只移除并中断对应 ID。登记确认避免“取消先到、请求后登记”的竞态，同一 ID 不能重复启动，完成和失败均清理状态。复用已安装的 `futures_util::Abortable`，中断范围包含等待响应头及读取正文。

前端在登记前后检查信号，监听取消并通知原生层，结束时移除监听并清理登记；取消理由优先于迟到的结果或错误。`lib.rs` 注册状态与命令。没有改变 API Key 保存位置、超时、TLS 校验、错误脱敏或摘要候选的确认规则；没有新增依赖。产品改动是三个文件，另扩展现有传输测试和 Rust 远程测试，[本批源码变更清单](evidence/implementation/native-summary/changed-files.json)保留了边界。

- 修复前复用 [TLS 续批](native-tls-validation.md)原生二进制，SHA-256 `97f8bd12018bf93dd26ad57ee5bbaf68b53ba8530dd5567431b80545c84862e0`。
- 修复后原生二进制 SHA-256 `4ce2b7a574497b834310a5557b34985291ffe9437f52f1128d4ad61087fcd770`。嵌入 `index-AZtTPHuC.js`，SHA-256 `4c3a392becf1bb9d86608f6ff2b0c4a083eeffbbc0bae3a0c7ed23bcb7628725`。
- [构建身份](evidence/implementation/native-summary/fixed-build/identity.json)对应本批最终源码；[1430 生产预览](evidence/implementation/native-summary/preview-final.json)返回相同入口字节。私有 debug 构建关闭打包，不代表签名安装包验收。

## 实际桌面结果

| 场景 | 观测 |
| --- | --- |
| 修复前取消 | 服务端保持响应头未返回。点击停止后 748、1382、2507 ms 的三个快照仍显示忙碌且连接未关闭；释放回复后才恢复。手动摘要没有被迟到回复覆盖。 |
| 修复后等待响应头 | 从 Cua 点击调用开始计，293 ms 记录到连接关闭，710 ms 的快照已恢复“重新摘要”；保存的会议字节不变。 |
| 修复后收到部分正文 | 服务端先送出响应头和 12 字节 JSON，再保持剩余正文。取消后 304 ms 关闭连接，728 ms 观察到空闲，保存的会议字节不变。 |
| 摘要与板书同时等待 | 取消摘要只关闭摘要连接；板书仍保持等待。释放板书回复后正常完成，原句 ID 不变，无重复原话，手动摘要保持。 |
| 同时运行时结束复盘 | 选择“保留未完成项并复盘”，两条连接分别于 683/684 ms 关闭；未完成原话保存为取消状态，摘要保留。 |
| 继续工作并重试原句 | 实际点击该句重试，成功更新同一个 ID，原话总数保持 5 条。 |
| 取消后重新生成 | 先显示候选草稿，保存文件仍是手动摘要；“保留当前摘要”维持旧稿，随后再次生成并明确“替换当前摘要”才保存新稿及来源指纹。 |
| 正常退出和冷启动 | 恢复“已保存”、5 条原话和明确接受的摘要。会议 JSON、空锁文件与远程配置三个文件，在退出、冷启动和再次退出期间哈希一致；冷启动只发送一次固定连接探测。 |

这些毫秒数包含 Cua 调用与截图读取开销，是本次观测结果，不是各平台的响应时间承诺。对照 [旧版取消后](evidence/implementation/native-summary/before-cancel-one-and-half-second.png)、[新版部分正文取消后](evidence/implementation/native-summary/fixed-body-canceled.png)、[并发板书完成](evidence/implementation/native-summary/concurrent-board-completed.png)、[原句重试](evidence/implementation/native-summary/retry-board-complete.png)和[冷启动摘要](evidence/implementation/native-summary/cold-summary.png)。

[服务端事件](evidence/implementation/native-summary/events.json)保留 16 次模型 HTTP 请求，其中 1 次为准备阶段预检，15 次来自原生应用。原生请求含 3 次固定连接探测、9 次明确触发的摘要、3 次明确发送或重试的板书；全部使用合成内容和合成凭据。取消测试没有把 UI 隐藏当作网络取消：对应连接均由服务端实际观察到关闭。

## 回归与运行边界

- [313 项 Node/Eval](evidence/implementation/native-summary/node-tests.log)通过，无跳过。新增传输回归覆盖预先取消、登记期间取消、不发送已取消内容、并发隔离及迟到成功结果；原中英文错误翻译继续通过。
- [Rust 远程测试结果](evidence/implementation/native-summary/rust-test-result.json)：10 项通过，包含提前取消、重复/未知 ID、成功/错误清理、并发隔离、真实 TCP 响应头/正文取消，以及既有 30 秒正文超时、代理和脱敏检查。1 项真实模型服务测试仍跳过；该文件明确标为已完成工具输出的结果摘录，不是原始 stdout。
- 同一生产构建的[摘要 21 项](evidence/implementation/native-summary/browser/check-summary/summary-check.json)与[任务隔离 6 场景](evidence/implementation/native-summary/browser/check-lifecycle/lifecycle-check.json)通过，无页面错误；后者包含 LLM、摘要、导入的迟到成功和失败。
- 类型检查、前端构建、locked/offline 原生构建通过。构建保留既有大 chunk 提示。首次构建发现测试使用的 `Promise.withResolvers` 超出项目 TypeScript lib 范围，已改用项目现有 deferred 写法后重新构建和运行完整 Node 测试。

运行目录 `/home/hsiangnianian/.cache/boardcast-native-summary-6a_kw9wo` 使用独立 Xvfb/Openbox、D-Bus、AT-SPI、XDG 配置/会议、Cua socket；不接入物理音频。模型服务只监听 loopback，应用代理变量也只在该私有进程内设置，代理拒绝转发外部目标。未修改系统代理、用户会议或真实模型配置，配置权限保持 `0600`。

Cua 的一次前台文本操作返回执行记录错误，但新截图及真实会议文件证明文字已写入，因此没有重复输入。背景页签点击未生效后才改用私有显示上的前台点击。首次等待状态的 AX 快照尚无进度文字，测试改为检查“停止”按钮与保留的草稿后重跑；原始截图及[说明](evidence/implementation/native-summary/harness-notes.json)均保留。截图黑色形状是 Cua 光标覆盖层。

修复前、修复后和冷启动三个应用均通过原生菜单正常退出，退出码为 0；私有桌面进程消失、HTTP 端口关闭，见[清理核对](evidence/implementation/native-summary/final-cleanup.json)。1423/1430 原预览继续运行。归档包含 210 个文件、9,165,327 字节及[哈希清单](evidence/implementation/native-summary/manifest.json)，不包含二进制或完整构建目录。`python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/native-summary/check-evidence.py` 只复核已捕获证据，不重新操作桌面。

## 剩余边界

本批证明 Linux 原生客户端终止 HTTP 请求；远端供应商是否随断连停止计算或计费，取决于服务端，未验证。真实模型效果、本地模型成功回退、原生在途配置切换的更多组合、物理音频和其他平台仍待验收。用户明确暂无测试 Mac，G3 实机和 G4 新用户试用保留，完整 C01–C68/G1–G4 尚未关闭。没有安装应用、提交 Git、推送或发布。
