# Boardcast 桌面 UI/UX 调研、审查与重构方案

2026-11-06 [C47 SVG 标题](svg-title-validation.md)：三种 SVG 输出加入安全的会议标题，特殊字符、中日文、emoji、空标题和自由编辑可读文字通过 36 项实际下载/无障碍树检查；七格式 92 项、动画 30 项、外观 82 项、Node 341 项与完整构建通过，1430 已更新。`503b677` 的四平台构建及三组 ASR 已通过；原 Adreno 机器、实际读屏、Mac、安装升级和完整 G3/G4 仍待验收。

2026-11-06 [C48 动态 SVG](animated-export-validation.md)：修复终态叠放、高亮轮次与静态宿主笔迹丢失。实际下载 30 项、七格式 92 项、外观 82 项、完整 341 项 Node 和构建通过；另修正 CI 的固定数组警告并通过严格 Clippy。跨宿主、Windows ARM 原机、Mac 与完整 G3/G4 保留。

## PR 审查入口（2026-11-06）

当前交付可进入代码审查，完整 C01–C68 / G1–G4 验收仍在进行。先读 [本次提交与验证范围](pr-readiness.md)，再按下面的方案和专题报告深入。

本目录提交方案、验证报告、可复跑脚本的索引，以及两张生产界面概览。约 844 MB 的原始 `evidence/` 截图、日志、导出样本和哈希清单留在本地，没有随 PR 上传；专题报告里的 `evidence/` 链接因此只在原工作区有效。报告记录各批次当时的结果，不代表每一项都在本次 PR 提交前重跑。用户的原始 Windows 诊断 ZIP 不属于提交内容。

[桌面概览](review/desktop.png) · [紧凑窗口](review/compact.png) · [回归脚本及依赖](../../scripts/ui/README.md)

2026-11-05 [紧凑顶栏与保存反馈](compact-header-validation.md)：小窗次要操作收进「更多」，保存状态固定可见，导入/导出及弹窗返回焦点补齐。24 组状态、24 组真实缩放、16 组触屏、14 组流程及本地通道检查通过，1430 预览已更新；Windows ARM/Mac 实机与完整 G3/G4 继续保留。

2026-11-05 [内容列表翻页与输入法改名](content-keyboard-validation.md)：修复按页重建后的选择器失焦、组合 Enter/Escape 误操作及对象说明字号；保留跨页草稿隔离。47 项生产界面、17 项真实缩放和既有内容/回放回归通过；真实 Linux 连续键盘翻页、Rime 中文改名、自由对象命名及冷启动完成 19 项核对。14 个测试进程退出，预览已更新；Mac、其他输入法与 G3/G4 保留。

2026-11-05 [原话生成状态与矮窗口阅读](transcript-state-validation.md)：原话区分排队、生成中、更新、无需修改和已处理，耗时收进详情，保留真实来源；修复旧板重试的过期兜底标记与 720×480 原生原话视口被挤掉的问题。322 项 Node/Eval、34 项新增生产检查、17 项真实缩放及相关回归通过；Linux 窗口、键盘详情和冷启动完成 13 项核对。Windows ARM/Mac 真机与 G3/G4 继续保留。

2026-11-05 [会议库读取失败与旧副本恢复](library-read-errors-validation.md)：损坏文件显示数量、详情与重新读取入口，健康会议继续搜索；完好的旧副本另存新编号后打开，写入失败保留原记录。321 项前端、13 项原生、14 项新增及 20 项既有会议库检查通过；真实 Linux 权限故障/重试、文件恢复重读和冷启动验证 7 个原文件及新副本保留。Windows ARM 实机、Mac 与 G3/G4 继续保留。

2026-11-05 [Windows ARM 本地请求与预热](windows-arm-local-model-validation.md)：依据用户诊断修复原生就绪但网页请求失败的通道缺口；统一原生本地推理，加入 180 秒预热超时、取消与模型管理重试。320 项前端、80 项原生（12 项忽略）、10 项新增生产界面及相关回归通过；真实 Linux Qwen 4B 在受限 CSP 下完成预热、生成和冷启动恢复。Windows ARM 实机、安装包与原 UI/G3/G4 仍待验收。

日期：2026-11-03。审查与实施基线：`4f9ab26a46f07cc5575460fd7431f1a995e705cc`。最初交付调研、问题与方案，随后已开始修改产品代码；当前进度、运行证据与未完成验收见 [实施记录](implementation-progress.md)。

2026-11-05 [材料导入的键盘返回](native-import-focus-validation.md)：三行产品改动修复取消错误/下载/预览后失焦，生产原生协议、21 项导入检查及真实连接取消、文件选择器、Rime、冷启动通过。18 项证据核对完成，22 个测试进程结束，预览更新。另定位到元素输入前的 AT-SPI GrabFocus 断开，Cua/WebKit 该路径仍失败；Mac/读屏/音频及 G3/G4 继续。

2026-11-05 [Linux 中文输入与标题焦点](native-ime-validation.md)：真实 Rime 验证原话、说话人与标题输入，修复取消/空标题/保存失败后的失焦，以及该环境可复现的标题取消 WebKit 崩溃。20 项生产会议库检查、真实 EACCES 键盘重试、冷启动、构建和 28 项证据核对通过；66 个测试进程结束，预览保留。其他底层输入触发、日文、Mac/读屏/音频及 G3/G4 继续。

2026-11-05 [C66/C68 Linux 界面异常恢复](native-webprocess-validation.md)：WebKit 进程停止后提供中英文原生恢复/退出提示，取消遗留请求并停止收音。真实恢复后继续保存、下载中断、退出与冷启动完成 20 项证据核对，16 项 Rust 测试及构建通过。标题取消触发由上述输入法批次接续；其他底层触发、物理音频、Mac/G3/G4 保留。

2026-11-05 [C42 原生取消与新会议恢复](native-arxiv-cancel-validation.md)：arXiv 导入复用既有请求取消机制；真实连接等待/正文传输均在点击取消后 1 秒内释放，重新下载、新会议提交和冷启动保留原记录。19 项证据核对、5 项 Node、10 项 Rust、生产导入/摘要/原生协议及构建通过。早期一次 WebView 退出原因未明，Mac 与 G3/G4 保留。

2026-11-05 [C65 下载与安装前保存](native-update-download-validation.md)：下载与安装分为两步，下载时可返回会议，校验后显式保存并重启；30 秒读取超时释放停滞请求，持续传输可超过 30 秒。真实 Linux 后台输入、签名拒绝、35.7 秒慢下载、EACCES 保存屏障、安装格式失败重试和正式配置冷启动完成 31 项证据核对；5 项 Node、4 项 Rust、维护/Dialog/生产协议及构建通过。真实安装包升级、音频和 G3/G4 保留。

2026-11-05 [C64 Linux 更新检查](native-update-check-validation.md)：修复网络停滞导致无限等待，30 秒超时释放真实请求并提供中英文重试指引。真实 GitHub 重试、原生菜单失败恢复、720×480 窄窗和关闭自动检查后的冷启动完成 25 项证据核对；会议文件保持，12 项 Node、4 项 Rust、维护/Dialog/生产外壳回归及构建通过。更新包下载、验签、安装与 G3/G4 保留。

2026-11-05 [PDF 失败恢复与取消](pdf-import-errors-validation.md)修复异常页面继续读取无效像素、显示底层异常的问题，提供具体页码和中英文恢复指引。60 项生产检查、21 项导入和 52 项 PDF 标注回归、15 项 Node 检查通过；覆盖密码文件、第二页失败、64 MiB 文件、实际解析取消及切会后的迟到读取，25 个解析线程全部结束。预览已更新；本批为 G1，原生与 G3/G4 边界见报告。

2026-11-05 [Excalidraw 导入与往返保存](native-drawing-validation.md)修复重复标签、同名分组、并行箭头、重复渲染、重新导入计数及扩展字段/随机笔触丢失。47 项生产绘图、21 项导入回归、工作区检查和 318 项 Node/Eval 通过；真实 Linux 导出、再导入、冷启动完成 29 组证据核对，原 10 个文件与冷启动前 22 个文件保持。应用预览已更新，任意未来绘图字段及 Mac/G3/G4 仍保留。

2026-11-05 [Linux 图片导入续批](native-image-validation.md)修复缺件时的配置入口，验证公共源视觉组件下载、原图片直接重试、真实 PNG/JPEG/WebP/GIF、大图、60 区域上限、识别中取消与六张板冷恢复。39 组证据核对、56 项模型入口、21 项导入和 4 项 Node 检查通过；远程启用期间图片仍由本地模型处理。预览 `http://127.0.0.1:1430` 已更新，Mac/G3/G4 与任意真实图片质量仍保留。

2026-11-05 [Linux 本地板书模型续批](native-llm-validation.md)修复模型错误入口、自动回退的使用状态和保留视觉组件的管理入口。真实本地生成、4B/9B 切换、权限失败恢复、组件删除与冷启动完成 34 组证据核对；51 项生产页面、10 项 Rust 检查通过，三个用户模型原件不变。预览 `http://127.0.0.1:1430` 可直接打开；各构建覆盖、真实图片理解、Mac/G3/G4 等边界见报告。

2026-11-05 [Linux 模型删除恢复](native-model-removal-validation.md)修复部分文件已删却仍显示就绪的问题。真实权限失败、取消后公共源补下载、直接重试与冷启动完成 24 组证据核对；19 项组件、21 项中英文生产检查、保存屏障和 9 项 Rust 下载回归通过。预览 `http://127.0.0.1:1430` 已更新，真实 LLM/视觉切换删除、物理音频和 Mac/G3/G4 仍待验收。

2026-11-05 [Linux 模型下载续批](native-download-validation.md)修复暂停不及时与续传阶段误显示。原生断点冷启动、下载源失败恢复、真实模型文件经隔离 TLS 备用源完成及安装后重启已验；23 组证据核对、9 项 Rust 和 16 项开发组件检查通过。当前预览仍为 `http://127.0.0.1:1430`；原生公共源、真实音频与 Mac/G3/G4 继续保留。

2026-11-05 [Linux 原生取消续批](native-summary-validation.md)修复摘要停止后继续等待的问题。实际 HTTP 取消、并发板书、复盘停止、原句重试、草稿确认与冷启动完成 25 组证据核对；313 项 Node/Eval、10 项 Rust、21 项摘要与 6 组任务隔离通过。当前预览为 `http://127.0.0.1:1430`；真实模型、Mac/G3/G4 仍待验收。

2026-11-05 [HTTPS 与认证代理续批](native-tls-validation.md)修复证书/代理故障提示过于笼统的问题。真实 Linux 中英文故障恢复、localhost 绕过、原句重试及冷启动完成 57 组证据核对；312 项 Node/Eval、8 项 Rust、30/31 项生产连接/隐私检查通过。真实模型与 Mac/G3/G4 仍待验收。

2026-11-05 最新续批：[归档打开保存状态](archive-open-save-validation.md)一行修复旧 render 将新会议误标为未保存的竞态；新增回归先失败后通过，真实 Linux 打开、编辑保存及冷启动已验，58 项启动、任务隔离、311 项 Node/Eval 与构建通过。[Linux 系统主题](native-theme-validation.md)已验证两种白板自动跟随、手动覆盖及三种偏好冷启动；[启动恢复期间保存](startup-validation.md)、[arXiv 原生下载与恢复](native-arxiv-validation.md)、[浏览页面不延长回放](replay-view-validation.md)、[Linux PDF 标注与换页](native-pdf-validation.md)、[多列竖排、表格与公式](pdf-structure-validation.md)及[文字方向与扫描页](pdf-directions-validation.md)保留各自证据。既有多余历史步骤不自动清理；其余 G2、Mac 与 G3/G4 验收仍保留，完整重构尚未完成。

## 阅读顺序

1. [完整重构方案](refactor-plan.md) 与 [布局线框](wireframes.html)：推荐方向、逐屏交互、状态、实现批次与验收。
2. [独立体验锐评](critique.md) 与 [执行矩阵](runtime-coverage.md)：68 组中 63 组有实际界面或模拟状态操作，2 组仅源码，3 组待 Mac；每组都列出未测变体，不能视作全通过。
3. [Mac 应用研究](research.md)：11 个案例、一手链接、3 个官方截图观察、可迁移原则。
4. [Issue 去重记录](issue-triage.md)：新增 #73–#78、复用问题和未立项项。
5. [源码覆盖清单](coverage-inventory.md)：68 组场景、源码定位，以及真实 Linux Tauri smoke 的环境与结果。

## 核心决定

保留有辨识度的手绘白板，采用安静稳定的会议工作台。首先处理保存、异步会话隔离和 PC 编辑持久化，再组织导航、录音状态、模型设置与导入导出；让用户清楚知道“内容有没有收进来、处理到哪一步、是否保存、如何取回”。

## 已完成的 GitHub 交付

新增 [#73](https://github.com/HsiangNianian/boardcast/issues/73)、[#74](https://github.com/HsiangNianian/boardcast/issues/74)、[#75](https://github.com/HsiangNianian/boardcast/issues/75)、[#76](https://github.com/HsiangNianian/boardcast/issues/76)、[#77](https://github.com/HsiangNianian/boardcast/issues/77)、[#78](https://github.com/HsiangNianian/boardcast/issues/78)。每项含复现/证据、用户影响、源码、去重说明与验收条件；没有把既有 #60、#61、#9、#45、#53、#68–#72 重开。

创建前全量读取 58 条既有 issue，保存 [快照](existing-issues.json)。该文件是创建前的时间点记录，不应当作实时状态。新增 issue 创建后已从 GitHub 回读核验为 open。

## 验证与证据边界

- [206 项现有 Node 测试通过](evidence/baseline-tests.txt)，不代表 UI 或硬件链路通过。
- reviewer 使用独立浏览器数据、真实 React UI 与 production build；桌面专属状态用有记录的模拟 IPC。运行矩阵明确区分真实 UI、模拟、源码及未测状态。
- Linux 原生 smoke 在 Cua `local:sb-5f1135f0` 的隔离显示中运行真实 Tauri/WebKit/GTK，覆盖模型、设置、诊断和文件框取消；使用旧原生壳与当前前端，因此不是当前 HEAD 原生全量验收。
- 本轮没有可用 Mac 真机。Apple 资源安装、系统权限、双路音频、VoiceOver、真实 updater 安装/重启与长会议负载均保留为方案中的硬件验收门槛。
- 第三方应用研究来自官方说明与截图，没有声称逐个亲自在 Mac 上操作。
- `evidence/` 仅包含本轮合成数据、截图、导出前后样本及日志。所有运行与 mock 结果按报告说明解释，不把模拟成功当后端真实成功。

文档与截图保留在当前 worktree，未提交或推送 Git。
