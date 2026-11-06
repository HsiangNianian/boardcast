# C39/C42/C67：材料导入的键盘返回与原生输入边界

2026-11-05，基线 `4f9ab26` 当前工作树。取消无效 arXiv 输入、下载或材料预览后，旧版焦点会丢失，键盘无法从“导入”入口接着操作。现在任务开始前聚焦该入口，让既有 Dialog 在关闭后恢复到仍存在的按钮。产品只在 `App.tsx` 增加一个按钮 ref、绑定和聚焦调用，文件导入与 arXiv 复用同一处；没有新组件、存储或依赖。

新增生产检查先在旧版失败，再在最终产物通过：无效编号以 Enter 提交，Escape 取消后回到导入按钮，未发送下载请求、未改原会议；下载和 PDF 预览取消也恢复焦点。既有原生协议浏览器套件（含 5 条 arXiv 检查）及 21 项材料导入检查通过，无页面错误。见[失败基线](evidence/implementation/native-import-focus/browser-before.log)、[最终协议记录](evidence/implementation/native-import-focus/browser-final/check-native-shell/native-shell-check.json)和[导入回归](evidence/implementation/native-import-focus/browser-final/check-import-task/import-task-check.json)。

| 实际 Linux 操作 | 结果 |
| --- | --- |
| 普通窗口按键提交无效编号 | 显示具体输入错误；Escape 关闭后再按 Enter，直接重开导入菜单。见[错误状态](evidence/implementation/native-import-focus/fixed-plain-enter-after.json)与[返回菜单](evidence/implementation/native-import-focus/fixed-invalid-return-after.json)。 |
| 真实原生连接等待中取消 | Rust 下载到达私有回环代理的 CONNECT 等待状态。Escape 取消后约 65 毫秒观察到连接释放，原文件保持，再按 Enter 重开菜单。见[连接结果](evidence/implementation/native-import-focus/fixed-download-result.json)。这只是本次观测，代理没有向远端请求论文，不能用作公共下载性能结论。 |
| 系统文件选择器 | 实际选择测试 `.excalidraw` 文件，解析出一张板、一个对象；Escape 取消后再按 Enter 回到导入菜单，原会议未改。见[文件选择器](evidence/implementation/native-import-focus/fixed-file-picker.png)、[预览](evidence/implementation/native-import-focus/fixed-file-preview.png)和[结果](evidence/implementation/native-import-focus/file-preview-result.json)。 |
| 真实 Rime 输入 | 组合期间 Enter 只确认原始输入，没有提交导入；第一次 Escape 取消候选并保留菜单，下一次关闭菜单，再按 Enter 能重开。见[组合输入](evidence/implementation/native-import-focus/fixed-ime-enter-after.png)和[操作结果](evidence/implementation/native-import-focus/fixed-cases-result.json)。 |
| 退出与冷启动 | 基线、最终及最终冷启动三个宿主都正常退出，退出码 0。冷启动显示原测试句、Saved 和未收音状态，会议及锁文件逐字节不变。见[冷启动界面](evidence/implementation/native-import-focus/cold-restored.png)与[文件核对](evidence/implementation/native-import-focus/after-cold-quit-files.json)。 |

**仍然失败的原生路径：**Cua 按输入元素 token 发送 Enter，在基线和最终构建均触发 WebKit `Crashed`。最终构建另一次按元素输入文字，在 `Component.GrabFocus` 阶段报告远端断开，尚未完成文字送达。见[Enter 对照](evidence/implementation/native-import-focus/fixed-invalid-action.json)、[文字聚焦错误](evidence/implementation/native-import-focus/fixed-download-value.json)和[最终日志](evidence/implementation/native-import-focus/fixed-runtime/app.txt)。后者也说明不能将全部失败归为表单提交逻辑；原生恢复弹窗重新载入后，原会议仍可继续操作。最终运行日志中的两次崩溃来自这两条失败探针，普通窗口按键及系统文件选择器检查没有增加崩溃次数。

同版本 Cua 0.34.0 的公开源码在元素定位的前台文字/按键路径中，先调用 AT-SPI 聚焦，再发送 XTest 输入；这与本次错误阶段一致，但不足以确定 WebKit 内部根因。参见[固定版本输入实现](https://github.com/trycua/cua/blob/cua-driver-rs-v0.34.0/libs/cua-driver/rust/crates/platform-linux/src/tools/impl_.rs#L7852)及[源码身份记录](evidence/implementation/native-import-focus/cua-source-observation.json)。本批没有升级、修改 Cua/WebKit，也没有关闭可访问性。

前端类型检查/双构建、原生 locked/offline 构建和差异检查通过；没有修改 Rust 产品代码或重复其无关全量测试。[18 项记录核对](evidence/implementation/native-import-focus/evidence-check.json)包含预期失败对照，只核对已有证据，不能把它解释为 18 个原生场景全部通过。194 个构建输入与最终产物一致，只有 App 改变；1430 预览实际脚本哈希与构建一致。见[身份记录](evidence/implementation/native-import-focus/identity.json)和[归档清单](evidence/implementation/native-import-focus/manifest.json)。

测试使用独立显示、会议、输入法配置及词典。22 个登记进程（含回环代理）已结束，没有私有环境残留；原 1423/1430 预览保留。首次下载探针停在上述文字聚焦失败，后续未执行的连接/选文件步骤不计成功；真正的连接与文件证据使用 `fixed-connect-*` 和 `fixed-file-preview*` 文件。

**未验收：**上述无障碍聚焦崩溃、其他输入法/原生 Wayland、Mac/VoiceOver、物理音频、60 分钟现场及 G4 用户试用。用户目前没有测试 Mac。当前修复只关闭导入后的键盘返回缺陷，完整 C01–C68 目标继续；未提交、推送或发布。
