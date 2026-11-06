# Boardcast UI/UX 功能与状态覆盖清单

审查基线：`4f9ab26a46f07cc5575460fd7431f1a995e705cc`，产品版本 `0.21.0`，2026-11-03。本文是**源码盘点与运行审查脚本**，不是实测通过报告；没有把源码存在、单测存在当作真实桌面行为已验证。实际执行结果、截图和环境限制应在体验报告逐项补充。

## 运行入口与隔离要求

- 当前工作树使用 `pnpm@11.13.1`、Node ≥24。网页开发 `pnpm dev`（生成 glyphs、构建独立 player 后起 Vite）；桌面开发 `pnpm tauri dev`（会先检查/下载 llama 与 voice sidecar）。Tauri 配置开发 URL 为 `http://localhost:1420`。
- 主 checkout 发现已有 `node_modules/.bin/vite` 和 `src-tauri/target/debug/boardcast`，路径前缀 `/home/hsiangnianian/GITPROJECT/boardcast/`。**仅证实存在，未确认二进制对应本次 commit，不能把旧 binary 的结果归于本工作树。**
- 目标窗口默认 1280×800，最小 720×480；额外检查 900×600、1440×900、200% 放大、窄窗口 + 日志面板。
- `pnpm typecheck`、`pnpm test` 覆盖类型和 engine/ASR 单测；`cargo test --manifest-path src-tauri/Cargo.toml` 覆盖 Rust。`pnpm build` 构建主 UI 与独立 player。现有测试以纯逻辑为主，未发现统一浏览器交互回归套件。
- 桌面数据默认在 OS 应用数据目录 `games.mmstudio.boardcast/`，包含真实会议。运行审查须用独立测试数据目录/用户，所有删除、故障注入、更新失败测试均限测试数据。浏览器可以用新的 profile 与 IndexedDB。
- 研究范围沿用 `docs/development-priorities.md`：产品流程优先，保留现有 Eval 与 SenseVoice + dynamic + VAD 默认；Apple SpeechAnalyzer 为 macOS 26+ 实验项。

## 执行规则

每行是一组必须取证的场景，而不是一个“点过就算”的入口。运行记录至少标 `PASS / FAIL / BLOCKED / SOURCE-ONLY / SIMULATED`，并记录环境、前置条件、操作、可见结果、截图/日志位置。原生权限、真实音频、磁盘写入、安装重启和 Apple API 不能用浏览器 mock 的通过替代。

每个弹窗/菜单还需统一检查：Tab 与 Shift+Tab 顺序、初始焦点、Escape、关闭后焦点回原入口、后台是否可操作、焦点可见、读屏名称、120%/200% 缩放、720×480 可达性。每个异步操作统一检查：加载、成功、失败、取消或无法取消的清楚解释、重试、切页/切会期间完成、连续点击、应用重开后的状态。

## 覆盖矩阵

| ID | 用户能力与入口 | 要执行的状态/边界 | 主要源码 |
|---|---|---|---|
| C01 | 首次启动与品牌动画 | 首开/再开；按键/点击跳过；字体慢；3 秒上限；减少动画；动画期间键盘是否误触后台 | `src/ui/Splash.tsx` |
| C02 | 无模型开始使用 | 自动模型面板；关掉后下一步；开始录音与打字；没有网络；仅缺 ASR/仅缺 LLM；已有远程模型 | `src/App.tsx:139`、`src/ModelPanel.tsx` |
| C03 | 示例会议 | 中/英文示例；无模型离线录制示例；有模型现场示例；示例缺失/加载失败；重复点击；示例不污染历史 | `src/App.tsx:1726`、`src/demo.ts` |
| C04 | 顶栏状态与导航 | 本地 idle/starting/ready/error、warmup；远程 checking/失败回本地；会议名长；多状态同时出现；窄窗按钮可达 | `src/App.tsx:1764`、`src/style.css:213` |
| C05 | PAD/PC 模式切换 | 空板/有板/录音/回放/资料页；往返切换；说明名称是否传达能力差异；视口与编辑保留 | `src/App.tsx:1840`、`src/board/PencilBoard.tsx` |
| C06 | 文本输入 | 空白、Enter 发送、Shift+Enter 换行、中日文 IME、长文本、粘贴多行；无模型/模型失败仍保留记录 | `src/ui/Composer.tsx`、`src/App.tsx:722` |
| C07 | 输入框折叠与焦点 | 折叠/展开；`/` 聚焦；弹窗期间按 `/`；重开保持；发送后输入丢失/失败重试路径 | `src/ui/Composer.tsx:82` |
| C08 | 手动说话人 | 点击轮换；`@名字 内容`；仅 `@名字`；中文全角 @；12 字上限；名字含空格；复制 A 换行文本 | `src/ui/Composer.tsx:64` |
| C09 | 录音开始/停止 | 未装模型；starting 取消；stopping 防重复；无设备；静音；拒绝权限；首字/最终结果；失败恢复 | `src/App.tsx:873`、`src/ui/Composer.tsx:134` |
| C10 | 音源选择 | 麦克风/系统/两者；设备消失；系统默认改变；录音中换设备/音源；热切后确认实际录到哪路 | `src/SettingsPanel.tsx:421`、`src/App.tsx:986` |
| C11 | 系统音频权限 | Mac <13、Mac ≥13；拒绝/授权后重启；耳机/扬声器；系统音量零；多应用；停止后 capture 释放 | `src-tauri/src/apple_speech.rs`、`src-tauri/src/voice.rs` |
| C12 | 临时字幕/录音反馈 | 音量低/高/削波；无声；临时→定稿不重复；字幕遮挡；背景窗口/最小化继续录音；可见持续状态 | `src/ui/Composer.tsx`、`src/App.tsx:894` |
| C13 | 自动说话人 | 开/关 CAM++；短句、重叠、说话人变更；Apple 路径用当前说话人；重命名/合并后的板书关联 | `src/App.tsx:1212`、`src/engine/voices.ts` |
| C14 | 全局收音入口 | Ctrl+Shift+Space；改快捷键/冲突/清空；托盘开始/停止/显示/退出；CLI `--toggle-mic`；Wayland 提示 | `src-tauri/src/shell.rs`、`src/App.tsx:1328` |
| C15 | 按住空格说话 | PAD 可用/PC 不抢键；输入框与按钮焦点；松开收尾；失焦/切模式时释放；starting 中释放 | `src/App.tsx:1278` |
| C16 | 本地 LLM 下载 | 推荐档、空间不足、未知余量；进度/速度/源/ETA；暂停续传；断网换源；校验失败；重启保留 partial | `src/ModelPanel.tsx`、`src-tauri/src/models.rs` |
| C17 | 模型切换/删除 | 下载并发限制；ready/idle 切换；删除正在使用的 LLM/语音/视觉；后续调用错误；删除是否明确后果 | `src/ModelPanel.tsx:131`、`src-tauri/src/llm.rs` |
| C18 | 语音模型与硬件检测 | 检测 loading/error/retry；SenseVoice 已装/缺失；非 Mac、旧 OS、硬件不支持；面板关闭后事件清理 | `src/ui/SpeechModels.tsx` |
| C19 | Apple 转录 | Auto/手动语种；资源 installed/downloading/missing/unknown/unsupported；下载失败/续传；录音中禁切；实际 locale 提示 | `src/ui/SpeechModels.tsx`、`src/engine/speech.ts` |
| C20 | Apple 原生长链路 | 真实兼容 Mac 上下载→收音→初始自动语言检测→定稿→正常 stop；混语/极短/静音；后台退出；5 秒 finalize 超时 | `docs/development-priorities.md`、`src-tauri/src/voice_auto.rs` |
| C21 | 远程接口配置 | 空 URL/model、错误地址、错误 Key、超时、CORS（Web）；test 与 save 的差别；失败保存草稿；启用/停用/重开 | `src/SettingsPanel.tsx:82`、`src/remote.ts` |
| C22 | 云隐私与 local-only | 新域名 consent 的确认/取消；loopback；修改地址再确认；每场会 localOnly；远程故障 fallback；Key 留空沿用 | `src/privacy.ts`、`src/App.tsx:185` |
| C23 | 设置信息架构 | 语言、本地、远程、快捷键、诊断、样本、术语、音源、更新、笔迹都可找到；滚动定位/关闭返回；失焦修改即时生效 | `src/SettingsPanel.tsx` |
| C24 | AI 板书生命周期 | 排队/运行/草稿→确认墨迹/无操作/失败；重复发言；纠正已画内容；长队列；失败语句可否恢复 | `src/engine/session.ts`、`src/App.tsx:429` |
| C25 | 多类板书内容 | 流程、分组、话题、要点、数学、待办、否决、箭头；中日英；长标题/密集图；超画布与视图跟随 | `src/ink/InkBoard.tsx`、`src/board/layout.ts` |
| C26 | 选择与编辑 | 节点/边改名、拖动、删除、空名、Esc；固定资料块限制；批注工具条不越界；键盘可达性 | `src/App.tsx:1112`、`src/App.tsx:2082` |
| C27 | 标注 | 圈/下划线/高亮/星标/删除线；词/句/段；开关同一标记；PDF 复杂布局命中；颜色不作为唯一语义 | `src/App.tsx:1191`、`src/engine/spans.ts` |
| C28 | 圈着说 | 空圈/多圈/重叠合并/最多 8 目标；撤销一圈/取消全部；下一句消费；排队时各句保留目标；PC 不可用 | `src/App.tsx:823`、`src/engine/pointing.ts` |
| C29 | 撤销/重做 | 手改、圈选、拖动、美化；按钮与 Cmd/Ctrl-Z、Shift-Z/Y；输入框原生 undo 不抢；跨页/跨会边界 | `src/App.tsx:1142`、`src/engine/edit.ts` |
| C30 | 自动美化 | loading/失败/重试；空板不可点；模型忙/回放时行为；布局变化不改内容；一次 undo 恢复 | `src/App.tsx` 的 `tidyUp`、`src/board/beautify.ts` |
| C31 | PC 自由编辑 | 新形状/改字/删除/拖动；接下一句模型输出；切回 PAD 再回 PC；保存重开；PC PNG/SVG/Excalidraw 导出一致 | `src/board/PencilBoard.tsx`、`src/App.tsx:640` |
| C32 | 分页 | 空白新页；缩略图；前后快捷键；语音翻页；密集板新页建议/取消；多页横滚；忙/回放禁切解释 | `src/ui/PageStrip.tsx`、`src/App.tsx:1456` |
| C33 | 会话自动保存 | 500ms 后保存；保存中退出/新会/换会；磁盘满/权限错误；重开恢复；草稿/空会/示例不误存 | `src/App.tsx:481`、`src/meetings.ts` |
| C34 | 会议历史 | 入口可发现；空列表/loading/error/retry；自动标题；长/重名标题；最近更新排序；新会保留原会 | `src/ui/MeetingsPanel.tsx` |
| C35 | 搜索与定位 | 标题/摘要/转写/板书命中；无结果；快速输入旧结果覆盖；中文；点击跳对应页与语句；大量会议 | `src/ui/MeetingsPanel.tsx:56`、`src/engine/search.ts` |
| C36 | 会议改名 | 点击/Enter/blur/Esc；空标题/超长；持久化失败；当前会同步；中文 IME 不提前提交 | `src/ui/MeetingsPanel.tsx:87` |
| C37 | 删除会议 | 确认/取消；当前会/非当前会；失败可见；删除后列表；没有误重新保存已删除会；素材一起清理 | `src/ui/MeetingsPanel.tsx:111`、`src-tauri/src/meetings.rs` |
| C38 | 接着旧板开会 | 一页直接/多页选择取消；复制布局、资料、术语；新会标题；旧内容淡墨、新改正常；原会议不受影响 | `src/App.tsx:1670` |
| C39 | 多类型导入入口 | 文件选取取消、拖放、同文件重选、多文件只处理第一项、扩展名/MIME 不一致、未知文件、忙时禁止解释 | `src/App.tsx:1560`、`src/App.tsx:2030` |
| C40 | PDF 文本页 | 多页/双栏/公式/图表；40 有效页上限；标题/引用页处理；当前会已有内容时新会/追加选择；进度与完成 | `src/board/importPdf.ts`、`src/App.tsx:1569` |
| C41 | PDF 失败与扫描 | 损坏/加密/纯扫描/无文字层/超长；底图可见；失败保留原会；导入中切会/新会；取消路径 | `src/board/importPdf.ts`、`src/App.tsx:1632` |
| C42 | arXiv | 合法 ID/版本号/abs/pdf URL、无效值、下载失败；桌面下载、Web 提示；重复点击与取消 | `src/board/arxiv.ts`、`src-tauri/src/arxiv.rs` |
| C43 | 图片/视觉组件 | PNG/JPEG/WebP/GIF、超大图、无文字、多块/60 块限制；缺 vision；识别失败；本地处理与远程禁传 | `src/board/importImage.ts`、`src/App.tsx:1636` |
| C44 | Excalidraw 导入 | 图形/文字/箭头/分组、未知字段、损坏 JSON；原坐标不保留说明；已有板新页；与原会数据隔离 | `src/board/importExcalidraw.ts` |
| C45 | 导出菜单与保存 | 空板/纯转写/扫描底图/当前页空但其它页有内容；重复点击；大文件进度；原生保存取消/路径无权限 | `src/App.tsx:1020`、`src/App.tsx:1960`、`src-tauri/src/export.rs` |
| C46 | Markdown 导出 | 当前/多页、摘要、完整记录、Mermaid、特殊字符、中日英；导出文件内容与当前会一致 | `src/exporter.ts:177`、`src/board/markdown.ts` |
| C47 | PNG/SVG 导出 | PAD/PC、深浅色、长图、资料底图、标记、手动编辑；实际打开文件；尺寸与截断 | `src/exporter.ts` |
| C48 | 动态 SVG | 初始/最终状态、约 12 秒动画、reduce motion、资料/笔迹、外部文档打开兼容 | `src/exporter.ts`、`src/ink/animate.ts` |
| C49 | 整场 PDF | 每板一页、摘要与记录、长文本页数、扫描图、方向、实际打开；导出过程可见/可恢复 | `src/board/meetingPdf.ts` |
| C50 | Excalidraw 导出 | PAD 图结构 vs PC 当前编辑；再导入另一实例；文字/箭头/分组保留 | `src/exporter.ts:178` |
| C51 | 离线回放 HTML | 禁网 file:// 打开；字体/素材内置；多页、无 steps、暂停/拖动/1×2×4×、窄窗、长会内存 | `src/player/Player.tsx`、`src/player/data.ts` |
| C52 | 应用内回放 | 录音/模型忙禁入；首/末步、拖动/键盘、速度、退出恢复最终板；不可修改；历史不会被回放覆盖 | `src/App.tsx:1383`、`src/ui/ReplayBar.tsx` |
| C53 | 回放分支 | 第 0 步禁用；中间步另起一板；原页保留；新页记录、来源和后续输入一致 | `src/App.tsx:1505` |
| C54 | 转写侧栏 | 空/多页分隔/长会滚动、自动追新与手动阅读；折叠恢复；pending/fail/fallback/原文；说话人溯源 | `src/App.tsx:2259`、`src/style.css:925` |
| C55 | 说话人改名与溯源 | 点击/键盘 Enter、同名合并、Esc；板上 hover/选择跳回发言；新名字在历史与摘要的关系 | `src/App.tsx:1212`、`src/App.tsx:2338` |
| C56 | 摘要生成 | 空记录不可点；长会分段进度；取消；再生成；远程失败 fallback；生成中切会不能污染新会 | `src/App.tsx:993`、`src/engine/summary.ts` |
| C57 | 摘要编辑/复制/清除 | Markdown 渲染；编辑即时保存；纯文本/Markdown 复制失败提示；清除误操作恢复；回放末尾才显示 | `src/ui/Summary.tsx` |
| C58 | 术语表 | 全局/本会、空/重复/错误格式；节点加术语；识别纠正保留原文；接着开会携带；换会不串 | `src/SettingsPanel.tsx:392`、`src/App.tsx:1443` |
| C59 | 纠错样本导出 | 零条/加载失败/有条；cases/JSONL；取消/保存失败；内容与当前修改一致；入口是否挤占常用设置 | `src/SettingsPanel.tsx:161`、`src/App.tsx:1057` |
| C60 | 外观 | 系统/浅/深循环；系统主题切换；潦草 0/60/100、中文线宽/英文线宽边界；重开保持；截图/导出一致 | `src/App.tsx:125`、`src/SettingsPanel.tsx:479` |
| C61 | 本地化 | 系统/中文/英文；应用内所有菜单、错误、托盘、PC 组件、导出播放器；长英文不截断；语种与会议语种分离 | `src/i18n/index.ts`、`src/board/PencilBoard.tsx`、`src-tauri/src/shell.rs` |
| C62 | 快捷键帮助 | `?`、设置入口、Cmd/Ctrl 差异；只 PAD 的动作说明；模态框不误触；PC 与 Excalidraw 键冲突 | `src/engine/shortcuts.ts`、`src/App.tsx:2507` |
| C63 | 诊断包 | 收集 loading/错误；预览每文件；导出/取消/失败重试；无会议内容/音频/Key；路径与敏感 URL 脱敏 | `src/ui/DiagnosticsPanel.tsx`、`src/engine/diagnostics.ts` |
| C64 | 更新检查 | 自动开关/手动；最新/新版本/离线失败；Release Markdown、无 notes 链接；会议进行中提示 | `src/SettingsPanel.tsx:141`、`src/ui/UpdatePanel.tsx` |
| C65 | 更新下载/安装 | 已知/未知 total、签名失败、下载失败后重试、安装失败模型恢复、取消不可用说明；保存 flush→停音→重启 | `src-tauri/src/updater.rs`、`src/ui/UpdatePanel.tsx` |
| C66 | 原生窗口/进程 | 最小化/关闭/退出/Dock 再开；全屏/分屏；录音和下载后台状态；子进程退出；native menu 及 Cmd+, 习惯 | `src-tauri/src/lib.rs`、`src-tauri/src/shell.rs`、`src-tauri/tauri.conf.json` |
| C67 | 可访问性与输入 | 全键盘所有入口、读屏、contrast、目标尺寸、状态 aria-live、radio 箭头、menu 键；触控/笔与鼠标；reduce motion | `src/style.css`、全部 UI 组件 |
| C68 | 恢复与极端数据 | 崩溃前待处理句、历史旧 schema、丢失底图、损坏会文件、大量会议与长会；明确数据不完整、可导出救援 | `src/meetings.ts:134`、`src/meetings.ts:193`、`src-tauri/src/meetings.rs` |

## 源码风险候选：须结合已有 issue 去重并尽量复现

这些条目是后续 reviewer 的优先操作清单，**未在本文件宣称运行复现，也未创建 issue**。

| 候选 | 源码证据 | 用户影响/最短复现方向 |
|---|---|---|
| R01 PC 自由编辑不进入统一数据流 | `PencilBoard` 没有 `onChange`；`App.tsx:640` 只执行 graph→`api.updateScene`；snapshot 存 `session.graph` | PC 改字/新形状后切模式、重开或追加输入，验证编辑丢失；导出格式之间可能不一致 |
| R02 保存失败不向用户显示 | `App.tsx:580` save catch 仅 console.error；`reset`/`openOther` 忽略 save 失败继续切会 | 注入测试 store 写入失败后修改→新会，验证是否被误认为已保存；应有持久保存状态与保留/导出恢复路径 |
| R03 模态行为不一致 | ModelPanel 独有 focus trap/Escape/aria-modal；Settings/Meetings/Diagnostics/Update/Keys 无相同处理；Composer `/` 不判断 `.modal` | 打开设置后 `/` 与 Tab/Escape，验证焦点泄漏和后台输入；读屏不应访问背景 |
| R04 无图节点时封锁全部导出 | `App.tsx:1966` `disabled={!graph.nodes.length}`，Markdown/HTML/PDF 构建本身可带 transcript/pages | 无模型文字记录、纯扫描 PDF、当前空页但其它页有内容时，用户可能无法带走会议 |
| R05 PDF 导入选择没有取消 | `App.tsx:2479` pendingPdf 仅“新会/这里”两按钮；导入作业也无 AbortController | 有内容的会议导入 PDF 后想撤回，是否被迫决定；大 PDF/图片过程中切会是否意外落入新会 |
| R06 更新失败后原按钮不能重试 | `updater.rs update_install` 在下载前 `Pending.take()`；失败后前端只重新 `install`，未重新 check | 可控无效更新 URL：第一次下载失败，第二次报 `No update ready`；应该保持可重试对象或重检 |
| R07 本地化穿帮 | PencilBoard 固定 `langCode="zh-CN"`；shell 托盘固定英文 | 切英文查看 PC 菜单；中文查看托盘；不需把两种无关根因拆成重复 issue |
| R08 编辑/删除失败反馈缺失 | MeetingsPanel rename/remove 的 await 未 catch；关闭编辑/确认发生在实际写入前 | 测试 store 拒绝写入/删除后应保持重试上下文，并说明操作未完成 |
| R09 资料丢失静默降级 | `loadAssets` 读不到底图直接 `underlay: undefined`；startup load 错误当 null | 打开缺素材/损坏会时应解释并保留导出救援入口，不能看似全新空会 |
| R10 首次体验/设置层级过长 | Settings 把 sample export、glossary 放在 audio 前面；模型面板先 SpeechModels 再全部模型列表 | 首次找“线上会议声音/录音故障/开始一场会”的点击和滚动成本；以用户任务组织设置 |
| R11 历史 loading/retry 不充分 | MeetingsPanel `list=null` 无 loading 文案；仅错误段落无 retry；query 变化时旧 found 保留到请求结束 | 慢存储和搜索时是否暂时显示旧结果；无结果/加载/读取失败要能区别 |
| R12 原生更新前缺前端保存握手 | updater 停音/停 LLM 后直接 restart，前端仅 500ms debounce 保存 | 正在录音/末句 finalize/刚编辑后更新：必须验证最终片段与板书落盘；源码不足以证明丢失 |

## 已有测试如何用于验证

保留并重用 `meetings/assets/privacy/diagnostics/shortcuts/search/replayHtml/pdf/importImage/importExcalidraw/summary/speech/voices/lasso/pointing/edit/beautify/i18n` 等测试，不为文档本身加测试。修复上述风险时需加真正覆盖失败边界或数据 round-trip 的小测试；现有纯函数测试无法证明控件能聚焦、真实权限可授权、WebView 能保存文件或 native update 可重试。

不得把未具备硬件/权限/网络的项目写成 PASS：C11、C20、C65 必须明确标阻塞原因或模拟证据，并给出真实 Mac 验收步骤。完整重构方案须保留这些发布门槛。

## 附录 A：更新失败重试的严格源码推导

证据等级 `SOURCE-ONLY`，未下载或安装更新，未运行伪装为生产 updater 的最小仿制测试。

1. `src-tauri/src/updater.rs:35-49`：只有 `update_check` 把检查得到的 `Update` 存到 `Pending`。
2. `src-tauri/src/updater.rs:77-83`：`update_install` 先对 `Pending` 调用 `Option::take()`；取走后原槽变成 `None`。
3. `src-tauri/src/updater.rs:86-95`：下载的 `Result` 用 `?` 提前返回；该失败分支没有把 `Update` 放回槽。
4. `src/ui/UpdatePanel.tsx:24-35`：捕获错误后把 `busy` 设回 false、清进度并调用 `llmEnsure`；没有 `updates.check()`。
5. `src/ui/UpdatePanel.tsx:79-80`：同一个按钮重新可点击，再次调用相同 `install()`。因此在**两次点击之间没有独立 update_check 成功补回 Pending**的条件下，第二次在 Rust 第 83 行直接得到 `No update ready to install; check for updates first`，不会进入 download。

关闭面板→设置→再次检查更新可能恢复，故不能写成“永久无法更新”。这个缺陷与已关闭 #17 的“接入自动更新”功能不同，是失败恢复路径。建议测试用可控下载错误重放真实状态机并断言第二次仍能发起下载；无须真实安装或重启来证明这一点。

## 附录 B：真实 Linux 原生 smoke，2026-11-03 15:41–15:44 +0800

证据等级 `NATIVE-SMOKE / MIXED-BUILD`。本节新增了真实 Tauri/WebKit/GTK 交互；不是前面 68 行全通过，也不是当前 HEAD Rust 或 macOS 验收。

### 环境与限制

- shell 是主 checkout 的既有 `src-tauri/target/debug/boardcast`，mtime `2026-11-02 20:33:43 +0800`，大小 407339128 bytes；早于 HEAD `4f9ab26`（2026-11-03 13:00）以及音源切换提交 `9c8a2a7`。未重编，因此原生新改动**未被验收**。
- 前端由本工作树 Vite 1420 提供；应用进程使用宿主 Linux GTK/WebKit，显示连接到既有 Cua `local:sb-5f1135f0` 容器的 Xvfb `:1`。只恢复了这个原本 suspended 的 Space，结束后已重新 suspend。
- 数据完全隔离：`XDG_CONFIG_HOME=/tmp/boardcast-native-audit/config`、`XDG_DATA_HOME=/tmp/boardcast-native-audit/data`、`XDG_CACHE_HOME=/tmp/boardcast-native-audit/cache`，另开 `dbus-run-session`；没有编辑/删除用户会议、下载 GB 模型或启用真实录音。
- 此 Xvfb 无窗口管理器，初始 1280×800 窗口超出 1080×760 显示；后调至 1080×760。原生文件选择器部分超显示边界不计作正常桌面缺陷。
- 共享依赖 symlink 导致 Vite allow-list 拒绝 DM Sans/JetBrains Mono 字体请求。因此截图用于说明功能状态/控件交互，**不用于准确字体、字宽和排版验收**。

运行命令（启动前以临时 symlink 将容器 `/tmp/.X11-unix/X1` 暴露给宿主；已清理）：

```sh
node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1420 --strictPort
env -u BOARDCAST_MODEL -u WAYLAND_DISPLAY DISPLAY=:1 GDK_BACKEND=x11 \
  XDG_CONFIG_HOME=/tmp/boardcast-native-audit/config \
  XDG_DATA_HOME=/tmp/boardcast-native-audit/data \
  XDG_CACHE_HOME=/tmp/boardcast-native-audit/cache \
  WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- \
  /home/hsiangnianian/GITPROJECT/boardcast/src-tauri/target/debug/boardcast
```

### 实际执行结果

| 操作 | 观察到的结果 | 证据 |
|---|---|---|
| 冷启动独立数据目录 | 真实桌面自动弹出模型面板，LLM 未装；SenseVoice in use；Apple 明确标本平台不可用；显示实际 NVIDIA RTX 3060 Laptop GPU 6 GB 与两档模型 | `evidence/native-models.png` |
| 关闭模型面板，点击设置，滚动 | 设置可进入，快捷键、Wayland 提示、诊断、纠错样本等原生专用选项存在；确认设置长页需要滚动 | `evidence/native-settings.png` |
| 设置→Export diagnostics | 实际 IPC 收集完成，显示 info.json 1434 chars、timing.json 189 chars、errors.txt 11 chars，Export zip 可点击 | `evidence/native-diagnostics.png` |
| Export zip | 弹出真实 GTK 保存文件选择器，默认文件名 boardcast-diagnostics.zip | `evidence/native-filepicker.png` |
| 文件选择器 Escape 取消 | 返回诊断预览；保持可再次导出，无虚假成功 toast | `evidence/native-filepicker-cancel.png` |

进程日志确认 `语音模型已预加载（1732 ms）`；这只是预加载，没有启动麦克风，也不证明实时识别成功。隔离 D-Bus 出现 AT-SPI、portal 与 secret proxy warning，不把这些环境告警直接登记为产品问题。

清理已完成：自身 Tauri 进程与 1420 Vite 退出（exit 130），宿主临时 X1 symlink 删除，Cua 恢复 suspended。未关闭 reviewer 的 1421/1422。保留 `/tmp/boardcast-native-audit` 测试数据和本目录证据图片。
