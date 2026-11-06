# C44/C50：Excalidraw 导入与往返保存

2026-11-05，基线 `4f9ab26` 的当前 worktree。本批修复外部绘图中的重复文字、同名分组和并行箭头导致的导入失败或合并，并保留第三方 `customData` 与原始随机笔触参数。最终生产浏览器 47 项绘图检查、21 项导入回归、工作区回归、318 项 Node/Eval 测试及 Linux 构建通过；真实 Linux 文件选择、导出再导入和冷启动完成 [29 组证据核对](evidence/implementation/native-drawing/evidence-check.json)。

## 问题与修复

| 输入 | 修复前的实际结果 | 最终行为 |
| --- | --- | --- |
| 重复便签、`Service` / `SERVICE` / `Service（2）` | 对象 ID 冲突，提示自由编辑数据无效 | 复用模型的名称规范化规则分配独立身份，保留原始显示文字 |
| `Team` / `TE AM` / 未命名 frame | 三个 frame 仅得到两个结构分组，未命名 frame 映射缺失 | 三个独立分组及各自成员、frame 引用保持 |
| 两条箭头连接同一对节点 | 两个对象映射到同一连线 ID，导入失败 | 一条映射为结构连线，另一条连同文字和路径保留为自由对象 |
| 手绘显示导入的箭头标签 | 原始文字和生成文字同时显示，画面出现重复 | 共用过滤规则隐藏被原图覆盖的标签及标签背景，并区分相邻连线 ID |
| 重新导入便签和话题 | 将话题及便签合计成“框”，数量提示不符 | 分别统计框与便签，混合图保持 3 个框、3 段文字 |
| 第三方 `customData` | 扩展字段被覆盖 | 保留扩展字段，仅重建 Boardcast 自有映射 |
| 原始随机笔触参数 | 首次生成结构视图时被新参数替换 | 导入时显式保留 `seed`，粗糙线条的原始参数随保存、导出和重开保持 |

生产修改仅涉及 `importExcalidraw.ts`、`freeScene.ts` 的首次导入及共用手绘过滤分支，没有新增依赖或改变存储格式。四个新增单元用例先在旧实现失败；随后桌面导出字段比对发现笔触参数问题，再以 [234 与 223 不符的失败断言](evidence/implementation/native-drawing/node-seed-before.log)定位并修复。随后截图复核还发现重复生成标签，[旧生产页面两项检查失败](evidence/implementation/native-drawing/browser-overlay-before/check-drawing-import/check.json)，最终显示及相邻连线过滤回归通过；重新导入数量也有先失败后通过的单元断言。完整本批差异留在 [source-diff](evidence/implementation/native-drawing/source-diff/importExcalidraw.ts.patch)。

## 实际检查

| 层级 | 范围与证据 |
| --- | --- |
| 生产 Chromium | 五组绘图：重复文字、分组、并行箭头、扩展数据、混合图。逐个检查预览不写旧会议、新会保存、双视图切换、真实下载、重新导入及刷新恢复；坏 JSON、未知形状、外部图片 URL、重复 ID 被拒绝且不改变旧记录。[47 项结果](evidence/implementation/native-drawing/browser-verified/check-drawing-import/check.json) |
| 现有流程回归 | [21 项导入检查](evidence/implementation/native-drawing/browser-verified/check-import-task/import-task-check.json)及[工作区检查](evidence/implementation/native-drawing/browser-verified/check-workbench/workbench-check.json)，覆盖保存失败恢复、绘图往返、键盘菜单及四个窗口尺寸。模型回复仅在既有回归中受控；绘图导入不需要模型。 |
| Linux 导入 | 混合绘图的[旧版导入报错](evidence/implementation/native-drawing/before-rich-result.png)，最终显示 [19 个可编辑对象](evidence/implementation/native-drawing/verified-rich-preview.png)，显式新开会议后保存。19 个原始对象全部保留，包括三个便签、三个 frame、两条箭头、自由矩形、笔迹和一张小 PNG。 |
| Linux 双视图与导出 | 真实进入[自由编辑](evidence/implementation/native-drawing/verified-free.png)并返回[手绘](evidence/implementation/native-drawing/verified-hand.png)，通过 GTK 保存框生成 [native-rich-verified.excalidraw](evidence/implementation/native-drawing/fixtures/native-rich-verified.excalidraw)。检查几何、样式、随机笔触、文字、分组引用、箭头绑定、扩展字段和图片原始字节。 |
| Linux 再导入与冷启动 | 实际文件选择器重新导入导出的文件，再正常退出并重新启动同一最终二进制。[冷启动画面](evidence/implementation/native-drawing/verified-cold-restored.png)显示已保存的再导入会议；[22 个会议、附件和配置文件](evidence/implementation/native-drawing/verified-cold-check.json)逐字节保持。 |

原来的三场合成会议、六张图片附件及空远程配置，共 [10 个原件](evidence/implementation/native-drawing/persistent-originals-final.json)始终未改变。中间版本创建的四场测试会议也没有被最终版本自动重写；最终导入、再导入各新建一场。所有资料位于独立 XDG 目录，未接触用户实际会议、模型或物理音频。

## 构建和证据

最终 Linux debug 二进制 SHA-256 为 `b38cff5037cecf6de94100d0f97bd029cd869a42172c68f93cb7f5aa472ca235`；前端为 `index-DN0EXjPr.js`，SHA-256 为 `0561bcd9034943cdb26f6a9809cf499fe1cd885fb814609606af2ce39bf604ed`。见[构建身份](evidence/implementation/native-drawing/fixed-build/identity.json)。运行中的应用、冷启动应用及 1430 返回的前端字节均已核对；[199 个输入文件](evidence/implementation/native-drawing/source-final-check.json)与最终构建一致。

[318 项 Node/Eval](evidence/implementation/native-drawing/node-verified.log)、类型检查、播放器及应用生产构建、locked/offline Linux debug 构建通过。七次应用均先正常退出，再停止私有桌面；包含一次启动路径纠正所产生的额外旧版实例。[42 个登记进程](evidence/implementation/native-drawing/final-cleanup.json)已结束，无隔离环境残留，1423/1430 原预览服务保留。

[证据核对脚本](evidence/implementation/native-drawing/check-evidence.py)只读取归档，不重放 UI 或启动服务。[文件清单](evidence/implementation/native-drawing/manifest.json)排除了二进制、模型和浏览器资料目录；初次检查脚本、Cua 输入与 GTK 焦点的纠正见[运行说明](evidence/implementation/native-drawing/harness-notes.json)。部分截图中的黑色形状是 Cua 光标覆盖层。

## 保真边界与剩余项

这是固定合成文件的验证，不表示任意未来 Excalidraw 字段、嵌入网页、超大绘图或所有图种都已验收。保留的是受支持对象及其扩展数据；对象 ID、内部版本信息及 Boardcast 映射会调整，不能宣称整个 JSON 字节一致。结构模型仍只表达每对节点的一条有向连线；额外箭头保持为可编辑自由对象。历史导入已丢失的扩展字段或笔触参数不会被自动补回。

原生日志保留 GTK 菜单/事件警告，以及退出阶段的 WebKit 内部载入错误；本轮保存、实际产物、正常退出码和冷恢复均通过，没有宣称底层日志无错。未修改 Rust，因此未重复无关 Rust 测试。

用户暂无测试 Mac；macOS/Windows、VoiceOver、实际音频、签名安装更新及 G3/G4 仍未验收。本批未安装应用、提交 Git、推送或发布。可直接打开 [1430 应用预览](http://127.0.0.1:1430)。
