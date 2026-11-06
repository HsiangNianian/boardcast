# Boardcast 客户端独立体验审查

基线 `4f9ab26a46f07cc5575460fd7431f1a995e705cc`，2026-11-03。评审 agent 实际操作当前前端，并对桌面分支注入有明确边界的 Tauri bridge。覆盖明细见 [runtime-coverage.md](runtime-coverage.md)，完整功能定义见 [coverage-inventory.md](coverage-inventory.md)。父 agent 另外组织的 Linux 原生壳 smoke 见后者附录 B。

## 锐评

画布的手写感值得保留，但现在的软件更像“一套可以演示很多技术能力的画布”，还没有形成稳定的会议工作台。最严重的体验问题发生在信任边界：用户在 PC 模式亲手写下的字会在切模式后消失；明明还有会议记录，导出却整组变灰；用户想撤回 PDF 导入，没有取消；弹窗开着时键盘还能跑到背后的输入框。漂亮的笔迹无法弥补这些问题。

命名也在制造无谓成本。“PAD Whiteboard / PC Meeting”让人猜设备差异，实际对应两套行为、编辑能力与数据保存程度都不同的画布。工具条用一串图标装下会议、编辑、导入、导出和外观，用户要逐个悬停才知道什么能做。左上会议标题又兼任历史入口，当前会议名称被压缩后尤其难发现。模型、录音、文字输入之间的依赖没有形成一句话说清的“现在可以做什么”。

设置是另一张问题地图：连接模型、音源和麦克风这些开会必需项，与训练样本导出、笔迹参数、诊断混在同一个很长的表单里。真实使用中，找音源要经过解释远程模型、快捷键、诊断、纠错样本、术语的长段文字。每段说明都可能正确，叠在一起就成为用户开会前必须翻阅的文档。

有做得好的部分：录音启动能取消；已观察的权限错误有具体恢复指引；模型下载进度包含速度、来源、ETA，空间不足能明确阻止下载；远程启用前列出发送内容和不发送内容；摘要能手改；回放和导出覆盖丰富；扫描 PDF 有清楚的“没有文字层”说明。重构应该保住这些已有能力。

## 证据边界

- **runtime**：Linux Chromium 153，当前工作树 production Vite build + preview `127.0.0.1:1422`；开发页 `1421` 用于早期 UI 验证。浏览器独立数据，只有合成文本和仓库自带示例。没有调用付费云模型。
- **simulated**：真实 React UI、事件与点击，Tauri IPC 用 [bridge fixture](evidence/boardcast-desktop-mock.js) 返回可控状态。模型下载、Apple 选择、录音/音源切换、云连接、更新失败、诊断保存的这些结果只证明前端如何响应，**不证明实际硬件、下载、模型、系统授权或安装成功**。调用记录 [mock-calls.json](evidence/mock-calls.json)、[audio-switch-simulated.txt](evidence/audio-switch-simulated.txt)。fixture 的英文 Apple `missing` 值不是正式状态枚举；没有把该分支用于缺资源/Auto 验收，已装中文选择和非 Mac 分支可独立观察。
- 没有可用 Mac，VoiceOver、Apple 真实模型安装、双路音频隔离、Dock/托盘、签名更新与重启仍待目标 Mac 验收。不得把本报告包装为“所有平台所有状态通过”。
- 开发服务器链接已有依赖时 PDF `?url` worker 导入曾报 `Invalid workerSrc type`；生产 build + preview 同一 PDF 成功。此项记为开发测试环境差异，没有据此新开产品 issue。

## 已确认且已去重开单

优先级是本评审建议，并非修改 GitHub labels。

| 优先级 | 已确认问题 | 复现与证据 | 验收方向 |
|---|---|---|---|
| P0 | [#73 PC 自由编辑丢失](https://github.com/HsiangNianian/boardcast/issues/73) | 结束示例回放→PC Meeting→T 插入 `QA PENCIL PERSISTENCE`→导出→切 PAD 再切 PC→再次导出。前后 [.excalidraw](evidence/pencil-before.excalidraw) / [after](evidence/pencil-after.excalidraw) 的文本断言确认消失；[前图](evidence/boardcast-pencil-before.png)、[后图](evidence/boardcast-pencil-after.png)。根因 `PencilBoard.tsx` 无 onChange，`App.tsx:640` 单向 graph→scene。 | 手改进入明确的持久数据模型；切模式/切页/保存重开/AI 更新不丢；各导出如实包含或明确解释支持范围。 |
| P1 | [#74 模态键盘泄漏](https://github.com/HsiangNianian/boardcast/issues/74) | Settings 打开焦点仍在背景入口；Esc 不关闭；Close 上 Shift+Tab 到背景 Collapse；`/` 下一帧聚焦背景 What was said。快捷键帮助也 Esc 不关闭。ModelPanel 的 Escape 能关闭，行为不一致。 | 统一初焦点、Tab trap、Esc、焦点归还、aria-modal 与背景快捷键隔离；沿用已有 ModelPanel 能力并测试全部弹窗。 |
| P1 | [#75 导出门禁错误](https://github.com/HsiangNianian/boardcast/issues/75) | 无模型输入文字后 transcript 有内容，Export disabled；另导入两页扫描 PDF 后新建空白第 3 页，整个 Export disabled，回第 1 页恢复。扫描 PDF 自身在有占位节点时可以导出，不能泛称所有扫描页失败。 | 当前画布导出与整场记录导出分别按实际内容启用；纯记录可取回；当前空页不能阻止整场 PDF/HTML/Markdown。 |
| P2 | [#76 PDF 没有取消](https://github.com/HsiangNianian/boardcast/issues/76) | production 中向已有示例会导入有效两页 PDF，只剩 New meeting / Add to this meeting；Esc 后仍在。[截图](evidence/boardcast-pdf-choice.png)。 | 提供取消并保持原会不变；后续方案补导入过程取消、目标会固定及切会竞态。 |
| P1 | [#77 更新失败重试无准备对象](https://github.com/HsiangNianian/boardcast/issues/77) | UI bridge 首次返回网络失败、再次返回 No update ready；调用顺序两次 install 间没有 check。[截图](evidence/boardcast-updater-mock.png)。真实 Rust `Pending.take()` 推导见 inventory 附录 A，未安装软件。 | 下载失败后原动作能有效重试；不承诺未落盘会议已保存；真实签名包与重启前保存握手单独验收。 |
| P1 | [#78 编辑工具条越界](https://github.com/HsiangNianian/boardcast/issues/78) | 1280×800 英文，导入示例 Excalidraw 后点左侧 Web app；Rename rect `x=-74.61, right=-8.97`，鼠标无法点击。[截图](evidence/boardcast-toolbar-clipped.png)、[DOM](evidence/toolbar-clipping.txt)。 | 按工具条实际宽度与视口定位，左右上下均不越界；长英文/中文、缩放、窄窗、资料块长工具条可达。 |

## 已有 issue，补证不重复开

- #61：bridge 注入 `meeting_save` 拒绝后 UI 仍显示正常转写，屏幕没有保存失败提示；[运行记录](evidence/lasso-shortcuts-save-error.txt)。这里只验证失败反馈，未宣称覆盖系统磁盘满/崩溃恢复。
- #45：PC 组件在英文界面仍显示中文“形状”“画布动作”；已有问题覆盖，保留在整体本地化验收。信息架构、弱状态层级、图标可发现性也归总体验项。
- #60、#68、#69–72：跨会异步结果、摘要真实性、Mac 权限切换/双语/字幕延迟/JSON 异常已有专门跟踪，本次没有额外重复建单，也没有给它们补上虚假的硬件通过。
- #32/#33：PDF 选页、复杂论文识别等仍归已有导入问题；#76 只记录独立的取消缺口。
- #53：本次生成 Animated SVG，但没有验证所有宿主渲染、GIF/WebM 能力，保持原问题边界。

## 真实操作通过的代表路径

1. 无模型网页播放内置英文示例，拖到终点，退出回放，摘要/6 条转写可见。7 种导出都生成了文件：Markdown、PNG、SVG、Animated SVG、整场 PDF、回放 HTML、Excalidraw；尺寸和 SHA256 见 [manifest](evidence/export-manifest.json)。PDF 再导入后成为 2 页；Excalidraw 再导入识别到 4 boxes、3 lines、3 texts。没有用“文件已下载”推断所有视觉内容均正确。
2. 回放 HTML 在新浏览器上下文、`offline: true`、`file://` 打开，板书、摘要、6 条转写出现；[截图](evidence/boardcast-offline-replay.png)、[日志](evidence/offline-replay.txt)。应用内回放首步 Fork 禁用，中间步分叉新增 Board 2，1×→2×→4×按钮可操作。
3. 原生网页存储的合成 transcript，刷新后恢复；历史搜索无结果/中文命中，改名、删除确认/取消、确认删除后空态均可操作。[round-trip](evidence/web-meeting-roundtrip.txt)。另模拟桌面“接着旧板开会”生成新标题和来源记录。
4. PAD 中部节点改名、Star、加入术语、删除、撤销、重做、美化均走过；边缘节点编辑暴露 #78。圈选入口/取消可用，但本次手势未命中目标，不将其记成语义圈选成功。
5. 全局与本会术语把 `board cast q a` 修正为 `Boardcast QA project`，转写保留 rawText；摘要编辑能落入当前 UI。模型生成内容正确性未用固定 `-` 响应评价。
6. 损坏 JSON、损坏 PDF、缺视觉组件图片分别有可读错误；生产 PDF 正常导入与扫描说明可见；无效 arXiv ID 最终显示格式提示。没有下载真实 arXiv 或视觉模型。
7. 桌面模拟录音权限拒绝→提示→再次开始→临时字幕/电平→停止；starting 中取消后恢复 Start listening；按住 Space 显示 Stop，松开后回 Start。录音中 both→system 调用 stop 再 start；只证明调度与反馈，不证明两路声音。
8. 设置中远程同意/取消/启用、401 测试错误、local-only、音源/设备、术语、中英语言、笔迹滑杆、快捷键说明、更新、诊断预览/导出取消/成功都实际操作过。诊断 preview 不含合成会议内容；未进行完整敏感数据模糊测试。

## 核心入口补测（同一隔离环境）

在最终覆盖审查后补齐之前仅源码检查的模型操作、说话人和摘要入口，不将这些模拟状态扩大解释为后端实机通过。

- **C17 模型操作**：用已安装的合成第二 LLM 点击 Use this，检查 `model_select` 参数与 selected 状态；删除返回模拟错误，UI 显示 `QA model action denied`；解除错误重试后 UI 从 Delete 变为 Download。[操作与错误记录](evidence/model-actions-simulated.txt)、[按真实 IPC 语义克隆响应后的刷新结果](evidence/model-actions-cloned-ipc.txt)。第一次 fixture 直接返回同一对象导致 React 不刷新，后一次以 structuredClone 模拟 IPC 数据序列化，排除了测试夹具自身问题。未把它作为产品缺陷。
- **C55 说话人**：产生 Alice/Bob 两条合成记录；改名 Esc 取消保持原名，Alice→Alex 成功，再把 Alex 改成已有 Bob；UI 两句均为 Bob，保存调用中的 lines、history、板书 node.by 也均为 Bob。[证据](evidence/speaker-rename-merge.txt)。真实声纹和原生文件重开仍未测。
- **C56 摘要**：返回四节合成 Markdown，UI 正常渲染；1.8 秒响应延迟期间明确显示 Summarizing… 和 Stop；取消后，随后返回的 `CANCELED RESULT MUST NOT APPEAR` 没有覆盖旧摘要；远程异常后尝试本地 fallback，但本地缺模型，因此出现 Summary failed，并保留旧摘要；解除错误后可再生成。[成功/取消/失败](evidence/summary-success-cancel-error.txt)、[等待状态](evidence/summary-pending.txt)。这只验证 UI 生命周期，不评价摘要真实性或 #68 是否修复。
- **C59 纠错样本**：让合成模型生成 `QA wrong extra`，马上在 PAD 改成 `QA corrected extra`；设置显示 1 collected。点击两种导出后解析实际传给保存接口的 UTF-8 内容：cases JSON 1 条、1445 bytes；DPO JSONL 1 条、9320 bytes，含 prompt/chosen/rejected，改前和改后文字均存在。保存取消保留设置，拒绝时显示错误，重试成功。[内容校验](evidence/correction-export-validation.txt)、[取消/失败/恢复](evidence/correction-export-failure-cancel.txt)。没有执行训练或用固定模型响应声称 Eval 质量通过。

## 重构约束交接

先修不可逆的失信行为，再调整壳层。会前应明确模型/音源是否就绪；会中把“正在听、听到什么、板书处理中、已保存/保存失败”分开，且允许用户安静看板；会后让摘要、记录和导出成为明确步骤。旧会议文件兼容必须作为迁移条件，PC 的自由编辑数据模型必须在换皮前决定。

视觉上推荐安静的原生感框架 + 有身份的手绘画布：工具栏与表单用稳定字体、间距和状态色，画布维持板书/粉笔个性；不把控制面板也写成手写体。Things/Linear 的层级、Raycast 的可发现命令、Audio Hijack 的收音链路是行为参考，来源与详细设计取舍见 [research.md](research.md)。不要为模仿竞品加入新社交、云同步或模型实验。

未检查：完整 Mac 原生链路、全部 68 组的每个异常组合与长时间负载。风险：已有编辑丢失、导出和保存反馈问题仍未在本轮修改源码；本报告与 issue 是重构输入，不是修复完成声明。
