# C61 续批：保存、绘图与启动失败提示

2026-11-04，工作树基线 `4f9ab26`。本批修复已复现的前端英文错误提示和启动失败误报，继续保留 C61 的原生及其他未验收边界。

基线生产页面在中文界面显示 `导入失败：Invalid Boardcast graph`、英文跨窗口保存冲突；读取新版或损坏会议时，还会错误地加上“找不到这场会议”。[基线记录](evidence/implementation/error-localization/baseline-browser/check.json) 和 [新版会议截图](evidence/implementation/error-localization/baseline-browser/zh-error-language-future.png) 保留了实际显示。基线使用上批生产产物，先核对全部源码哈希。

## 修改范围

沿用 `msg().errors`，将五个模块的 32 个报错位置接入 28 条中英文字典：会议保存/删除/迁移/版本保护、自由场景和模型建议校验、绘图导入、布局失败、背景导出。英文原因保持原文，中文保存冲突增加先导出副本的恢复指引。通用事务失败使用“会议存储操作失败”，避免将读取或删除失败说成保存失败。

启动时捕获实际异常改用 `openFailedReason(error)`；明确未找到会议的其他调用仍使用原来的提示。没有改变校验条件、事务顺序、删除标记、版本判断、超时、图片校验或导出流程。八个产品文件及一个既有断言的差异见 [本批补丁](evidence/implementation/error-localization/product-and-existing-test.diff)，完整文案映射见 [28 条对应关系](evidence/implementation/error-localization/error-copy-map.json)。

## 运行结果

| 检查 | 已验证内容 |
| --- | --- |
| 最终生产包，33 项 | [中英文各四种失败提示](evidence/implementation/error-localization/final-browser/check.json)：新版会议、损坏场景、损坏导入和双窗口保存冲突。拒绝打开/导入后原记录保持；重试和新建不会覆盖另一窗口保存的版本，未保存原话保留 |
| 实际恢复 | 两种语言实际下载 HTML，在相反浏览器语言的独立禁网环境中打开；保留导出语言、原始原话和未保存原话，零 HTTP 请求及脚本错误。640×400 下滚动后，恢复按钮完整落在视口内；见 [中文窄窗](evidence/implementation/error-localization/final-browser/zh-conflict-small.png) 和 [离线恢复](evidence/implementation/error-localization/final-browser/zh-offline-recovery.png) |
| 导入回归，14 项 | [现有任务检查](evidence/implementation/error-localization/import-regression/import-task-check.json)：取消/迟到结果、同文件重试、明确追加、保存失败保留预览、PDF 页数限制、图片识别失败和本地配置恢复 |
| 删除竞争 | [六种竞争与附加控制](evidence/implementation/error-localization/browser-tests.txt)：正常/零 revision/旧库，删除前后及图片写入中；拒绝晚到保存、图片不复活，并验证重载标记、迁移、旧版副本、事务中止和重试 |
| Node | [44 项定向](evidence/implementation/error-localization/targeted-node.txt) 和 [268 项完整测试](evidence/implementation/error-localization/full-node.txt) 通过。新增中英文检查覆盖拒绝写入、80 MB 上限、恶意链接/外部图片和模型建议恢复；失败保留输入，补回依赖后可采用 |
| 类型和构建 | 类型检查、播放器和主应用生产构建通过。最后仅调整通用存储失败的一句中文后，重建两个产物，并再次通过 [9 项语言单测](evidence/implementation/error-localization/final-copy-node.txt) 和上述 33 项生产检查；14 项导入与删除回归未因这句文案再次执行 |

浏览器检查使用真实 App、IndexedDB、两个页面及实际下载；模型回复是合成输入。删除竞争运行于开发模块，和生产页面结果分开记录。新增脚本为 [check-error-localization.cjs](../../scripts/ui/check-error-localization.cjs)。

测试脚本的最初问题包括空数据库未创建对象仓库、在 `about:blank` 写入 localStorage，以及响应式重绘时定位器遇到已移除按钮。修正初始化与等待条件后通过，没有为此改动产品逻辑；详见 [脚本说明](evidence/implementation/error-localization/harness-notes.txt) 和 [首次最终检查记录](evidence/implementation/error-localization/final-browser-attempt1/check.json)。

## 身份与剩余范围

最终入口 `index-Di2qEYlu.js`，SHA-256 `a2ac2f88999bb49ff209fc73a386b5dadb983b03802b0755056271f56163cf2e`；播放器 SHA-256 `7737aff97da8e70dfdf032f97fcf2677ed9058a3f58447fb0c3ae332dbd53c7c`，与 `public/player/player.js` 相同。[构建身份与命令](evidence/implementation/error-localization/identity.json) 记录阶段差异，源码快照和清单位于同目录。

本批未强制触发布局超时/不完整结果或无效导出背景，也未声称每条字典都在生产页面逐项触发。原生 Rust 竞争/文件错误、图片完整性异常详情及任意系统/服务错误仍需按实际调用边界处理，见 [剩余示例](evidence/implementation/error-localization/remaining-boundaries.txt)。原生菜单/托盘、Mac/VoiceOver、真实系统语言切换、其他格式导出中切换语言及 G4 用户试用仍未验收，不关闭 C61 或整体目标。

未提交、推送或发布。独立测试浏览器及本批两个预览已关闭，用户 1423/1430 服务保留；见 [清理记录](evidence/implementation/error-localization/cleanup.json)。
