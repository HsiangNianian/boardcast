# 远程目标、同意与本场仅本地：C21/C22 验证

2026-11-03。继续实施完整重构方案；本批证据支持下述具体行为，不关闭整个 C21/C22 或 G2/G3/G4。最终网页构建为 `index-CAAA7nIq.js`，源码、bundle 哈希及各报告的内嵌快照见 [构建记录](evidence/implementation/remote-privacy-build.json)。

## 实际发现与修复

1. **摘要切到仅本地后仍继续外发。** 长摘要持有最初的远程配置。修复前，第一个分段请求已经发出，勾选“本场仅本地”后另四个请求仍发到远程主机；[失败抓取](evidence/implementation/remote-privacy-before.json) 保留了五个请求的目标。现在每次派发都检查当前会议策略，后四个请求转到本机。远程停用也会阻止旧摘要继续外发。取消通过 AbortSignal 阻止后续分段及回退请求，网页当前 fetch 可中止；原生已经发出的 IPC 请求不能撤回，结果和后续工作有隔离。
2. **“本地模型地址”可绕过远程同意。** 修复前，在该字段填入 `boardcast-b.test` 后，真实 HTTP 服务器收到了会议内容，见 [失败记录](evidence/implementation/local-destination-before.json) 和 [原始输出](evidence/implementation/local-destination-before.txt)。现在板书、摘要及回退都在发送前验证本机地址；健康探测也不连接非本机地址。原话仍保存为可重试状态，修正地址后重试同一条，不新增重复原话。其他电脑上的 llama.cpp 仍可通过“远程与隐私”的 OpenAI 兼容接口配置并确认，原地址不会被清空或自动改写。
3. **`127` 开头的域名被误判为本机。** 前端和原生都曾使用前缀判断。现在只接受 HTTP(S)，并区分真正的 127.x.x.x 地址与 `127.boardcast.test`、`127.0.0.1.example`。保留已有 localhost、子域 localhost、IPv6 回环和本机未指定地址兼容；地址规范化仍接受 `127.1`。前端单测先得到两个失败，再修复为通过。
4. **自动重定向可以改变实际目标。** 本地板书/摘要/健康探测、图片识别、网页远程请求及原生远程请求均不自动跟随跳转。远程设置明确要求填写接口最终地址。被重定向的远程会议请求可按现有机制回退到本机；本机不可用则保留原话/已有摘要，并显示失败。
5. **界面状态与实际行为一致。** 活跃远程主机和端口在 1280×800、720×480、640×400 持续可见；勾选仅本地后显示会议策略。本机完成的摘要使用成功提示，避免把成功回退标为失败。非本机“本地地址”有说明、`aria-invalid` 和关联描述，见 [设置截图](evidence/implementation/local-address-guidance.png)。浏览器连接失败只陈述可观察失败及可能原因，不断言就是 CORS。

## 已运行的验证

- [真实浏览器 HTTP 报告](evidence/implementation/remote-privacy-check.json)：**29 项通过**。生产 React App、真实 CORS/HTTP、真实 IndexedDB；三个测试主机由独立 Chromium 解析到临时回环服务器，不使用 `page.route` 替代此链路，不读取用户凭据或会议目录。报告只记录目标、请求类别和是否包含合成会议标记，不记录请求正文或 key。
- 固定连接探测不含会议内容；测试成功不启用；取消同意不增加启用请求；目标显示含端口；同域更换模型可留空沿用 key；更换主机重新询问；摘要中切换仅本地、停用远程、取消；HTTP 503 回退；已有摘要保留；外部本地地址拒绝和修正后重试；伪回环域名拒绝；本地及远程 HTTP 307 不把内容送到第二主机；健康检查只到本机。完整逐项输出见 [浏览器日志](evidence/implementation/remote-privacy-ui.txt)。
- 同一最终生产构建通过摘要 **21 项**、导入 **14 项**，以及逐句重试、任务隔离、原生外壳协议回归。设置 Dialog 的键盘/嵌套/忙碌关闭/六分类/窄窗检查也通过，见 [Dialog 日志](evidence/implementation/remote-privacy-dialogs.txt)。原生外壳回归使用受控 IPC，不能作为真实 Tauri 运行证明。
- [Node 完整输出](evidence/implementation/remote-privacy-node.txt)：**257 项通过**，包含取消后不再启动自定义传输、原生式迟到结果不启动下一摘要分段，以及真实 HTTP 图片识别重定向拒绝。会话生命周期测试等待请求确实开始才取消，避免仅测到“还没发送”。
- [Rust 远程测试](evidence/implementation/remote-privacy-rust.txt)：**4 通过、1 跳过**。新测试调用正式 `post()`，回环服务器收到首个合成正文，返回 307 后没有第二个连接；跳过项需要真实模型服务，未将其算作通过。原生非测试路径 [cargo check --lib](evidence/implementation/remote-privacy-cargo-check.txt) 通过。
- `tsc --noEmit`、脚本语法、生产构建和 `git diff --check` 通过。构建仍有既有的大 chunk 提示，见 [构建输出](evidence/implementation/remote-privacy-build.txt)。

复跑命令及主机映射见 [浏览器检查说明](../../scripts/ui/README.md)。所有测试 HTTP 服务会关闭；映射只作用于检查用 Chromium，不修改系统 DNS/代理。先前 20 项检查及 256 项 Node 结果保留在 [前一阶段快照](evidence/implementation/remote-privacy-phase1-build.json)，不将旧构建的 lifecycle 报告误算为最终构建验证；最终构建已重新运行该回归。

## 仍未达到的边界

- 本批之后已在 [连接失败恢复验证](remote-connection-validation.md) 补充 HTTP 401、畸形响应、实际拒绝 CORS、30 秒正文超时、忙碌草稿保护和服务错误回显凭据。随后 [真实原生流程](native-remote-validation.md) 补上 Linux Tauri 配置/同意/超时/文件权限恢复/重开，并修复停用后历史来源显示；最新浏览器检查为31项。这里的构建快照保留原批次29项证据，最新源码与运行结果以续批报告为准。
- C22 的 Linux 配置/同意/每会本地策略/持久化恢复已有续批证据，真实本地模型成功回退、TLS/代理与请求期间更换配置等变体仍待验。客户端限制的是它实际连接的目标与跳转，本机服务自身是否再转发不在该证据范围内。
- 已经发到服务端的请求不能撤回；本场仅本地的切换阻止后续派发，不代表对方已经收到的内容被删除。原生传输的在途 IPC 尚不支持主动取消。
- 没有真实外部服务商、TLS、系统代理、macOS 音频/IME/VoiceOver、60 分钟现场或 G4 用户试用证据；完整 C01–C68/G1–G4 的未完成项仍保留。

本批未安装依赖、提交 Git、推送或发布。
