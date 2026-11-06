# Windows ARM 本地模型请求与预热

2026-11-06 后续进展：PR #79 的 `503b677` 已在 Windows ARM CI 完成安装包构建、随包 llama-server smoke 和 Rust 测试；[该次构建](https://github.com/HsiangNianian/boardcast/actions/runs/42180042046)的 `boardcast-windows-arm64` artifact（ID `13486390167`，保留至 11 月 20 日）包含本报告的本地请求与预热修复。它也包含动态 SVG 续批，但早于 SVG 文档标题修复；未在用户的 Adreno 机器上验证，也未安装或发布。下面保留原排查批次的结果与边界。

2026-11-05，基线 `4f9ab26`。用户提供 Windows ARM64 的 `boardcast-diagnostics.zip`，报告 GPU 有占用但模型一直无法完成预热。本批修复桌面版本地请求通道和预热失败恢复，Windows ARM 真机验收仍待完成。

## 诊断能确认什么

- 包内版本为 `0.21.1-5`，原生架构为 `windows/aarch64`；识别到 Qualcomm Adreno X2-90 和 Qwen3.5-4B-Q4_K_M，服务状态为 `ready`。
- 两条前端异常为 `TypeError: Failed to fetch`；`llama-server.log` 为 0 字节。GPU 被识别、服务健康检查成功，不代表前端推理请求成功。
- 现有代码由 Rust 检查服务启动，网页却直接请求本地健康接口、预热和推理。原生与网页的网络路径不同，确实存在服务已就绪而页面无法请求的缺口。
- 仅凭这份包无法区分 WebView 网络策略、代理、首次推理故障或驱动问题。包中的音频缓冲异常也不能作为 GPU 故障证据。保存了[不含用户路径的诊断摘要](evidence/implementation/windows-local-transport/diagnostics-summary.json)，未把用户原始 ZIP 加入项目。

[微软的 WebView2 公告](https://github.com/MicrosoftEdge/WebView2Announcements/issues/126)说明启用本地网络访问检查时可能阻止回环请求，同时说明默认行为和推出时间仍有约束。因此本批没有把 LNA 当作这台机器上已证实的根因，也没有关闭浏览器安全检查。

## 改动

- 本地预热、板书、摘要、远程失败后的本地兜底和图片识别统一调用原生 `llm_chat`。复用现有 HTTP 客户端和取消机制，回环地址不走系统代理、不跟随重定向；只接受当前应用拥有的模型服务地址，不发送远程 API key。
- 桌面在线状态直接采用原生状态。网页版本继续使用浏览器请求。
- 预热最多等待 180 秒。失败会显示中英文说明，入口打开模型管理并提供「重新启动」；失败不会再被吞掉而显示已就绪。切换服务、离开页面时取消旧预热，迟到结果不能覆盖新状态。
- 复用既有 GPU 参数、引擎和模型，没有增加依赖。产品涉及 9 个文件；[本批补丁](evidence/implementation/windows-local-transport/product-and-tests.patch)和[文件哈希](evidence/implementation/windows-local-transport/changed-files.json)只记录本批差异，不混入之前的 UI 工作。

## 验证

| 范围 | 结果 |
| --- | --- |
| 前端及 Eval | 320 项通过，见 [Node 日志](evidence/implementation/windows-local-transport/node-tests.log)。包括 WebView fetch 全部失败时四类调用仍经原生、预先取消、非本机地址拒绝、原生错误和视觉组件错误说明。 |
| 原生单元与 HTTP | 80 项通过、12 项显式忽略，见 [Rust 日志](evidence/implementation/windows-local-transport/rust-tests.log)。新本地通道测试在 HTTP/HTTPS/ALL_PROXY 均指向不可用端口时通过；验证当前地址限制、请求正文保留、无 Origin/API key，以及错误响应。取消、正文超时和拒绝重定向复用已有 HTTP 测试。 |
| 生产界面新增回归 | [10 项通过](evidence/implementation/windows-local-transport/browser/check-local-transport/report.json)。旧生产构建未发出原生模型请求，测试先失败；新构建在阻断网页请求时生成并保存。失败、180 秒超时取消、管理模型重试、服务切换均通过。超时测试加速计时，并断言产品传入 180000 ms；显式移除 `AbortSignal.any` 后仍通过。 |
| 原有界面回归 | 完整原生协议、40 项输入、64 项导出交接、缺失素材退出、60 项存储错误语言、27 项录音切换和56项模型状态语言检查通过。相应模拟原生接口已迁移到新命令。完整视觉矩阵未重跑。 |
| 构建与格式 | TypeScript、生产构建、Linux 原生离线锁定构建、Rust 格式和 `git diff --check` 通过。最终 11575 个前端文件与原生二进制内嵌前端逐字节相同，见[核对](evidence/implementation/windows-local-transport/frontend-equivalence.json)。 |

存储语言检查暴露了测试时序问题：解除故障后，自动保存可能先于 Playwright 的稳定点击完成。测试改为先聚焦重试按钮，再解除故障并按 Enter，仍要求相同编辑完整保存。另外一次失败来自测试期间重建 `dist` 导致动态导入短暂缺失；最终回归在完整且稳定的产物上通过。[失败截图、日志及说明](evidence/implementation/windows-local-transport/test-investigation/notes.json)均保留。

## 真实 Linux 模型

使用独立 XDG、D-Bus、Xvfb 和应用标识，真实 Tauri IPC、`llama.cpp b11954`、Qwen3.5-4B-Q4_K_M、NVIDIA RTX 3060 Laptop GPU。模型来自已有测试文件的独立副本，输入为合成内容。

测试构建额外从 CSP 移除了网页直接访问 localhost/127.0.0.1 的许可；原生通道仍完成：

1. 首次实际预热，模型日志记录一次完整推理；没有用健康接口响应代替预热成功。
2. 在原生窗口键入三步流程，生成主题和三个步骤，会议文件保留原话、模型操作、内置来源及白板。见[实际窗口](evidence/implementation/windows-local-transport/native/generated.png)和[模型设备](evidence/implementation/windows-local-transport/native/model-device.json)。该句约 3.0 秒，仅作运行证据。
3. 正常退出后冷启动，原会议和白板恢复，第二次实际预热完成，会议文件字节不变。见[冷启动窗口](evidence/implementation/windows-local-transport/native/cold-warm-check.png)和[文件核对](evidence/implementation/windows-local-transport/cold-proof.json)。
4. 去掉 `AbortSignal.any` 依赖后，重新构建并启动最终二进制，恢复原记录、完成真实预热；新会议再次生成 4 个节点，原会议字节不变。见[最终窗口](evidence/implementation/windows-local-transport/native/final-generated.png)、[最终推理记录](evidence/implementation/windows-local-transport/final-generation-proof.json)和[最终模型日志](evidence/implementation/windows-local-transport/native/final-run/llama-server.log)。之前两次运行的构建身份分别保存在各自目录内。

三次运行均正常退出，注册的测试进程已结束；原 1423/1430 预览进程保留。[构建身份](evidence/implementation/windows-local-transport/identity.json)、[受限 CSP](evidence/implementation/windows-local-transport/native-build-config.json)、[首次模型日志](evidence/implementation/windows-local-transport/native/first-run/llama-server.log)、[冷启动模型日志](evidence/implementation/windows-local-transport/native/cold-run/llama-server.log)均可核对。

## 剩余边界

没有可访问的 Windows ARM 真机，也没有生成或发布新的 Windows 安装包。当前已安装的 `0.21.1-5` 不包含这批修改；需要新构建在原 Adreno 机器复测，才能确认原问题是否还包含 GPU/驱动侧故障。本批没有声称修复音频缓冲异常。用户暂无测试 Mac；原 UI 全量验收和 G3/G4 继续保留。
