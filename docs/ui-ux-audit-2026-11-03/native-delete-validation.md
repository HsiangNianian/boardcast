# C37 / B0 Linux 原生删除验证

2026-11-03。在上一批 [删除保护修复](delete-race-validation.md) 的同一份产品源码上，补上真实 Tauri/WebKit 窗口、IPC、文件锁和系统保存流程。三个独立原生进程使用私有数据目录；未替换存储命令、未连接模型、未启动录音。18 项 [证据检查](evidence/implementation/native-delete/native-check.json) 通过，不代表完整 G2/G3/G4 已完成。

## 构建与范围

运行目录为 `/home/hsiangnianian/.cache/boardcast-g2-delete-699auaf2`，应用标识为 `games.mmstudio.boardcast.g2.delete`。私有 Xvfb `:90`、D-Bus、XDG 数据和缓存由现有 helper 管理。A/B 共用测试会议目录，C 是删除完成后启动的新进程。产品源文件没有新增改动；helper 增加了在应用启动前注册 AT-SPI 并等待就绪，首个原生窗口即返回 61 个可达元素。

实际 [构建日志](evidence/implementation/native-delete/native-build.log) 和 [源码核对](evidence/implementation/native-delete/source-check.json) 标识本轮运行：189 个源码/配置文件与工作树一致，私有 Rust 副本一致，嵌入生产包 `index-Dcpz5Vs4.js`。二进制 SHA-256 为 `930aa63794e6148fab013964970be6d12187b8c277a92559775fbdfad7c8fb89`。只改变私有应用标识、窗口配置和构建路径；不将旧缓存二进制或浏览器 IPC fixture 算作本轮原生证据。

## 实际结果

| 场景 | 观测结果 |
| --- | --- |
| v2 无 revision，删除当前会议 | B 从会议库确认删除；A 提交此前输入的 `LATE-NATIVE-R0`，出现已删除错误。重试、新建会议仍保留原会议、两条转写、画板和图片。 |
| v1 待迁移，删除非当前会议 | A 打开后，v1 原件和图片哈希仍完全一致；B 删除它。A 新增第 2 张白板后保存失败，重试和新建均未迁移或复活记录，原转写和底图仍可见。此项通过原生可达性按钮操作，不作为键盘焦点通过证据。 |
| v2 revision 1，两个原生进程同时操作 | 外部测试进程持有该会议的实际 `.lock`；A 新增白板触发保存，B 确认删除。`/proc/locks` 同时记录两个原生 PID 等待同一 inode。释放后，A 保存完成，B 删除完成；A 再新增第 3 张白板、重试、新建均出现冲突并保留编辑。 |
| 删除后磁盘内容 | 三个 ID 在 v1/v2 两个目录都没有 JSON、`.json.tmp` 或图片目录，只保留空删除标记和锁文件。原有未引用图片也随目录删除。 |
| 新进程重开 | C 冷启动后，会议库仅列出两个另外生成的 continuation 测试记录，没有三个已删除的原件。 |
| 原生恢复副本 | A 实际点击“Export a copy”，通过 GTK 系统保存为 850,260 字节的 HTML；独立禁网 Chromium 打开后，原转写、迟到转写、图节点和蓝色 SVG 底图完整，且无页面异常。 |

查看 [R0 编辑保留](evidence/implementation/native-delete/a-r0-switch-blocked.png)、[v1 编辑保留](evidence/implementation/native-delete/a-v1-retained-final.png)、[R1 冲突与三张白板](evidence/implementation/native-delete/a-r1-switch-blocked.png)、[实际文件锁队列](evidence/implementation/native-delete/r1-lock-contention.json)、[最终目录检查](evidence/implementation/native-delete/native-delete-storage-check.json)、[冷启动会议库](evidence/implementation/native-delete/c-cold-library.png) 和 [离线副本](evidence/implementation/native-delete/r0-offline-recovery.png)。实际 [恢复 HTML](evidence/implementation/native-delete/r0-recovery.html) 的 SHA-256 为 `7e2ea6b865c36377ef02dce9b3da0c952b234f81d22b50aad2f80f4169846552`。

本次锁释放后的观测顺序是保存完成后删除；未穷举锁唤醒顺序，也未追踪 A 等锁时具体处于图片写入还是 JSON 提交。底层各写入阶段的独立进程测试仍见上一批报告。测试期间误选过“Continue on it”，生成两个独立的新 ID；随后重新打开并核对原始 R1 的哈希未变，再执行竞争。保留这两个副本作为冷启动列表的存活对照，没有将它们算作被删除会议复活。

## 保存窗口异常与对照

R0 文件已生成且应用显示成功后，原 GTK 保存窗口又处于 mapped 状态，占住焦点。可达性取消、桌面直接取消和窗口关闭都未消除它；最小化后可继续执行主窗口按钮操作。一次尝试输入的 `LATE-NATIVE-V1` 实际进入了旧保存窗口的搜索框，因此没有计入会议编辑验证。原始 [残留窗口](evidence/implementation/native-delete/r0-residual-save-dialog.png)、[搜索框误收输入](evidence/implementation/native-delete/dialog-before-desktop-cancel.png) 和遮挡画面均保留。

另在全新 C 进程对 continuation 记录导出 HTML，直接以桌面坐标点击保存：实际文件生成、成功路径出现、保存窗口关闭，[对照检查](evidence/implementation/native-delete/desktop-save-control.json) 通过。此对照没有记录点击前文件不存在，且进程、会议和输入方式不同，不能据此断言原问题属于 Cua 或 GTK，也不能宣布已修复。本报告当轮未验证原生“Open file”按钮和其他格式的完整系统交接。

后续 [C45 原生导出验证](native-export-validation.md) 实际打开了 Chromium/Thunar，并发现文件名自动化赋值后可能提前提交，另捕获无标题覆盖确认框。最终改用键盘输入、保存前文件不存在证明及保存后的文件时间，补齐 HTML 保存/取消/权限失败重试。这个续批不改写本报告历史产物的有效性，但此前仅凭保存后状态的对照不能排除提前提交；残留框根因仍未确定。

自动化中还观察到部分 WebKit 元素的坐标包含额外标题栏偏移；没有可达性 action 的导出菜单使用重新截图后的实际像素操作。启动 C 后采集的 `a-r1-retained-final.png` 全黑，不作为视觉证据；R1 使用此前的 `a-r1-switch-blocked.png` 及对应可达性快照。没有把工具的 `unverifiable` 返回当成成功，结果以新窗口状态及实际文件为准。私有 D-Bus 的 GTK 搜索曾启动桌面索引服务，原始会话 stderr 含用户目录路径，未收入仓库；后续隔离运行仍需注意文件选择器搜索的范围。

## 复核与剩余边界

运行 `python scripts/ui/check-native-delete.py docs/ui-ux-audit-2026-11-03/evidence/implementation/native-delete` 可重验归档数据。这是证据一致性检查，不会重放 UI。新一轮交互应另建私有目录，按当前 PID/window ID 操作，不能复用这里的旧句柄。文件清单与哈希见 [manifest.json](evidence/implementation/native-delete/manifest.json)。本批未重复完整 Node/Rust 单测；上一批的 259 项 Node、12 项存储测试及生产检查仍单独保留。

取证后仅通过 UI 丢弃合成的额外白板，保留 R0 恢复文件。B/C 正常退出码为 0；A 已从空会议派发退出，但 helper 清理前 PID 仍存在，未记录当时的进程状态或退出码，因此不额外宣称 A 正常退出通过。[清理检查](evidence/implementation/native-delete/cleanup.json) 确认所有私有应用、显示、可达性及辅助进程均已退出，用户浏览器页面保留。

ENOSPC/配额耗尽、附件准备阶段崩溃、断电持久性、Mac/真实音频/VoiceOver/签名安装、真实 60 分钟会议及 G4 用户试用仍未验收。保存窗口异常保持开放；没有提交、推送或发布，也没有关闭完整 UI/UX 重构目标。
