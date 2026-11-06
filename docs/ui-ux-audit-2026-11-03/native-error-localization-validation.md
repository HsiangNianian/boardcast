# C61 续批：原生存储保护与图片错误

2026-11-04，工作树基线 `4f9ab26`。本批继续处理上一批留下的原生错误和图片完整性提示，不关闭 C61 或整体重构目标。

## 问题与修复

生产 App 在受控原生回复下复现五类中文界面混入英文的提示：会议已删除、原生 revision 冲突、新格式覆盖保护，以及图片缺失和校验不符。[基线记录](evidence/implementation/native-error-localization/baseline-browser/check.json) 包含十二组中英显示结果；59 项保留/恢复断言通过，最终文案断言按预期失败。英文中两条原生文本与前端文案不一致，本批统一为已有前端文案。

`meetings.ts` 的现有原生调用入口只识别三条固定保护错误，使用已有字典。未知错误仍原样抛出，包括包含相同文本前缀但附带其他诊断的错误。`AssetIntegrityError` 的四种原因使用字典生成文字，保留 `issue.key/reason` 和原生 `ASSET_HASH_MISMATCH:` 分类信号；英文图片文案保持原样。

没有改变 Rust、错误分类、存储格式、图片校验、文件锁或保存顺序。四个产品文件和新增断言见 [本批补丁](evidence/implementation/native-error-localization/product-and-tests.patch)。固定原生文本目前是匹配依据，后端更改这些返回值时必须同步更新，代码中已标明这一限制。

## 已运行检查

| 项目 | 证据与实际范围 |
| --- | --- |
| 生产 App，60 项 | [独立运行结果](evidence/implementation/native-error-localization/standalone-browser/check.json)：中英文各六种失败，已知错误本地化、未知错误原文保持；拒绝写入后原记录和草稿保留，重试/切会不能越过失败，Markdown 保留原话及未保存文字；图片恢复后可保存原编辑和图片引用。原生回复与模型结果受控，Markdown 内容取自导出调用参数，不称为真实系统保存 |
| 图片回归，14 项 | [真实 Chromium 解码与 IndexedDB](evidence/implementation/native-error-localization/asset-probe/asset-integrity-check.json)：错哈希、损坏图片、历史资源、缓存隔离、恢复重试与 5000.7 ms 解码超时 |
| 关闭/救援回归 | [原有检查](evidence/implementation/native-error-localization/close-regression/missing-assets-close-check.json)：缺资源的只读浏览可关闭；编辑后拒绝退出，保留原件和草稿，并可导出可用文字 |
| Node 与构建 | [24 项 Node](evidence/implementation/native-error-localization/node-tests.txt)、类型检查、播放器/主应用生产构建和 Linux Tauri debug 构建通过。没有重新运行完整 Node 或 Rust 测试集合 |

初次组合浏览器运行器在输出 60 项通过后未结束，已终止其确切进程及子进程，未把该运行器记为成功。图片和关闭回归分别独立完成，随后原 60 项脚本独立运行并关闭浏览器，退出码为 0。另按现有运行器的显式 `context.close()` 方式复核，60 项通过后 context 数归零、浏览器关闭，退出码为 0，未重现首次停滞。详见 [运行说明](evidence/implementation/native-error-localization/harness-notes.txt)、[独立运行日志](evidence/implementation/native-error-localization/standalone-browser.txt) 和 [context 清理复核](evidence/implementation/native-error-localization/context-close-check.txt)。

## 真实 Linux 原生路径

在独立 XDG、D-Bus、Xvfb `:90` 和应用标识下，通过 cua-driver 操作实际 Tauri 窗口；没有控制用户当前桌面。后台语言点击未生效后，以新截图定位在隔离显示器内前台点击，并确认设置及原生菜单标签切换成中文。

1. 种入一页真实 v2 会议，保留图片引用但缺少文件。打开后新增第二页并尝试切会，应用显示[中文缺图错误](evidence/implementation/native-error-localization/native-missing-error.png)，当前会议和新增页仍在，原 JSON 字节未改。恢复原 SVG 后点击“重试保存”，实际磁盘变为 revision 2、两页，原图片引用和原话保持。冷启动重开后两页仍在，缺图提示消失，中文偏好保持。见 [保留](evidence/implementation/native-error-localization/native-missing-preserved.json)、[重试保存](evidence/implementation/native-error-localization/native-repaired-save.json)、[重开核对](evidence/implementation/native-error-localization/native-reopened-check.json)。
2. 外部辅助进程持有同一会议文件锁，原生 App 新增第三页并提交。观察到该 App PID 的内核 `FLOCK` 等待者后，辅助写入者在锁内提交 revision 3，再释放锁。原生后端拒绝旧 revision，显示[中文冲突与恢复指引](evidence/implementation/native-error-localization/native-cas-error.png)；重试后另一写入者的 JSON/原话及图片仍未改动，当前第三页保留。该路径是“真实原生应用＋持锁文件写入者”，不是两个原生窗口。见 [锁与提交记录](evidence/implementation/native-error-localization/cas-race.json)、[原件核对](evidence/implementation/native-error-localization/native-cas-preserved.json)。
3. 从该冲突状态点击“导出副本”，通过真实系统保存对话框生成 HTML，保存对话框关闭，App 报告实际文件路径。文件含自己的三页状态、原话及嵌入图片；另一写入者新增的原话仍只在其保存版本中。随后用英语环境的独立 Chromium 禁网打开，中文控件、三页数据、原图和原话均保留，零网络请求和脚本错误。见 [实际文件核对](evidence/implementation/native-error-localization/native-rescue-file.json)、[离线核对](evidence/implementation/native-error-localization/native-offline-check.json)、[离线截图](evidence/implementation/native-error-localization/native-offline-recovery.png)。

## 身份、清理和剩余范围

最终生产入口为 `index-Cq1Pge3H.js`，SHA-256 `c8fd2430e07bd719ec45ca5dcd09233955df01afd3041224ddf032144d345c96`；实际原生二进制 SHA-256 `ae867e3d296b431291972219ed067fd187f887b8af60ef55e87425925bb70ee9`，包含该入口。189 个源码/构建文件哈希在构建及运行期间保持，私有项目 Rust、Cargo、build.rs 和基础 Tauri 配置与工作树一致；详见 [构建身份](evidence/implementation/native-error-localization/identity.json)。

这次真实原生窗口未分别触发删除、新版本覆盖保护、图片写入竞态或所有其他原生错误；三条固定回复和图片异常的其余组合仅由页面协议及单测覆盖。原生模型状态仍观察到英文“未安装模型”提示，后续继续处理。菜单标签切换不等于所有菜单/快捷键验收，Mac/VoiceOver、其他原生状态、其他导出过程中的语言切换及 G3/G4 仍待验收。

未提交、推送或发布。测试窗口、Cua 会话、私有显示器、辅助进程和预览均已清理，保留用户 1423/1430 服务，见 [清理记录](evidence/implementation/native-error-localization/cleanup.json)。
