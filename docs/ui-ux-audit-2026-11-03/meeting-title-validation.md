# C04：顶栏直接修改会议名称

2026-11-05，基线 `4f9ab26` 的当前工作树。方案 §2 要求标题可以直接改名，但原按钮只打开会议库，[改动前检查](evidence/implementation/meeting-title/baseline/check-meeting-title/failure.txt)和[页面](evidence/implementation/meeting-title/baseline/check-meeting-title/failure.png)确认了这个缺口。

现在会议库和当前标题分别是独立按钮。标题打开复用现有 Dialog 的编辑框，调用原有 `renameMeeting` 和保存事务；明确命名的空会议也保存并进入会议库。空标题保留输入焦点，组合 Enter/Escape 不提交或关闭，失败保留草稿，保存期间阻止重复操作，取消和完成后恢复入口焦点。保存失败沿用既有语义：新名称保留在当前未保存会议中，旧磁盘文件不变，取消编辑不回滚已经提交的保存意图。切会时清除编辑框，回放和恢复期间不可改名。

原生初版的长模型提示把名称压缩到一个字母，已固定名称区域的可读宽度，窄窗允许换行。两按钮保持 36px 高度，长名称省略显示，完整名称保留在可访问名称、提示和编辑框中。[初版](evidence/implementation/meeting-title/initial.png)与[最终原生顶栏](evidence/implementation/meeting-title/final-initial.png)可对照。

| 边界 | 结果与证据 |
| --- | --- |
| 浏览器真实存储 | [28 项生产页面检查](evidence/implementation/meeting-title/final-layout/check-meeting-title/report.json)通过：空标题、Escape/指针取消、组合键、命名空会议、刷新恢复、原话保留、写入拒绝和重试。覆盖五种尺寸及中英、明暗编辑框；[720×480](evidence/implementation/meeting-title/final-layout/check-meeting-title/editor-720x480.png)、[中文暗色](evidence/implementation/meeting-title/final-layout/check-meeting-title/editor-zh-dark.png)。640×400 为逻辑视口检查，本新增脚本不冒充实际浏览器 200% 缩放。 |
| 原生保存等待 | 复用现有 native-shell fixture，[保持未完成的原生保存](evidence/implementation/meeting-title/final-browser/check-native-shell/native-shell-check.json)时，保存/取消不可用，Enter/Escape/新会议菜单不重复提交；完成后才关闭并恢复焦点。此项是协议模拟，不是真实慢磁盘。 |
| Linux 中文输入 | 真实 Fcitx5/Rime [候选窗口](evidence/implementation/meeting-title/real-candidates-desktop.png)可见；候选 Enter 只提交输入法文本，编辑框保持。Escape、前台指针取消均可回到标题并用 Enter 重开。选择“你好”并明确保存后，产生 [revision 1 的空会议文件](evidence/implementation/meeting-title/named-empty.json)，[会议库](evidence/implementation/meeting-title/named-library-ready.png)显示 0 句、1 张板。 |
| Linux 实际写入拒绝 | 仅将隔离目录设为不可写，真实 EACCES [保留草稿并显示错误](evidence/implementation/meeting-title/permission-error.png)，[原文件字节不变](evidence/implementation/meeting-title/save-failure-result.json)。恢复权限后直接 Enter，保存为同 ID 的 [revision 2](evidence/implementation/meeting-title/retried-empty.json)。 |
| 原生窄窗与冷启动 | [720×480 编辑框](evidence/implementation/meeting-title/small-reopen-after.png)的输入、保存、取消均可达；空标题和指针取消[不改原文件](evidence/implementation/meeting-title/small-result.json)。同一最终二进制[冷启动恢复名称与 Saved](evidence/implementation/meeting-title/cold-restored.png)，[退出前后文件哈希一致](evidence/implementation/meeting-title/cold-result.json)，仍只有一场空会议。 |

九个会议库入口调用脚本已迁移到独立按钮并通过回归：会议库 21 项、继续旧板 43 项、术语 30 项、全部示例 55 项、示例首次编辑 37 项、读取失败 14 项、收音切会 27 项，以及删除恢复和原生外壳流程；见[运行记录](evidence/implementation/meeting-title/final-browser.log)。最终宽度调整后再验新增页面、64 项导出交接和工作区布局/编辑/导出，见[最终记录](evidence/implementation/meeting-title/final-layout.log)。14 项保存、会议信息和语言单元测试、类型检查、前端构建及原生 locked/offline 构建通过。Rust 产品代码本批未改，没有重复 Rust 全量测试。

[20 项证据核对](evidence/implementation/meeting-title/evidence-check.json)与[身份记录](evidence/implementation/meeting-title/final-identity.json)确认测试源码和最终产物一致。浏览器入口为 `assets/index-BDP2K7O-.js`；隔离原生二进制 SHA-256 为 `7c0167912a8b7a0ca9cda1132f6d4174fb551cb5b499beff45543327a68aa1e7`。三个应用正常退出，21 个登记进程及私有 X socket 已清理，目录权限恢复，原 1423/1430 预览保留并更新；见[清理记录](evidence/implementation/meeting-title/process-cleanup.json)。完整临时记录位于 `/home/hsiangnianian/.cache/boardcast-meeting-title-p_1p0iuf`，归档见[清单](evidence/implementation/meeting-title/manifest.json)，不包含二进制、模型或输入法词典。

调试边界：早期文件探针漏掉 `v2` 子目录，相关“没有文件”断言不计作证据；后来使用实际文件路径重验空标题、取消、失败、重试和冷启动。一次语义点击没有建立入口焦点，后续改用前台指针和键盘验证焦点恢复。窗口截图中的黑块为 Cua 光标或外部候选窗口捕获，候选内容以完整桌面截图核对，未修改截图。GTK 原有快捷键警告仍在，最终两次运行未出现 WebKit 终止或 SIGABRT。

仍未验证 Windows ARM/Mac 上的此入口、日文输入法、VoiceOver 和全部状态组合；用户目前没有测试 Mac。此前 Windows ARM 模型预热修复保留在同一工作树，本批不补充该硬件证据。G3/G4 与完整 C01–C68 目标继续，未提交、推送或发布。
