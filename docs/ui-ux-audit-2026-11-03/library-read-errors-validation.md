# 会议库读取失败与旧副本恢复

2026-11-05，继续落实 [方案 §5.1](refactor-plan.md)：读取失败必须与空会议库、搜索无结果分开。本批涉及 7 个产品文件和 2 个测试文件；[补丁](evidence/implementation/library-read-errors/product-and-tests.patch)相对本批开始时的工作区生成，保留之前的 UI 和 Windows ARM 修复。

## 问题与改动

真实 Linux 基线含 7 个合成会议文件，其中 4 个 JSON 截断。会议库只显示 3 个普通条目，没有读取错误提示；点击完好的旧版记录，会先读同编号的损坏新版并报 `Unexpected EOF`。[基线窗口](evidence/implementation/library-read-errors/baseline-library.png)、[打开失败](evidence/implementation/library-read-errors/baseline-legacy-error.png)。

- 原生列表保留文件级读取错误，校验记录编号与文件名一致；读取目录失败仍返回错误。严格模式继续拒绝遗漏记录的纠错样本导出。
- 会议库显示失败数量、可展开的文件详情和重新读取入口；健康记录仍可打开、搜索。全是坏文件时不显示“没有保存过的会议”。详情路径可以换行，中英文均有操作说明。
- 同编号的不同版本分别保留错误。新版损坏但旧版可读时，复用既有“旧版本副本”路径，以新编号恢复，清除原存储修订号；打开前先保存，失败则停留在会议库。原文件不被覆盖。
- 保留原有改名焦点、未保存搜索、列表操作和持久化机制；没有增加依赖。

## 验证

| 检查 | 结果 |
| --- | --- |
| 前端与 Eval | [321 项通过](evidence/implementation/library-read-errors/node-tests.log)。新增用例在旧实现先因读取损坏新版失败；修复后覆盖两版本错误保留、健康搜索、严格导出拒绝、新编号及修订号重置、原记录保留。 |
| 原生存储 | [13 项通过](evidence/implementation/library-read-errors/rust-tests.log)。目录当作记录、截断 JSON、缺失/不匹配/非法编号均报告；删除标记仍生效，健康记录的 IPC 字段保持兼容。 |
| 生产界面 | [新增 14 项](evidence/implementation/library-read-errors/browser/check-library-read-errors/report.json)及[原有 20 项](evidence/implementation/library-read-errors/browser/check-library/library-check.json)通过。覆盖读取失败与空状态区分、搜索、恢复写入失败重试、外部修复后重新读取、中英文、720 像素及 200% CSS 缩放；无未捕获错误。新增界面检查使用模拟原生 IPC。 |
| 构建 | TypeScript、生产前端、Linux Tauri 离线锁定构建、Rust 格式及差异空白检查通过；原生构建使用的 11575 个前端文件与本次生产构建字节相同。[构建身份](evidence/implementation/library-read-errors/identity.json)、[核对](evidence/implementation/library-read-errors/frontend-equivalence.json)。 |

首次界面脚本在搜索尚未开始时过早检查旧列表，已改为等待搜索状态出现并结束；[失败证据](evidence/implementation/library-read-errors/search-timing-failure/failure.txt)保留。随后测试揭示旧副本虽显示已保存、实际尚无新文件的缺口，[窗口证据](evidence/implementation/library-read-errors/unsaved-copy-failure/failure.png)保留；本批加入打开前保存后通过，没有放宽持久化断言。

## 真实 Linux 桌面

使用独立 XDG 数据、显示和 D-Bus，真实 Tauri/WebKit 和文件系统；所有会议均为合成测试记录，不使用用户会议，也不启动模型或物理音频。

1. [会议库](evidence/implementation/library-read-errors/final-library.png)显示 4 个文件错误和 3 个可用条目；[展开详情](evidence/implementation/library-read-errors/final-error-details.png)、[720 像素窗口](evidence/implementation/library-read-errors/final-narrow-errors.png)、[健康记录搜索](evidence/implementation/library-read-errors/final-search-result.png)均已实际操作。
2. 临时撤销测试目录写权限，恢复旧副本时显示真实 `Permission denied`，保持会议库和全部原文件；恢复权限后点击同一条目重试。[错误窗口](evidence/implementation/library-read-errors/final-write-failed.png)、[失败时文件核对](evidence/implementation/library-read-errors/failed-save-proof.json)。
3. [成功窗口](evidence/implementation/library-read-errors/final-recovered-stable.png)恢复 1 行原话和 4 个白板节点。新文件为 v2、修订号 1，图、原话、历史及页面与旧版一致；7 个原文件 SHA-256 均未变。[恢复核对](evidence/implementation/library-read-errors/recovery-proof.json)。
4. 正常退出后重新启动，[原话和白板恢复](evidence/implementation/library-read-errors/cold-recovered.png)，包含恢复副本的 8 个 JSON 文件逐字节不变。[冷启动核对](evidence/implementation/library-read-errors/cold-proof.json)。
5. 把一个合成坏文件替换为完好测试记录，点击重新读取，错误从 4 个降到 3 个且新条目出现。[实际窗口](evidence/implementation/library-read-errors/cold-repaired.png)。退出后撤销这次测试注入，8 个文件仍与恢复核对一致。

基线、过程构建、最终恢复和冷启动共 4 次运行均正常退出，28 个登记的测试进程已结束。[清理与最终文件核对](evidence/implementation/library-read-errors/final-proof.json)。截图中的黑色光标覆盖物来自 Cua；原 1423/1430 预览服务保留。

## 边界

本批不自动修复截断的原文件；恢复依赖仍完好的旧副本或用户备份。没有覆盖所有语义损坏的 JSON，也没有增加全量会议 schema 校验。

Windows ARM 的本地请求修复见[专门记录](windows-arm-local-model-validation.md)，仍需原机器复测，未发布新安装包。用户暂无测试 Mac；Mac、物理音频、读屏和完整 G3/G4 验收继续保留，完整 UI 重构尚未完成。
