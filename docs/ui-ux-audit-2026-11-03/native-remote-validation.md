# C21/C22 原生配置与 C24 历史来源验证

2026-11-03。实际运行 Linux Tauri/WebKit、Rust IPC、系统文件读写和 Cua 原生输入。没有用网页 IPC fixture 代替这一轮桌面流程。原始证据位于 [native-remote](evidence/implementation/native-remote/)；核心流程有 [23 项证据检查](evidence/implementation/native-remote/native-remote-check.json)，最后的显示修复另有 [4 项原生复核](evidence/implementation/native-remote/native-final-check.json)。

## 运行身份与隔离

私有构建目录为 `/home/hsiangnianian/.cache/boardcast-g2-remote-r3g2f6ci`，应用标识 `games.mmstudio.boardcast.g2.remote`。配置、会议、缓存均使用此目录下的独立 XDG 路径。复用已有 Xvfb/Openbox 工具，以私有 D-Bus、AT-SPI 和 Cua socket 操作独立显示 `:90`；没有操作用户日常会议或物理麦克风。

- 主流程构建嵌入 `index-DbxxH0Ro.js`，二进制 SHA-256 为 `4370ee2368c92b395764112c6ca5726fcc4ed9faaf8742adc30da6204d1c8548`。报告内保存 192 个源码/配置文件的哈希。
- 发现历史来源显示问题后，仅修改 App 中该标签的显示条件，重新构建前端和私有 Tauri 包。最终嵌入 `index-DxrjKKf1.js`，二进制 SHA-256 为 `33f5cc04255d4e4473c77bf73bc02f1ff4b612a287fbb0b32722ee60e894482a`。当前源码哈希与最终快照逐项相符。
- 核心原生流程在第一阶段构建运行；最终构建重跑停用后重开与历史来源显示。两阶段共用真实测试会议，最终文件逐字节不变；没有把第一阶段全部检查说成最终构建重新执行。

`serve-native-remote.py` 启动本机、不转发的 HTTP 代理 fixture，仅接受两个合成域名。代理变量只设置给私有测试进程，不修改系统 DNS/代理。测试使用合成 Key 和会议句子；服务器保存目标、阶段、固定探测/凭据匹配布尔值，不保存正文或 Key。它返回受控模型内容，不代表真实供应商推理能力。

## 实际完成的桌面流程

| 用户操作 / 故障 | 实际观测 |
| --- | --- |
| 填写后测试连接 | 只收到固定探测，页面显示 Connected，`remote.json` 仍不存在，远程未启用 |
| 首次保存，取消域名授权 | 弹窗指明 `boardcast-a.test` 与发送范围；取消不增加请求、不创建配置，URL/模型/密码草稿保留 |
| 确认启用 | Rust 保存配置，Unix 权限实测为 `0600`；密码框清空，只显示保存提示 |
| 输入第一句合成原话 | 请求实际到 A，使用正确的保存凭据；v2 文件写入原话、生成节点、来源及 revision 1，UI 显示 Saved |
| 勾选本会仅本地，再输入一句 | 策略写入 revision 2；无远程请求，原话在 revision 3 保留为 `failed / unavailable`，有重试和配置入口 |
| HTTP 401 回显 Key | 实际原生请求使用已保存 Key，UI 错误为 `Invalid credential: [key]`；原配置哈希未变 |
| 服务端只发部分响应正文 | 实测 **30,001 ms** 后 native client 关闭 socket，UI 显示超时并恢复按钮；等待期间尝试输入没有改掉模型草稿；随后测试成功 |
| 改成 B，取消授权 | 弹窗显示 `boardcast-b.test`；B 未收到请求，A 配置哈希未变，B 草稿保留 |
| 配置文件设为 `0400` 后确认保存 B | 真正文件写入返回 `Permission denied`；原文件哈希未变，草稿保留；恢复 `0600` 后点击 Save 成功 |
| 停用、原生菜单退出、重开 | 停用状态、保存的 Key、两句原话和失败重试入口均恢复；停用启动不发送探测 |
| 重新启用 B | 沿用已保存 Key，已确认域名无需重复询问；原会议的本地策略仍勾选 |
| 解除本地限制、重试失败句 | 真正请求到 B；更新原句 ID，会议仍为两句，revision 5，未新增重复原话 |

全程 14 个请求中只有两次携带合成会议内容，分别对应显式发送到 A 和显式重试到 B，其余均为固定探测。详见 [请求记录](evidence/implementation/native-remote/remote-requests-stage1.json)、[已保留的失败句](evidence/implementation/native-remote/meeting-local-failed.json) 和 [重试后的会议](evidence/implementation/native-remote/meeting-recovered.json)。设置变更触发的状态检查也计入探测，没有从记录中删掉。

## 运行中发现的显示问题

停用远程配置后，历史原话的 `Remote native-fixture` 来源消失，虽然文件里的 `via` 仍在。原因是显示条件依赖当前 `useRemote`。现在按该句实际保存的 `via` 显示，保留原有的完成/失败判断；本地回退来源和标记同样保留。

生产浏览器新增两项断言：先在旧构建 [复现失败](evidence/implementation/native-remote/provenance-before.json)，修复后整个远程隐私检查 **31 项通过**，包括历史远程来源和本地回退标记。真实 Tauri 也按相同会议文件对比 [修复前](evidence/implementation/native-remote/native-provenance-before.png) 与 [修复后](evidence/implementation/native-remote/native-provenance-after.png)：远程停用、重开后仍显示两句的实际模型来源，没有新请求，也没有重写会议文件。

截图里的黑色光标形状来自私有 Xvfb 上的 Cua 光标覆盖层，保留原图。Cua 对 WebKit 的部分复选框/文本输入背景投递没有生效，已根据新截图改用私有显示的前台输入，并用实际配置/会议文件核对结果；没有把投递成功当作产品状态成功。

类型检查、生产构建、独立 Tauri 构建、31 项生产浏览器回归和 `git diff --check` 通过。这次改动只影响原话来源显示，没有重复运行上一批完整 Node/Rust 测试。原生证据复核入口为 `python scripts/ui/check-native-remote.py <上述私有目录>`；该脚本检查已捕获证据，不代替重新操作 UI。

## 剩余边界

2026-11-05 续批：[Linux 原生主动取消](native-summary-validation.md)已补上响应头/部分正文中断、并发隔离、结束复盘、原句重试、摘要候选确认和冷启动，25 组证据核对通过。下段的“原生主动取消”是本报告原批次边界，不再代表当前 Linux 路径未实现；其他在途配置组合及平台边界仍保留。

2026-11-05 续批：[Linux HTTPS 与认证代理](native-tls-validation.md)已补上真实 TLS、证书拒绝、407/502/断开、中英文恢复提示、localhost 绕过和冷启动；下段记录的是本报告原批次的边界，供应商/真实模型及其他平台仍未因此关闭。

没有安装本地模型，所以本批验证了“仅本地且无模型时保留原话”，没有验证真实本地模型成功回退。真实供应商、HTTPS/TLS/代理认证与失败、更多在途配置并发、原生主动取消仍未完成。后续 [C63诊断验收](native-diagnostics-validation.md) 已补上真实Linux诊断ZIP、敏感样本及保存取消/失败/重试；七种会议格式的原生导出/打开、Mac音频/IME/VoiceOver、60分钟现场及G4用户试用仍保留，不能据此关闭完整C01–C68/G1–G4。

私有 app、代理服务、AT-SPI、Cua daemon、窗口管理器和 Xvfb 已退出，见 [清理记录](evidence/implementation/native-remote/cleanup.json)。保留构建快照供复查。没有安装依赖、提交 Git、推送或发布。
