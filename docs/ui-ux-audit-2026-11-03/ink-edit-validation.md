# C26 手绘板改名、选择与编辑

2026-11-06，基线 `4f9ab26` 的当前工作区。补查选择工具条的改名、移动、删除与撤销，修复无变化提交、空节点名称、输入法组合键、返回焦点，以及清空连线文字后的场景残留。

## 修复与范围

- 未修改名称也会新增“已改名”原话和回放步骤。共享 `rename()` 现在沿用已有名称归一化规则，无变化或选中对象已不存在时返回原图；空节点名称继续留在编辑框内，空连线文字仍可提交。
- 提交或 Esc 后焦点丢失；组合输入中的 Esc 还会关闭整个改名框。复用内容列表已有的组合键处理，退出编辑时先结束原生输入焦点，再返回 Rename 按钮。无需鼠标即可继续按 Enter 改名。
- Linux 原生检查发现第二处问题：清空连线文字后，空白文本被场景合并保留为无归属的自由对象，内容列表显示“未命名”。现在删除对应标签及旧覆盖值；立即或稍后恢复文字，都保持可见、与连线关联。[原生修复前](evidence/implementation/ink-edit/native-first/edge-commit.png)、[修复后](evidence/implementation/ink-edit/native-final/commit-edge-clear.png)。

产品仅改 `App.tsx`、`engine/edit.ts`、`board/freeScene.ts`，另加两处单元测试和一份生产检查。已检查工具条、内容列表、场景编辑/保护/恢复及相关测试调用；没有新增依赖、存储格式或翻译键。[本批差异](evidence/implementation/ink-edit/batch.diff)与[源文件哈希](evidence/implementation/ink-edit/source-hashes.json)不混入此前批次改动。

## 浏览器与逻辑检查

[`check-ink-edit.cjs`](../../scripts/ui/check-ink-edit.cjs) 通过生产 App 导入真实 Excalidraw 文件，使用实际 IndexedDB 和编辑器。最终 [1430 预览的 30 项检查](evidence/implementation/ink-edit/browser/report.json)通过：

- 节点和连线无变化提交不新增记录；空节点名称不可提交，改名保留真实旧名且只记一步；提交/取消后返回键盘焦点。
- Chromium 组合输入事件中的 Enter/Escape 不提交或关闭草稿；这不是原生输入法候选验收。
- 初始化自由编辑场景后，清空连线文字不留下空对象，恢复文字仍属于原连线。
- 主题移动完整保留 65×35 位移及子树，一次撤销/重做恢复完整操作；删除连线不删节点，删除主题及后代也可一次撤销。
- 固定资料块只开放标记，键盘 Delete 不能删除；1280×800、900×600、720×480 的实际工具条保持在画布内。
- 冷重载保留图数据、位置和时间线，没有未捕获应用错误。

原构建的 28 项检查中有 [10 项失败](evidence/implementation/ink-edit/baseline/browser/report.json)；第一轮修复后，新增场景检查又明确复现 [2 项失败](evidence/implementation/ink-edit/baseline/scene/report.json)。两项新增单元测试也先失败。最终 [42 项 Node 测试](evidence/implementation/ink-edit/node-tests.log)、[内容列表及场景往返](evidence/implementation/ink-edit/related/browser.log)、TypeScript、播放器、主应用及 Linux 原生构建通过。构建仍有既有大 chunk 提示。

```sh
node --test src/engine/edit.test.ts src/engine/freeScene.test.ts src/engine/corrections.test.ts src/engine/steps.test.ts src/engine/toolbar.test.ts
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-ink-edit.cjs
```

检查脚本早期把 Locate 后画布外的文字当成可点击目标，还错误限定标签必须属于手绘路径；改用内容列表定位与真实选择框后，完整位移与撤销通过。没有因此修改产品拖动逻辑。过程说明见 [notes.json](evidence/implementation/ink-edit/notes.json)。

## Linux 原生窗口

使用 cua-driver、独立 XDG/D-Bus/Xvfb、私有应用标识及合成会议，没有访问用户会议或物理音频。

1. 第一轮构建通过真实文件选择器导入 Client → Service。实际键盘验证未改名、空名、Esc、焦点返回、改为 Native client，以及一次撤销/重做；改名只新增一条原话和一步回放，旧名 Client 保留为别名。[空名画面](evidence/implementation/ink-edit/native-first/empty-submit.png)、[返回后直接 Enter](evidence/implementation/ink-edit/native-first/cancel-focus.png)、[实际记录核对](evidence/implementation/ink-edit/native-proof.json)。清空连线时发现残留，随后撤销并正常退出。
2. 重建最终二进制，恢复上一步实际保存的会议。清空 requests 后，箭头和两节点保留，已保存场景没有空标签或残留覆盖值；再输入 delivers，标签恢复语义归属，切换自由编辑可见。[最终窗口](evidence/implementation/ink-edit/native-final/free-edit.png)、[最终构建身份](evidence/implementation/ink-edit/native-final/identity.json)。
3. 正常退出后，用同一二进制和实际写出的数据冷启动。Native client、delivers 及内容列表均恢复，无“未命名”对象。[冷启动画面](evidence/implementation/ink-edit/native-cold/contents.png)。一份会议、两份 SVG 素材及锁文件共四个文件，在最终退出、冷启动和再次退出期间逐字节一致。

三次原生运行均正常退出，登记的 54 个测试进程已结束，私有预览也已关闭，见 [cleanup.json](evidence/implementation/ink-edit/cleanup.json)。原生输入采用实际英文键盘；本批没有运行该字段的原生中日输入法候选。

1430 更新至 `index-BpNJ40jS.js`，资源先写入、入口最后替换，原有 1,336 个资源文件保持；HTTP 内容与最终构建哈希一致。[同步记录](evidence/implementation/ink-edit/publication.json)。1423/1430 原进程继续提供预览。

## 保留边界

此修复防止新增无归属空标签，没有自动迁移或删除旧会议里已经存在的自由文本。未重复全量 Node/Rust 测试，没有制作或发布安装包。实际中日输入法、物理触屏、Mac/VoiceOver 和完整 G3/G4 仍待验。

Windows ARM 本地通道与预热修复、此前 Linux 真实模型验证见[独立报告](windows-arm-local-model-validation.md)；当前 `0.21.1-5` 安装包不含修复，仍需新 Windows ARM 构建在原 Adreno 机器复测。本批原生检查没有启动模型或验证 GPU。
