# Linux 覆盖导出保留访问权限

2026-11-04，C45/G2 续批。[上一批完整写入后替换](export-atomic-validation.md)解决了满盘破坏旧文件，但只复制普通权限位会丢失 Linux POSIX ACL。本批复现了该问题，并在共享 `save_export` 中保留原 UID、GID 和访问 ACL；复制失败时保留旧文件，继续使用已有错误提示和重试入口。

## 根因和修改

基线从上一批 [export.rs 原件](evidence/implementation/export-permissions/export-before.rs)逐字提取 `save_export`，用 [独立 Rust 调用程序](evidence/implementation/export-permissions/baseline-writer.rs)写入真实测试文件。原文件为 `0640`，但其所属组没有读权限，读权限只授予指定用户 `65534`。旧实现替换后指定用户条目消失，所属组变成可读；[前后记录](evidence/implementation/export-permissions/baseline-result.json)保留完整 ACL。这是共享函数的文件系统复现，没有将它描述为修复前的原生 UI 复现。

Linux 文件模式的组权限位可能对应 ACL mask；仅复制 `0640` 无法还原原来的访问规则。临时文件还可能继承父目录的默认 ACL。[Linux acl(5)](https://man7.org/linux/man-pages/man5/acl.5.html)

产品修改仅在 [export.rs](../../src-tauri/src/export.rs)：保留对原文件的非截断写句柄，在临时文件写完后复制所有者、所属组和 `system.posix_acl_access`；原文件没有访问 ACL 时，移除临时文件继承的访问 ACL。使用已有 `libc` 的 fd 接口，失败在替换前返回。普通权限、完整写入/同步/替换、只读和符号链接保护、失败清理及成功后记录可打开路径的顺序保留，没有新增依赖或修改前端。[fgetxattr(2)](https://man7.org/linux/man-pages/man2/fgetxattr.2.html)、[fsetxattr(2)](https://man7.org/linux/man-pages/man2/fsetxattr.2.html)

## 实际验证

原生运行在私有 Xvfb、D-Bus、XDG 目录，进程 UID/GID 为 `1000`，有效 capabilities 为零。测试文件起初为 40 字节、`0640`、指定用户 `65534` 可读；父目录默认 ACL 指向另一个用户 `65533`。会议是一页延续会议和一条手动说明；本批没有重跑三页七格式内容矩阵。

| 检查 | 结果和证据 |
| --- | --- |
| ACL 复制失败 | [测试注入器](evidence/implementation/export-permissions/fail-acl.c)只在私有导出临时文件的 `fsetxattr` 上返回 EACCES，[日志](evidence/implementation/export-permissions/acl-denials.log)命中真实应用 PID `2393990`。实际输入路径，五次采样旧文件未变，点击 Save，观察 [Replace 确认框](evidence/implementation/export-permissions/acl-failure-confirm.png)后确认。[错误界面](evidence/implementation/export-permissions/acl-failure-result.png)保留原因、重试且没有打开入口；[磁盘记录](evidence/implementation/export-permissions/acl-failure-disk.json)显示原字节、ACL、UID/GID、模式均未变，无临时残片。这是受控系统调用错误，不能当作自然发生的文件系统权限拒绝。 |
| 同一窗口直接重试 | 移除故障标记，再次通过 Export again、Save/Replace 保存。[成功界面](evidence/implementation/export-permissions/acl-retry-result.png)显示路径和打开/目录入口；[文件记录](evidence/implementation/export-permissions/acl-retry-disk.json)为 1,328,094 字节，原 ACL 完全一致，没有继承用户 `65533`，无临时残片。 |
| 保留不同所属组 | 将合成目标文件的 GID 改为当前进程已有的补充组 `958`，父目录仍为 `1000`，再实际导出覆盖。[结果](evidence/implementation/export-permissions/group-disk.json)与[成功窗口](evidence/implementation/export-permissions/group-result.png)证明 GID 仍为 `958`，ACL、UID 和模式未变。[最终实际 HTML](evidence/implementation/export-permissions/restricted.html)归档的是内容副本；原权限证据来自磁盘记录，不取归档副本的权限作为证明。 |
| Rust 7 项全部执行 | [原始日志](evidence/implementation/export-permissions/full-volume-tests.log)：7 通过、0 失败、0 跳过，包括 ACL 保留、去除不需要的继承 ACL、并发完整写入、只读/符号链接保护和真实满盘。32 MiB 私有 tmpfs 留 128 KiB，已有/新路径失败后原文件保留、无新残片、空间回到 128 KiB，[记录](evidence/implementation/export-permissions/full-volume-result.json)已归档。该测试在拥有挂载能力的 user namespace 中运行，与无 capabilities 的原生 UI 进程分别记录。 |

首次在单用户 namespace 运行 ACL 测试时，固定 UID `65534` 没有映射，fixture 设置失败。最终单测使用当前有效 UID，原生 UI 测试仍使用不同用户 `65534`；[修正记录](evidence/implementation/export-permissions/namespace-fixture-correction.json)明确说明首次失败仅保留了观察摘要，未留独立原始日志。最终七项是修改后实际重跑结果。

## 构建身份和未验边界

原生二进制 SHA-256 为 `c121a06f4e95123d313aea77939724f0b467f91c48b219340ad1cfd085664d4d`，见 [构建记录](evidence/implementation/export-permissions/native-build-result.json)和[运行身份](evidence/implementation/export-permissions/native-identity.json)。前端仍为上一批 `index-BgcaXh2r.js`，本批未修改或重建前端。189 份源码/配置中只有 `export.rs` 变化。

原生构建早于最后的测试 UID 修正。[原生源码快照](evidence/implementation/export-permissions/runtime-export.rs)与[最终源码](evidence/implementation/export-permissions/export.rs)在 `#[cfg(test)]` 之前逐字相同；最终 Rust 测试使用修正后的 fixture，没有把测试改动后的文件哈希冒称为原生构建输入。[证据检查](evidence/implementation/export-permissions/verification.json)只核对已归档的观察，不重新驱动 UI；可运行同目录 `check-evidence.py` 复核。[清单](evidence/implementation/export-permissions/manifest.json)记录各文件 SHA-256。

本批仅覆盖 Linux POSIX 访问 ACL、既有 UID 和补充组保留。未验证不同 UID 的所有权转换/拒绝、并发外部权限修改、Mac/Windows ACL、NFSv4 ACL、SELinux 或其他扩展属性。未重跑全量 Node/Rust、生产页面 64 项、七格式和离线浏览器；前批结果保留原来的构建和范围。进程中断的临时文件、父目录同步与断电持久性仍见前批边界，完整 C01–C68、G3/G4 未完成。

原生进程正常退出为 0；私有 GUI 进程、显示、故障标记和满盘挂载已清理，最终文件及权限在退出后保持不变，见[清理记录](evidence/implementation/export-permissions/cleanup.json)。没有提交、推送、安装或发布。
