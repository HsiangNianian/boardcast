# 导出覆盖失败时保留原文件

2026-11-04，C45/G2 续批。真实 Linux 原生复现了数据损坏：在写满的私有文件系统中确认覆盖已有 HTML，应用显示 ENOSPC，但原来的 73 字节文件已经被替换为 4,096 字节的新 HTML 残片。修复后，同目录临时文件完整写入、同步成功后才替换目标；失败保留旧文件，清理本次临时文件。最终界面也把权限提示改为“文件或文件夹不可写”，避免把只读文件误说成目录不可写。

## 修改范围

[export.rs](../../src-tauri/src/export.rs) 的 `export_save` 统一调用 `save_export`。七种桌面导出格式、诊断 ZIP、纠错 JSON/JSONL 都经由这个调用点，没有分别修改格式生成器或新增依赖。取消、错误传播、成功后记录可打开路径的顺序保留。

临时文件使用同目录独占创建和进程内序号，避免并发导出共享一个临时文件。已存在的文件先检查普通文件类型和写权限；只读目标仍拒绝覆盖。Unix 新文件使用 `0600`，替换普通文件保留其权限位。已存在的符号链接保留链接本身并替换其真实目标；悬空链接明确失败。

[英文](../../src/i18n/en.ts) 与 [中文](../../src/i18n/zh.ts) 各调整一个权限提示开头，[交互检查](../../scripts/ui/check-export-handoff.cjs) 同步核对这句文案。产品改动为三个文件；没有改变会议存储、画布布局或模型流程。

## 实际验证

运行目录为 `/home/hsiangnianian/.cache/boardcast-g2-export-atomic-cqtwlieb`，使用私有 Xvfb、D-Bus、XDG 和 32 MiB tmpfs。仅导出目标位于该 tmpfs，会议记录另存于隔离数据目录。基线进程 UID 1000、有效 capabilities 为 0，见 [故障条件](evidence/implementation/export-atomic/baseline-fill.json)。输入均为测试会议，通过“Continue on it”得到一页延续会议和一条手动说明；本批不是此前三页七格式内容矩阵的重跑。

| 检查 | 直接证据与结果 |
| --- | --- |
| 修复前真实覆盖损坏 | 键盘输入目标路径，不发送 Enter；五次采样确认旧文件未变。点击系统 Save，观察 [Replace 确认框](evidence/implementation/export-atomic/baseline-overwrite-confirm.png) 后确认覆盖。[错误界面](evidence/implementation/export-atomic/baseline-full-result.png) 显示 ENOSPC，[实际文件](evidence/implementation/export-atomic/baseline-damaged.html) 只剩新 HTML 的 4 KiB 前缀；[磁盘记录](evidence/implementation/export-atomic/baseline-disk-result.json) 明确为原文件未保留。 |
| 修复后覆盖遇 ENOSPC | 恢复原 73 字节文件，留下 128 KiB 空间，重新通过原生 Save/Replace 导出。[磁盘记录](evidence/implementation/export-atomic/fixed-existing-disk.json) 证明旧文件 SHA-256 不变、可用空间回到 128 KiB、没有临时文件；[界面](evidence/implementation/export-atomic/fixed-existing-result.png) 保留原因及 Export again，没有打开失败产物的入口。 |
| 新路径遇 ENOSPC | 在上述错误页点击 Export again，选择不存在的 `fresh.html`。[结果](evidence/implementation/export-atomic/fixed-fresh-disk.json) 为目标仍不存在、旧文件未变、无临时残片。 |
| 释放空间后直接重试 | 删除测试填充文件，在同一原生进程点击 Export again，输入相同新路径后点击 Save。[成功界面](evidence/implementation/export-atomic/fixed-recovery-result.png) 显示完整路径及 Open file/Show in folder；实际 [HTML](evidence/implementation/export-atomic/recovered.html) 为 1,328,071 字节。 |
| 只读文件和正常覆盖 | 将测试旧文件设为 `0400`，系统确认覆盖后，应用明确拒绝，字节和权限均未变。恢复为 `0640` 后直接重试成功，[实际文件](evidence/implementation/export-atomic/overwritten.html) 为 1,328,073 字节，权限仍为 `0640`，无临时文件，见 [记录](evidence/implementation/export-atomic/fixed-overwrite-disk.json)。这里的只读拒绝来自新增检查，不冒称操作系统返回了 EACCES。 |
| 最终原生文案 | 更正文件/目录文案后重新构建并实际确认覆盖 `0400` 文件。[窗口](evidence/implementation/export-atomic/final-readonly-result.png) 显示 `file or folder is not writable` 和原始只读错误；[核对](evidence/implementation/export-atomic/final-native-check.json) 确认文件不变、重试存在、打开入口不存在。 |
| Rust 6 项 | [原始日志](evidence/implementation/export-atomic/rust-tests.log)：包括新建/完整覆盖、8 个线程并发写入、符号链接/普通权限/只读/悬空链接，以及真实私有满盘对已有与新路径的失败保护。满盘测试默认 ignored，本批通过 `BOARDCAST_EXPORT_FULL_VOLUME` 与 `--include-ignored` 实际运行，最终 6 通过、0 跳过；不是仅跑默认测试后把故障项算作通过。 |
| 生产页面 64 项与词典 4 项 | [页面报告](evidence/implementation/export-atomic/browser/export-handoff-check.json) 加载最终 `index-BgcaXh2r.js`，中英文恢复提示和原有导出交互均通过，无页面错误；IPC 在这里受控。另有 [词典测试原始日志](evidence/implementation/export-atomic/i18n-tests.log)。 |
| 实际 HTML 离线打开 | 两个实际成功文件在独立 Chromium 上打开，拦截 HTTP；各有一页、一条手动说明、4 个内嵌图片全部解码成功，无脚本错误或 HTTP 请求，见 [结果](evidence/implementation/export-atomic/offline-check.json) 和 [画面](evidence/implementation/export-atomic/overwritten-offline.png)。没有在本批重新操作系统默认应用。 |

两次成功 HTML 的字节不是完全相同。最初直接比较字节的断言失败，随后检查实际嵌入数据，差异仅为重新生成自由场景的 24 个 `versionNonce`/`updated` 值，其余结构和内容完全一致；差异路径和值保留在上述磁盘记录中，没有把不同哈希描述为相同。

## 构建身份和边界

- 基线二进制为 `28bb4c7e0b8224233835c24287ed38b5a530a64e4c26154b29ee30c0f3e286ad`。首次修复构建为 `2b011708a4ab7536fcb26a3655f49e9485c8f5e7730ff0143b47d2e2132db834`，执行满盘、失败后重试和正常覆盖；两者前端均为上一批 `index-YCCl-Tk3.js`。
- 最终构建为 `97553c0e2d6e005676b2f706ba9dd127d41553dda57f329b491a494bfd2c9821`，实际嵌入 `index-BgcaXh2r.js` 并完成最终权限提示复核，见 [构建记录](evidence/implementation/export-atomic/final-native-build-result.json)。与首次修复构建相比，仅中英文提示变化，Rust 完全相同。189 份源码/配置的三个阶段哈希均归档。
- [verification.json](evidence/implementation/export-atomic/verification.json) 汇总核对；[manifest](evidence/implementation/export-atomic/manifest.json) 包含 79 份文件、约 4.4 MB。三个原生进程均正常菜单退出、退出状态 0；私有 GUI、显示、namespace 与挂载均已清理，见 [清理记录](evidence/implementation/export-atomic/cleanup.json)。现有开发服务器和物理桌面未被重启，没有提交、推送或发布。
- 本批验证 Linux 所列文件系统/权限路径。未重跑全量 Node/Rust、七格式内容矩阵，未验证 Mac/Windows、网络文件系统、扩展 ACL/属性或硬链接语义。仅普通权限位的保留已有证据，不能据此宣称全部文件元数据保留。后续 [Linux 导出权限续批](export-permissions-validation.md)发现并修复 POSIX ACL 丢失/组权限扩大，新增 ACL、补充组和失败重试证据；本报告仍保留当时的构建及验证范围。
- 进程在写临时文件时被强制终止，可能留下隐藏临时文件；本批没有做导出中断实验或新增启动清理。文件同步后没有同步父目录，代码明确记录了断电持久性的限制；不宣称断电安全。G3/G4 与完整 C01–C68 准入保持未完成。
