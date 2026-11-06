# C27 Linux PDF 标注、换页与重启恢复

2026-11-05，基线 `4f9ab26`。真实 Linux Tauri 检查发现：自由编辑从第 2 页切回第 1 页时，标记已更新，PDF 底图仍是第 2 页。见[修复前](evidence/implementation/native-pdf/reopened-page1-free-settled.png)与[最终构建](evidence/implementation/native-pdf/final-return-page1.png)。最终产品改动只有一行：按会议 ID 和页码挂载 `PencilBoard`，让 Excalidraw 的图片缓存属于当前白板。既有会议与导出格式不变，见[本批补丁](evidence/implementation/native-pdf/product-change.patch)。

Excalidraw 的 `addFiles` 保留已存在 ID 的文件；原先跨页复用编辑器，底图的固定 ID 因而命中上一页。曾试用内容哈希 ID，画面恢复后又发现旧缓存文件进入当前页的 scene；该方案已撤回。最终直接隔离编辑器实例，同时避免错误图片和无操作的保存记录。独立红/绿 PDF 的[旧版 7 项检查有 3 项不符](evidence/implementation/native-pdf/underlay-before-complete/underlay-pages-check.json)，[最终 8 项全部通过](evidence/implementation/native-pdf/regressions/underlay/underlay-pages-check.json)，包括缓存不进入保存状态。

| 验证范围 | 结果与证据 |
| --- | --- |
| 原生 PDF 导入 | 经真实 GTK 文件选择器导入两页 PDF，实际会议 JSON 的两页 graph 与 Chromium 导入结果完全一致。[导入界面](evidence/implementation/native-pdf/import-settled.png)、[原生文件](evidence/implementation/native-pdf/imported-meeting.json)。 |
| 原文命中与标记 | 跨列“会议”实际添加五种标记，每种保留两个正确字框；撤销/重做移除、恢复一个完整标记。同高度左右“保存”各留在本列，左列整句星标不混入右列。中文表格单元格选择整表，独立公式编号 `7` 可高亮。核对真实落盘快照，见[30 项证据核对](evidence/implementation/native-pdf/native-check.json)。 |
| 最终原生换页 | 同一最终进程完成 2→1→2→1，底图与标记同步；[第一页](evidence/implementation/native-pdf/final-page1.png)、[第二页](evidence/implementation/native-pdf/final-page2.png)均目视复核。切换手绘后，[整表圈和公式高亮](evidence/implementation/native-pdf/final-hand-page2-settled.png)也正确。翻页前后会议文件逐字节不变，两页 scene 均无缓存图片文件。 |
| 最终系统保存 | 实际保存 [164,884 字节 Excalidraw](evidence/implementation/native-pdf/exports/native-final.excalidraw)与 [1,211,936 字节 HTML](evidence/implementation/native-pdf/exports/native-final.html)。键盘填写完整路径，保存前 7 次采样覆盖 3 秒且目标不存在；文件修改时间位于明确 Save 点击期间。两次保存后 X11 只剩主窗口。 |
| 最终离线产物 | [13 项独立检查通过](evidence/implementation/native-pdf/final-offline-check.json)：两页 graph 与原生文件一致，底图内嵌且可解码，跨列圈分成两个局部圈，整表圈和公式编号保留，无 HTTP 请求或脚本错误。见[第一页](evidence/implementation/native-pdf/final-offline-page1.png)、[第二页](evidence/implementation/native-pdf/final-offline-page2.png)。 |
| 正常退出与冷启动 | 应用通过原生菜单退出，退出码为 0。移开测试用原 PDF 路径后，同一最终二进制在同一私有数据目录重启；会议文件仍逐字节一致，[手绘](evidence/implementation/native-pdf/final-cold-hand.png)及[自由编辑翻页](evidence/implementation/native-pdf/final-cold-free-page1.png)均正常。 |
| 回归与构建 | 最终 [310 项 Node/Eval](evidence/implementation/native-pdf/final-node-tests.txt)、[前端构建](evidence/implementation/native-pdf/final-build.txt)、[Linux locked/offline debug 构建](evidence/implementation/native-pdf/final-build/native-build.log)通过。生产浏览器通过新增 8 项、PDF 结构 83 项、文字方向 52 项、模式说明 27 项、七格式 92 项，以及自由场景改名、模型变化/撤销、翻页/重开/续会分支回归，见[运行日志](evidence/implementation/native-pdf/final-browser.txt)。 |

所有原生操作位于私有 Xvfb/Openbox、独立 D-Bus 和测试数据目录，使用真实 WebKitGTK 2.52.6 与系统文件对话框。最终二进制 SHA-256 为 `ee628acadb607e77c7b8bf767d5ef2bb2f79a5b72fee48df95e5d7e71049cfb1`，见[构建身份](evidence/implementation/native-pdf/final-identity.json)。最终检查前，仅将私有测试会议恢复到早先由真实界面导入、标注的快照，以去掉已撤回实验产生的缓存文件；[恢复记录](evidence/implementation/native-pdf/test-fixture-restored.json)与各阶段原始文件均保留。没有改写用户会议。

自动化边界保留在[验证备注](evidence/implementation/native-pdf/validation-notes.json)：最初第二个 Save 的可达性焦点曾重新显示旧对话框，错误文件名在提交前已发现；最终保存改用当前窗口的指针定位与真实键盘输入，且核对写入时间和 X11 窗口。最初回归器的重载模式与离线整区圈选择器也已修正，未将这些脚本问题当作产品通过。截图中的黑色光标块为 Cua 覆盖层。

归档含 620 个文件、43,252,236 字节，另附[哈希清单](evidence/implementation/native-pdf/manifest.json)。5 次测试运行均正常退出应用并清理私有显示服务，浏览器检查已结束。1423/1430 保持原预览进程；1430 当前入口 `/assets/index-DYLcTBAw.js` 和播放器均与最终磁盘产物一致，见[预览身份](evidence/implementation/native-pdf/preview-identity.json)。

本批原生样本覆盖多列中文、表格与公式，未补验旋转 PDF 的原生操作、原生缩放或触摸/笔。用户暂无测试 Mac；真实音频/模型、VoiceOver、签名更新、完整 G2/G3 与 G4 用户试用仍待验证。完整 UI/UX 重构继续，未提交或发布。
