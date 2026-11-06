# Linux 原生磁盘满与附件提交中断验收

2026-11-04，C33/C68、B0/B1、G2 续批。真实 Tauri 应用分别在附件和会议 JSON 写入时遇到 ENOSPC；旧记录保留，恢复空间后能保存。另一次在图片临时文件仍打开、尚未改名时终止测试进程，重启读到完整旧会议，再次编辑成功迁移。第二次冷启动读到了两份恢复后的会议。[22 项证据核对通过](evidence/implementation/native-space-crash/check.json)。本批没有修改产品源码。

## 环境与构建

私有目录为 `/home/hsiangnianian/.cache/boardcast-g2-space-z6ra4va4`。使用独立 user/mount namespace、Xvfb `:90`、D-Bus、Cua 及 XDG data/config/cache；进程 UID 仍为 1000。只有测试会议的 v2 目录指向私有 32 MiB tmpfs，其他目录和用户磁盘不参与填满操作。[挂载脚本](evidence/implementation/native-space-crash/mount-volume.py) 检查容量并在退出前复制最终文件，再卸载；[namespace 记录](evidence/implementation/native-space-crash/namespace.json) 保存 PID、UID 和 namespace 标识。

复用上一批最终原生二进制，SHA-256 为 `01f9aedfc009dac29ccf91416acfc313fc85382d341998d612d9b94b13e167c7`，前端入口 `index-N8K0hlyP.js`，测试 identifier 仍为 `games.mmstudio.boardcast.g2.delete`。189 个源码/配置哈希与工作树一致，见 [身份](evidence/implementation/native-space-crash/identity.json)、[源码清单](evidence/implementation/native-space-crash/source-hashes.json) 和 [结束核对](evidence/implementation/native-space-crash/final-source-check.json)。该二进制的生产/原生构建结果见 [上一批报告](native-formats-validation.md)，本批没有重新构建或重复完整 Node/Rust。

输入、提交、重试、切会和退出由原生界面触发，随后检查真实文件；没有模型或音频任务。磁盘满样本包含三页、两条已有原话、摘要和图片。其原始 [seed](evidence/implementation/native-space-crash/seed.json) 保留了调整选中页、未同步顶层 graph 的构造细节；本例用于检查存储保留与恢复，不作板书或导出保真验收。

## 磁盘满

填充文件只创建在已核实容量的私有 tmpfs 内，实际 `write` 返回 errno 28，剩余可用字节为 0。两次填满的独立记录见 [第一次](evidence/implementation/native-space-crash/record-fill.json) 和 [第二次](evidence/implementation/native-space-crash/json-fill.json)。

| 步骤 | 实际结果 |
| --- | --- |
| 附件保存失败 | 输入 `ENOSPC_RECORD_RECOVERY_LINE` 后显示 `Couldn't save the image: No space left on device (os error 28)`；新增原话仍在界面，旧 JSON revision 1 和原附件哈希不变。见 [窗口](evidence/implementation/native-space-crash/record-failed.png)、[文件核对](evidence/implementation/native-space-crash/record-failure-disk.json)。 |
| 阻止丢弃 | 分别点击 New meeting 和原生 Quit 后仍停留在同一失败会议并保留新原话。见 [新建后](evidence/implementation/native-space-crash/full-new-blocked.png)、[退出后](evidence/implementation/native-space-crash/full-quit-blocked.png)。 |
| 第一次恢复 | 释放空间后原生状态为 Saved，磁盘为 revision 2，新增原话已保存。首次 Retry save 自动化返回 stale token，后续观察时已保存；不把这次恢复归因于已确认的重试按钮点击。见 [恢复记录](evidence/implementation/native-space-crash/first-recovered-disk.json)。 |
| JSON 保存失败 | 再次填满，输入 `ENOSPC_JSON_RECOVERY_LINE`；错误明确来自 meeting 写入。revision 2 和全部已有附件不变，`.json.tmp` 为 0 字节。见 [窗口](evidence/implementation/native-space-crash/json-failed.png)、[文件核对](evidence/implementation/native-space-crash/json-failure-disk.json)。 |
| 明确重试 | 释放空间后五次观测仍是 revision 2，再通过界面点击 Retry save。新文件为 revision 3，四条原话含两条新增内容，六个资源引用完整，临时 JSON 消失，UI 为 Saved。见 [点击前](evidence/implementation/native-space-crash/json-before-retry.json)、[恢复后](evidence/implementation/native-space-crash/json-recovered-disk.json)。 |
| 冷启动 | 独立的新进程通过会议库打开该会议，两条新增原话仍在且显示 Saved。见 [冷启动窗口](evidence/implementation/native-space-crash/cold-space.png)。 |

## 附件提交中断

另建 v1 会议 `asset-crash`，保留三页与已有原话，将第一页底图替换成可正常解码的 SVG：160×90 青色矩形，XML 注释内含 24 MiB 字符，文件共 25,165,947 字节。原始 JSON 和六份附件的哈希见 [种子记录](evidence/implementation/native-space-crash/asset-crash-seed.json)；实际资源以无损 gzip 归档，便于独立解码核对。

[观察器](evidence/implementation/native-space-crash/watch-asset-crash.py) 只针对已核实路径的测试进程。界面提交新原话后，inotify 捕获目标图片 `.tmp` 创建，先 SIGSTOP 并核实临时文件、未出现的最终文件与 v2 JSON，再 SIGKILL。捕获时临时文件已有全部 25,165,947 字节，进程仍持有该文件的 fd 53，图片尚未改名。这证明附件提交前中断，不能称为“只写了一部分图片字节”。[原始结果](evidence/implementation/native-space-crash/asset-crash-result.json) 和 [退出信号](evidence/implementation/native-space-crash/crash-process-exit.json) 保留这些区别。

终止后 v1 JSON/附件全部不变，没有 v2 JSON 引用未完成图片。第一次重启仍能打开旧会议，未确认保存的 `ASSET_CRASH_UNACKNOWLEDGED_LINE` 不存在；临时图片留在磁盘但未被当作最终资源读取。见 [重启状态](evidence/implementation/native-space-crash/restarted.png)、[重启文件检查](evidence/implementation/native-space-crash/restart-before-edit-disk.json) 和 [第一页实际显示](evidence/implementation/native-space-crash/restart-board1-visible.png)。

再次输入 `ASSET_CRASH_RECOVERED_LINE` 后迁移成功：v2 revision 1、三条原话、六个完整资源引用；大图片内容哈希匹配，旧版全部原件保持不变，临时图片和 JSON 均消失。见 [迁移核对](evidence/implementation/native-space-crash/migration-recovered-disk.json)。正常 Quit 后再次启动，实际界面读到了新原话和图片，见 [第二次冷启动](evidence/implementation/native-space-crash/cold-migration.png)。

## 归档与剩余范围

[最终文件清单](evidence/implementation/native-space-crash/final-disk.json) 与 `final-records/` 中的实际 JSON 对应；`assets/` 保存八份去重的 gzip 原始资源。[只读核对脚本](evidence/implementation/native-space-crash/check-evidence.py) 检查记录、AX 状态、哈希、资源引用、原件和清理结果，直接运行即可输出 22 项结果；它不会重放 GUI 或再次注入故障。脚本检查通过不替代这里记载的人工查看截图与实际操作过程。

[manifest](evidence/implementation/native-space-crash/manifest.json) 记录 120 份归档文件、约 6 MB 的大小和哈希。故障原进程收到 SIGKILL；两个后续进程均通过正常 Quit 以 0 退出。私有驱动、D-Bus/显示服务、namespace 进程及挂载已经清理，用户桌面和既有开发服务器保持运行，见 [清理记录](evidence/implementation/native-space-crash/cleanup.json)。完整临时目录及 `volume-final` 留作复核。

本批覆盖 Linux tmpfs 的 ENOSPC 和一处明确的附件提交前进程中断。没有验证突然断电、所有文件系统/额度/inode 耗尽变体、每个附件/JSON 提交时点或 Mac 实机；现有原子改名也不等于文件/目录 fsync 的断电保证。本批观察到的分类提示缺口随后在 [恢复提示续批](storage-recovery-guidance-validation.md) 中补齐，并通过新的生产/原生构建验证。G3 真实音频/VoiceOver/签名安装及 G4 用户试用保持待验，没有提交或发布。
