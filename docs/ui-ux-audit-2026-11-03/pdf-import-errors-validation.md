# C41：PDF 失败恢复、取消与迟到读取

2026-11-05，基线 `4f9ab26` 的当前 worktree。本批在生产 Chromium 中复现极高页面和舍入后高度为零的页面：浏览器未能生成有效图片，导入却继续读取像素，最终显示底层内存或尺寸异常。现在先检查绘图上下文及编码结果，失败时指出具体页码，并提供裁剪或调整页面尺寸后重新导入的中英文指引。原有会议保持，第二页失败也不会留下第一张临时白板。产品修改仅涉及 `importPdf.ts` 和两份语言文件，没有新依赖或存储格式变化。

[旧版 60 项检查](evidence/implementation/pdf-import-errors/browser-before/check-pdf-import-errors/check.json)中，12 项失败，集中在三种异常页面的错误提示与无效像素读取；[最终 60 项全部通过](evidence/implementation/pdf-import-errors/browser-final/check-pdf-import-errors/check.json)。检查使用实际 PDF.js、Canvas、PDF 文件及独立 IndexedDB；仅为迟到读取场景控制 `File.arrayBuffer()` 完成或拒绝，解析和编码结果没有替换。

| 场景 | 实际结果 |
| --- | --- |
| 密码保护与坏文件 | qpdf 生成的 AES-256 文件提示先移除密码；坏文件保留 PDF.js 原因。原文件重试仍显示对应错误，取消后可选择有效文件。两种语言下已保存记录保持。 |
| 极高、极扁及第二页失败 | 100×50000 点、1000000×1 点及正常页后接异常页的样例，均在提交前指出第 1/2 页无法生成有效图片。无效页面不再进入 `getImageData()`；不创建部分会议。 |
| 窄窗恢复 | 在 640×400 的中英文界面中，取消和重试都能通过键盘聚焦并完整显示。见[中文错误界面](evidence/implementation/pdf-import-errors/browser-final/check-pdf-import-errors/zh-compact-error.png)。 |
| 取消与切会 | 取消尚未完成的文件读取后，新建另一场导入会议；旧读取迟到成功或失败均不写入新会，也不启动 PDF 解析线程。41 页 PDF 实际解析过程中取消，旧记录和当前会议保持。 |
| 大文件与后续恢复 | 大于 64 MiB 的真实 PDF 成功进入预览，取消不写旧会议。所有失败与取消之后，正常 PDF 仍可追加、保存和显示底图。见[恢复画面](evidence/implementation/pdf-import-errors/browser-final/check-pdf-import-errors/zh-recovered.png)。 |

最终记录的 25 个 PDF 解析线程全部终止；两个语言环境均无无效尺寸像素读取和未处理的页面异常。大文件样例是在一个黑色矩形后包含 64 MiB 空白内容流，验证字节加载和解析，不代表巨型内嵌位图的解码能力。两个大文件未复制到文档目录，其大小、SHA-256 及再生成方式分别留在[输入清单](evidence/implementation/pdf-import-errors/fixture-files.json)和[运行说明](evidence/implementation/pdf-import-errors/harness-notes.json)。其余小型 PDF 样例随证据保留。

现有 [21 项导入回归](evidence/implementation/pdf-import-errors/browser-final/check-import-task/import-task-check.json)、[52 项 PDF 方向与标注回归](evidence/implementation/pdf-import-errors/browser-final/check-pdf-directions/pdf-directions-check.json)、[15 项定向 Node 检查](evidence/implementation/pdf-import-errors/node-checks.log)、类型检查及生产构建通过。复用了既有扫描 PDF 生成器，默认输入保持原字节；新增可调尺寸和内容流仅用于这些实际文件测试。

前端为 `index-TD_zekd-.js`，SHA-256：`b3e517eb9b82d0c4c818153da49ca1a276f9f341f6cd79cb0c182b97228dfa10`。已核对 1430 实际返回的字节及 200 个源输入，见[构建与环境记录](evidence/implementation/pdf-import-errors/identity.json)。本轮独立浏览器由 runner 关闭，1423/1430 原预览进程保留。[归档清单](evidence/implementation/pdf-import-errors/manifest.json)不包含大文件、浏览器资料或应用二进制。

本批属于 G1，没有重建或运行原生安装包，也没有重复无关 Rust/Node 全量测试。没有新增统一文件尺寸上限，未声称覆盖任意解码错误、内嵌巨型图片或各平台内存极限。用户暂无测试 Mac，G3/G4 与真实音频继续保留；未提交、推送或发布。
