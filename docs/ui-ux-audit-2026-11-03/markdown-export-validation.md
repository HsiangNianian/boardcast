# C46 Markdown 整场输出

2026-11-06，基线 `4f9ab26` 的当前工作区。本批修复纯文字被解释成 Markdown、公式反引号截断、Mermaid 标签破坏代码块，以及摘要空行被合并的问题。

## 改动

产品仅修改 `src/board/markdown.ts`：标题、说话人、原话、节点、批注、范围标记和关联文字共用转义；字段换行留在原标题或列表项内。公式使用比内容更长的反引号边界，保留首尾空格和多行。Mermaid 的节点、分组、连线文字使用实体，标签里的三反引号不会再提前结束整个流程图代码块。

摘要仍作为原有 Markdown 输出；移除整份文档的连续空行合并，避免改变摘要代码块。保留既有板号、编辑记录、草稿排除和自由对象遗漏提示，不改变会议数据、播放器、原生保存接口或依赖。

已核对 `toMarkdown` 的唯一产品入口 `exporter.build('md')`、App 的当前/回放快照，以及摘要、语言、原文批注、标记测试。两处既有导出检查改为读取实际 Markdown 渲染结果，避免把合法的下划线转义误判为文字丢失。[本批差异](evidence/implementation/markdown-export/batch.diff)、[文件哈希](evidence/implementation/markdown-export/source-hashes.json)。

转义依据 [CommonMark 的反斜杠规则](https://spec.commonmark.org/0.31.2/#backslash-escapes)、[代码跨度规则](https://spec.commonmark.org/0.31.2/#code-spans)及 [Mermaid 的实体语法](https://mermaid.js.org/syntax/flowchart.html#entity-codes-to-escape-characters)。实际测试使用已有 micromark/GFM 和 Mermaid 11.17.2，未新增运行库。

## 实际产物

新增 [`check-markdown.cjs`](../../scripts/ui/check-markdown.cjs) 使用生产 App、真实 IndexedDB 和三张合成白板，从可见导出菜单下载中英文 Markdown，再用独立的 Markdown 和 Mermaid 渲染器核对：

- 中日英混合文字、字面 Markdown/HTML/实体、首行编号、反引号公式、原文多行和批注。
- 三张板（含当前空板）、旧记录默认第 1 板、跨页后返回原板、既有编辑记录与自由对象遗漏提示。
- 摘要原格式及连续空行、原会议导出和重载前后完全相同、无应用未捕获错误。

旧生产构建的 [24 项对照](evidence/implementation/markdown-export/baseline/report.json)中 18 项失败；最终 [24 项检查](evidence/implementation/markdown-export/browser/report.json)全部通过。实际查看了[旧版渲染](evidence/implementation/markdown-export/baseline/zh-rendered.png)和[修复后渲染](evidence/implementation/markdown-export/browser/zh-rendered.png)：旧版流程图报语法错误，后半场记录落入代码块；修复后流程图、中文原话、板号及遗漏说明均正常。原始下载文件：[中文](evidence/implementation/markdown-export/browser/zh.md)、[英文](evidence/implementation/markdown-export/browser/en.md)。

最终针对性 Node 测试 39 项通过（Markdown 7 项，其中新增 5 项；含摘要、原文、标记、语言和本地模型通道）。通过加载保留的原导出函数运行同组 Markdown 测试，新增 5 项均先失败。[修复前](evidence/implementation/markdown-export/unit-baseline.log)、[最终日志](evidence/implementation/markdown-export/unit-tests.log)。

最终构建另通过七种格式 92 项、保存交接 70 项、原文标注 59 项、中日英图形 28 项、存储错误恢复 60 项，以及工作台的纯转写导出/保存恢复。TypeScript 和主应用构建通过，播放器与前一构建字节相同。[相关生产日志](evidence/implementation/markdown-export/related/browser.log)、[各检查摘要](evidence/implementation/markdown-export/verification.json)。

1430 已更新到 `index-DaFagoTZ.js`；两次同步共新增 148 个资源，批次开始时的 1,484 个资源保持，入口最后替换，最终 11 个 HTTP 资源核对一致。1423/1430 原进程继续提供预览；本批私有预览结束。[同步](evidence/implementation/markdown-export/publication.json)、[清理](evidence/implementation/markdown-export/cleanup.json)。

```sh
node --test src/engine/markdown.test.ts src/engine/summary.test.ts src/engine/regions.test.ts src/engine/highlight.test.ts src/engine/i18n.test.ts src/engine/localTransport.test.ts
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-markdown.cjs
```

测试校准与中间运行单独记录在 [notes.json](evidence/implementation/markdown-export/notes.json)。最初测试误设“编辑记录应删除”；核对既有会议导出后保持原记录。初版转义覆盖普通句点和词内连字符，最终仅在行首处理这两种可能形成列表的符号，词内连字符和句末句点保持原样；没有为此放宽原文、纯转写或存储恢复的既有检查。

最后补查到独立的 `1.` 会被渲染为空列表；[补充对照](evidence/implementation/markdown-export/number-baseline.log)复现后，将行尾纳入原有编号转义条件。最终逻辑测试及公开预览上的上述生产回归通过，下载文件来自最终入口。

## 保留边界

本批覆盖 C46 的 G1 场景；HTML 被禁用或不支持 Mermaid 的外部阅读器可能显示标记或流程图源码，未声称任意阅读器视觉一致。没有重建播放器或原生二进制，没有重跑全量 Node/Rust，也未制作安装包。

同轮重新读取用户的 Windows ARM 诊断包，并通过本地请求通道的针对性逻辑及 10 项生产协议回归。该包仍为 `0.21.1-5`、Adreno X2-90、模型 `ready`、前端 `Failed to fetch`；请求通道的已有修复见 [Windows ARM 记录](windows-arm-local-model-validation.md)。旧安装包不含修复，新 Windows ARM 安装包/原机、Mac 实机及完整 G3/G4 继续保留。
