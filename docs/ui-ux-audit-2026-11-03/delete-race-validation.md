# C37 / B0 删除与迟到保存

2026-11-03。本批修复删除后旧窗口重新保存会议、重新写入图片的问题。完成了真实 IndexedDB、生产浏览器界面和 Linux 文件系统多进程验证。后续同源码的 [原生删除验证](native-delete-validation.md) 补上 Tauri 窗口/IPC 保存删除竞争、冷启动和实际 HTML 恢复文件，保留系统保存窗口异常的独立边界。

## 复现与修复

[修复前结果](evidence/implementation/delete-races/browser-before.txt) 使用两个独立页面共享真实 IndexedDB，没有替换 `meetings.save/remove`。三种记录分别在删除完成后、图片哈希计算暂停期间发起保存：

| 原始记录 | 删除后保存 | 图片准备期间删除 |
| --- | --- | --- |
| v2 revision 1 | 记录拒绝保存 | 记录拒绝，但重新写入一张图片 |
| v2 没有 revision | 会议重新出现 | 会议和图片重新出现 |
| v1 待迁移 | 会议重新出现 | 会议和图片重新出现 |

原因是不存在的记录与未设置 revision 的记录都按 0 比较；每个窗口自己的保存队列无法阻止另一窗口已经开始的写入。附件在记录提交之前写入，也必须检查删除状态。

网页删除现在将记录删除、附件删除、仅含 ID 的布尔删除标记放进同一个 IndexedDB 事务。记录提交和附件写入在各自事务里检查标记。标记放在附件库的独立键中，不会给旧版会议列表插入缺字段的假记录，也不需要升级数据库版本。

原生端复用同一把文件锁，删除前写入空的 `.deleted` 标记；记录和图片写入都在锁内检查标记。删除也清理中断写入的 `.json.tmp`。记录删除失败会撤回本次新标记；已经删除记录但后续清理失败时保留标记，返回错误并允许重试。标记不保留标题、原话或图片内容。

## 已执行验证

- [存储检查](evidence/implementation/delete-races/browser-after.txt)：上述六种情况全部拒绝迟到保存，列表/原始记录/加载均不再返回会议，附件数保持 0；页面重新加载后标记仍有效。普通迁移保持 v1 原件及图片，旧 revision 仍拒绝覆盖，旧版副本能以新 ID 恢复。删除事务在请求成功后被强制 abort 时，记录、图片和可保存状态均保留，重试删除成功。这是事务 abort 注入，不是实际磁盘配额耗尽。
- [生产双标签页](evidence/implementation/delete-races/ui-check.json)：B 从会议库确认删除当前会议；A 随后提交新原话，显示保存失败并保留原话和板书。再次保存不会恢复已删除数据，新建会议被阻止。实际点击错误提示中的“导出副本”，下载的 [HTML](evidence/implementation/delete-races/recovery.html) 在禁网的新浏览器环境下打开，保留原话、板书、图片和回放数据；另有实际 Markdown 下载。查看 [错误状态](evidence/implementation/delete-races/retained-edit.png) 和 [离线副本](evidence/implementation/delete-races/offline-recovery.png)。模型回复为合成内容；删除与保存使用生产代码和真实 IndexedDB。
- [Linux 文件系统](evidence/implementation/delete-races/rust-storage.txt)：12 个存储测试通过，包括 v1/v2 命名空间、删除标记不含内容、重复删除、迟到记录/图片拒绝、失败回退与清理重试。12 轮线程竞争以及 v1/v2 各三个独立子进程实际争用文件锁；释放锁后删除、记录写入、图片写入交错，最终记录和图片均不存在。测试调用生产命令使用的同一组文件函数，不是 Tauri UI 或 IPC 验证。
- [完整 Node](evidence/implementation/delete-races/node-tests.txt) 259 项通过；[附件检查](evidence/implementation/delete-races/asset-check.txt)、[会议库回归](evidence/implementation/delete-races/ui-run.txt)、TypeScript、生产 Vite 构建和非测试 Rust `cargo check --lib` 通过。附件单测曾按异步 Map 插入顺序删图片，偶尔删错页；改为按实际第 1 页的资源引用删除。

生产浏览器加载 `index-Dcpz5Vs4.js`。精确源码、脚本及构建哈希见 [source-hashes.json](evidence/implementation/delete-races/source-hashes.json)，证据文件见 [manifest.json](evidence/implementation/delete-races/manifest.json)。附件检查第一次遭遇 Vite 导航导致执行上下文销毁，独立重跑通过；未把该次中断算作产品成功。离线测试按真实 `.html` 文件打开，等待播放器内容就绪后取证。

## 复跑与边界

开发端口执行 `check-delete-races.cjs check-asset-integrity.cjs`；生产 preview 执行 `check-delete-recovery.cjs check-library.cjs`，均使用 [现有 runner](../../scripts/ui/README.md)。在 `src-tauri` 执行 `CARGO_NET_OFFLINE=true cargo test --lib meetings::tests -- --nocapture`。测试使用独立浏览器上下文和新建临时目录；子进程退出、临时目录清理后结束，不接触用户会议。

删除保护需要保留这些不含内容的标记，旧 ID 不再复用。v1/v2 的跨库/跨目录删除仍分两步，第二步失败会报错并允许重试；这里没有新增跨命名空间原子事务。忽略标记的旧版客户端和手动修改存储不在并发保护承诺内。

本批没有验证 ENOSPC、附件准备阶段进程崩溃或断电。原生窗口/IPC 的后续实际验证及保存窗口残留见链接报告，不以底层文件测试替代。文件仍未新增 fsync，不从文件锁或原子 rename 推断断电持久性。G3 Mac/真实音频/VoiceOver/签名安装及 G4 用户试用继续保留；本批不关闭完整重构目标。
