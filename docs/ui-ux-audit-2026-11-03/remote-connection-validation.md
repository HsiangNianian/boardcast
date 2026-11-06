# C21 连接失败恢复与 C22 迟到检查

2026-11-03。本批继续补充 [上一批远程隐私验证](remote-privacy-validation.md)。最终生产构建 `index-DbxxH0Ro.js`，源码哈希、bundle 哈希和内嵌报告见 [构建记录](evidence/implementation/remote-errors-build.json)。这些证据覆盖下述路径，不替代完整 G2/G3/G4。

## 失败证据与修复

**错误正文回显凭据，且忙碌表单仍可编辑。** 真实回环 HTTP 服务返回带合成非标准 Key 的 401。原构建把完整 Key 显示在错误正文里，三个字段仍可编辑；[失败报告](evidence/implementation/remote-errors-before.json) 中 `unauthorizedContainsKey: true`、`busyFieldsLocked: false`，截图为 [修复前](evidence/implementation/remote-errors-before.png)。测试使用专门生成的假凭据，没有读取用户真实 Key。

现在请求层根据实际使用的 Key 脱敏，然后限制错误显示长度；网页继续复用现有 `scrub`。非 JSON 成功响应给固定说明，不回显原始正文。成功探测或模型正文意外包含认证 Key 时也替换该值，短 Key 的保存提示完全遮盖。网络错误不会携带原生请求 URL。状态码仍保留；不会把 401、畸形协议响应都说成网络问题。

测试/保存期间连接字段只读，可查看复制但不可改掉正在验证的草稿；失败和超时恢复编辑。错误使用 `role=alert`，成功反馈使用 `role=status`。640×400 下长错误换行、重试和关闭可达，见 [窄窗截图](evidence/implementation/remote-errors-compact.png)。真实屏幕阅读器仍未验收。

**网页连接探测没有明确超时。** 网页探测现在用 30 秒超时，覆盖等待响应头和读取正文；原生探测也使用 30 秒总期限，普通原生模型请求保留 180 秒上限和原有连接期限。超时保留草稿并恢复按钮；这是连接测试的期限，不是新增的网页所有推理请求总时限。

**旧检查成功会覆盖“已停用”。** 已保存远程配置启动探测时，暂扣 HTTP 回包；先在设置停用，再放回成功响应。修复前顶部又显示远程模型可用，见 [失败报告](evidence/implementation/remote-late-check-before.json) 和 [截图](evidence/implementation/remote-late-check-before.png)。现在异步检查按发起顺序隔离，只接纳最新检查的成功/失败；原生内置模型停用前再次检查归属和当前本地策略。有效远程状态还必须对应 `enabled` 配置。

## 实际运行结果

- [生产浏览器连接检查](evidence/implementation/remote-errors-check.json)：**30 项通过**。真实 HTTP、真实浏览器 CORS 和实际 localStorage，不拦截请求、不替换计时器。覆盖 401 脱敏/长度、草稿保护、测试与保存分离、非 JSON/缺字段/null/对象/空正文、成功回显脱敏、Key 周边空白、错误 URL/URL 内用户密码、完整接口路径、实际拒绝 CORS、超时、失败重试/保存、短 Key，以及迟到成功/失败不覆盖停用和新配置。
- 超时服务器先发响应头和部分 JSON，再停住正文。网页实测 **30,509 ms** 显示超时，服务端确认连接关闭；草稿保留并可继续编辑，见 [超时截图](evidence/implementation/remote-errors-timeout.png)。这不是只验证 `setTimeout` 被调用。
- 同一生产构建重跑上一批隐私 **29 项**、摘要 **21 项**、任务生命周期和原生外壳协议，全部通过。[完整浏览器输出](evidence/implementation/remote-errors-ui.txt)；Dialog 和维护面板 fixture 也通过 [回归](evidence/implementation/remote-errors-dialogs.txt)。原生外壳 fixture 不等于真实原生配置界面运行。
- [Rust 远程请求](evidence/implementation/remote-errors-rust.txt)：**7 通过、1 跳过**。正式 `post()` 连接实际回环 socket；验证 401/原始文本/非 JSON/非法结构脱敏、探测回显脱敏、URL 拒绝、重定向禁止、短 Key 提示及实际等待 30 秒的正文超时。跳过项要求真正的模型服务，未算成通过。原生非测试路径 [cargo check --lib](evidence/implementation/remote-errors-cargo-check.txt) 通过。
- [完整 Node](evidence/implementation/remote-errors-node.txt) **257 项通过**。`tsc --noEmit`、脚本语法、`git diff --check` 和 [生产构建](evidence/implementation/remote-errors-build.txt) 通过；构建仍有既有大 chunk 提示。

复跑入口见 [scripts/ui/README.md](../../scripts/ui/README.md)。`check-remote-errors.cjs` 无需主机映射，正常运行会真实等待约 30 秒；HTTP 服务和 Chromium context 在结束时关闭。报告只保存请求类别、路径和匹配布尔值，不保存请求正文或凭据。

## 未完成的验收边界

C21 已补齐本批网页常见失败路径与正式 Rust 传输边界。续批 [真实原生流程](native-remote-validation.md) 已补上 Linux Tauri 配置/保存/同意、401、30 秒正文超时和文件权限失败恢复；真实服务商/TLS/系统代理失败仍未验。此报告保留本批构建身份，原生证据以续批报告为准。

C22 的 Linux 域名同意/取消、每会仅本地、停用重启和原句重试已由续批补充；真实本地模型成功回退和更多并发配置变体仍待验。已发出的请求不能撤回，原生在途 IPC 暂无主动取消。后续 [C63诊断验收](native-diagnostics-validation.md) 已检查真实Linux ZIP及保存的合成凭据排除，但未验macOS VoiceOver、真实音频和G4用户试用。完整C01–C68/G1–G4保持未完成状态。

未安装依赖、提交 Git、推送或发布。
