# C58 全局与本会术语验证

2026-11-04，工作树基线 `4f9ab26`。本批修复会前术语丢失、连续替换改错词和格式错误没有反馈的问题，补齐 [C58](refactor-plan.md) 列出的 G1 路径。整体重构及 G3/G4 门槛仍未完成。

## 实际复现

在修改前的生产页面 `index-a6BkM9ti.js` 中填写本会术语，没有发言或画板内容时，等待超过自动保存间隔后 IndexedDB 仍没有会议；刷新后术语字段为空。随后实际输入 `probe | c sharp`，存下的文本将 `probe` 变成 `Core`、`c sharp` 变成 `C sharp`。原始输入保存在既有 `rawText` 字段中。

界面结果与源会议内容见 [基线结果](evidence/implementation/glossary/baseline-result.json)、[术语丢失截图](evidence/implementation/glossary/baseline-lost-glossary.png) 和 [错误改写截图](evidence/implementation/glossary/baseline-rewritten-text.png)。另对生产解析函数直接执行了 [定向反例](evidence/implementation/glossary/baseline-parser.json)：

| 输入 / 词条 | 原行为 | 本批行为 |
| --- | --- | --- |
| `Core = raw`、`raw = probe`；输入 `probe` | 先变成 raw，再变成 Core | 基于原句只替换一次，得到 raw |
| `C# = c sharp` | `#` 后都被当作注释，留下 C | 保留 C#；行首或空白后的 # 仍可写注释 |
| `Cache = ---`；输入 `.` | 空匹配插入名称，得到 `Cache.Cache` | 标出坏行，不生成空匹配规则 |
| `Broken = cash = ignored` | 悄悄丢弃第二个等号后的内容，仍应用前半条 | 整行不应用，保留文本并说明行号 |
| 两个不同名称都使用 `shared` 作为别名 | 按规则顺序选中第一个名称 | 保留 shared，避免无依据地改成某个名称 |

## 最小实现范围

产品改动在 `App.tsx`、`SettingsPanel.tsx`、`engine/glossary.ts` 和两份词典，没有改 Rust、依赖或模型配置。[本批差异](evidence/implementation/glossary/glossary-changes.patch) 基于修改前工作树生成。

- 现有保存判断把本会术语也视为需要保存的内容，继续使用同一自动保存、失败重试和切会屏障。术语草稿进入会议库的当前未保存项；没有转写或画板时，导出仍禁用，原生菜单也使用同一可导出判断。
- 共享解析器同时返回有效术语与坏行号，原有 `parseGlossary` 调用方式保留。重复名称/别名合并；缺名称、缺别名、多个等号或只有分隔符的写法不生效。两个字段独立显示中英文反馈，用 `aria-invalid` / `aria-describedby` 关联，保存原始草稿供修正。
- 替换只匹配原句，已有标准名称优先保留，含糊别名保持原文；长名称优先、英文边界和多语种规则保留。替换结果按普通文本返回，因此 `$&` 等名称不会被解释成替换指令。之后修改术语不重写已有记录。

## 真实浏览器结果

[可复跑检查](../../scripts/ui/check-glossary.cjs) 在实际 App、独立 localStorage/IndexedDB 和最终生产构建上通过 **30 项**，没有页面脚本错误，见 [检查结果](evidence/implementation/glossary/browser/glossary-check.json)。模型请求通过浏览器路由返回合成板书，没有调用真实模型；会议读写和界面操作使用实际代码。

覆盖了以下用户路径：

1. 只填本会术语即保存；刷新恢复；清空后再次刷新仍为空。
2. 实际 IndexedDB 写入路径注入 `QuotaExceededError`：保留完整术语草稿，会议库标为未保存，拒绝新建切会；恢复写入后通过“重试保存”写回。
3. 两级坏格式的独立行号/可访问描述、草稿保留、修复后的反馈清除，以及重复条目合并后送入模型的术语列表。
4. 单次纠正、C#、含糊别名、未生效坏行、保留 `rawText`、实际转写原文提示，以及模型 history 中的原始输入。
5. 新会议清空本会术语、保留全局术语；输入只应用当前作用域。分别从归档会议和当前会议“接着开”，携带对应术语；续会独立修改、重开源会和冷刷新均保持正确范围。
6. 中文 640×400 分类导航、错误描述、Tab 到本会字段、聚焦后编辑区域可见且页面无横向溢出。

[续会前原件](evidence/implementation/glossary/browser/source-before-continue.json) 与 [续会改动后原件](evidence/implementation/glossary/browser/source-after-fork.json) 所有字段相同；这项比较只针对续会及其独立编辑，没有将之后明确修改源会术语的操作算成“原件不变”。最终 [四场保存记录](evidence/implementation/glossary/browser/saved-meetings.json) 保留各自术语和转写。已人工查看 [英文错误提示](evidence/implementation/glossary/browser/glossary-invalid-lines.png) 与 [中文窄窗](evidence/implementation/glossary/browser/glossary-chinese-compact.png)。

## 回归与版本身份

| 检查 | 结果 | 范围 |
| --- | --- | --- |
| Node 定向单测 | 24 通过，0 失败、0 跳过 | glossary、phonetic、sessionRetry、sessionLifecycle、i18n；其中一个测试运行 45 条合成多语种样例 |
| 会议库生产回归 | 14 项通过 | 同一最终生产入口，包含未保存当前项、重命名失败、重试、搜索、导出和删除取消 |
| 原生外壳协议回归 | 通过 | 实际 App + 受控 IPC；启动、停止/保存失败、退出确认顺序、编辑历史和维护保存屏障 |
| SettingsPanel / Dialog 回归 | 通过 | 开发源上的实际组件 fixture；键盘、模态、分类、窄窗、关闭与焦点 |
| TypeScript / Vite 生产构建 | 通过 | 没有新增依赖或正式发行产物 |

本批最终入口为 `index-osV4AmK3.js`，SHA-256：`bed5ce91bb661569e3f2bc5d6a2dfb0cc9ed94893df4ea19ee8408e9b810a855`。见 [构建身份](evidence/implementation/glossary/build-identity.json)、[162 份源码/配置哈希](evidence/implementation/glossary/source-hashes.json)、[Node 原始日志](evidence/implementation/glossary/node-tests.log) 和 [外壳协议结果](evidence/implementation/glossary/regression/native-shell-check.json)。全部产品改动在这些最终检查之前完成，后续只整理证据和说明。

浏览器上下文已关闭，本批 1438 预览经 Ctrl-C 停止并确认端口不再监听；现有 1423/1430 服务保留，见 [清理记录](evidence/implementation/glossary/cleanup.json)。本批未启动真实原生应用，没有提交、推送、安装或发布。

[静态归档核对](evidence/implementation/glossary/archive-verification.json) 通过 37 项；[清单](evidence/implementation/glossary/manifest.json) 包含 53 份文件、1,339,440 字节，不含清单自身。它核对产物、源码身份、结果和清理记录，不重放 UI。

未重复完整 Node/Rust 或此前导出矩阵；未验证真实 ASR/模型推理、音频采集、macOS/Windows 和 VoiceOver。45 条合成文本回归不代表音频识别准确率。设置仍沿用现有即时应用方式，本批没有新增独立保存按钮或跨设备术语同步。
