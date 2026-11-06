# 实际执行覆盖与剩余边界

此表保留开始重构前的审计快照；后续修复与新增运行证据见[实施记录](implementation-progress.md)，不能将表中旧失败状态视作当前工作树结论。

对应 [inventory](coverage-inventory.md) 的全部 C01–C68。每行的“partial”只表示测试了列出的状态，不把一个成功入口算作整组通过。R=当前前端真实浏览器运行，S=真实前端+模拟 IPC，O=仅源码，B=当前环境无法验证。原生 smoke 另见 inventory 附录 B；下面不重复把旧壳结果计为本 commit 原生通过。

| ID | 证据级别 / 状态 | 本次确实执行/观察 | 仍未执行或不确定 |
|---|---|---|---|
| C01 | R partial | 首开/刷新；reduce-motion 新上下文可进入主界面 | 慢字体、逐个跳过入口、3秒精确计时 |
| C02 | R+S partial | 网页无模型示例/手动输入；desktop no-model 自动模型面板 | 真下载后首次完整开会；所有缺件组合 |
| C03 | R partial | 英文录制示例完整回放、退出；示例未入历史列表 | 中文全示例、损坏样例、现场模型模式 |
| C04 | R+S partial | no-model、远程启用、更新并列；720/1280 窗口 | warmup/多GPU/所有状态并发 |
| C05 | R FAIL partial | PAD↔PC、已有板切换，PC手改消失 #73 | 录音中、全部资料页变体 |
| C06 | R partial | Enter、无模型仍保留、术语改写、刷新恢复 | 真实中日 IME、多万字、Shift+Enter边界 |
| C07 | R FAIL partial | `/` 从设置偷焦点 #74；输入正常展开 | 折叠持久化所有状态 |
| C08 | R partial | @QA/@Audit 切说话人发送 | 全角、12字上限、空格名、复制格式组合 |
| C09 | S partial | 权限拒绝提示、开始/停止、启动中取消 | 真实无设备/权限弹窗、尾句定稿 |
| C10 | S partial | USB设备、both→system热切 stop/start | 实际双路信号、拔设备、系统默认变化 |
| C11 | B | 无Mac原生权限环境 | Mac13+/旧版、首次授权/拒绝恢复、耳机外放 |
| C12 | S partial | 临时字幕、电平、开始/停止可见状态 | 真音量/削波、临时→最终长文本、最小化 |
| C13 | O | 源码显示CAM++与Apple边界 | 重叠/短句真实声纹、自动归属准确性 |
| C14 | S partial | 默认 shortcut_set 请求、设置入口存在 | 系统冲突、托盘、CLI、背景全局快捷键 |
| C15 | S partial | 按住Space开始、松开300ms后停止 | 失焦、PC冲突、starting释放等组合 |
| C16 | S partial | partial续传、40%速度ETA、暂停、空间不足/错误 | 真断点/校验/多源/重启partial |
| C17 | S partial | 已装第二LLM点击Use this，IPC与selected更新；删除拒绝显错、重试删除后出现Download | 真实模型停启/删除文件与后续录音；当前UI直接删除，无二次取消步骤 |
| C18 | S partial | 非Mac说明、SenseVoice未装/已装、检查入口 | 真实硬件/检测失败、完整事件清理 |
| C19 | S partial | Apple可用+已装zh_CN选择 | 真实下载、所有正式资产状态；mock en missing非正式枚举不能算通过 |
| C20 | B | 无兼容Mac | Apple下载→长录音→finalize/混语/退出全链路 |
| C21 | S partial | 远程启用、401测试错误、配置UI | 真HTTP/CORS/超时、Key沿用、断网fallback |
| C22 | S partial | 新域名consent取消/同意、local-only开关 | 抓包核实禁传、改域名、fallback私密性 |
| C23 | R+S partial | 完整设置滚动/语言/模型/术语/音源/更新/诊断 | 目标用户找入口耗时、所有错误返回定位 |
| C24 | R+S partial | 无模型失败保留、固定远程`-`无改图完成 | 真模型草稿确认、长队列、跨会竞态 #60 |
| C25 | R partial | 登录流程/多节点/问题；Excalidraw往返 | 全图种/公式/日文/密集长图、既有 #67 |
| C26 | R FAIL partial | 中间节点改名/删除；边缘toolbar出界 #78 | 拖动、边、空名、PDF固定块、键盘全可达 |
| C27 | R partial | Star、撤销后状态恢复 | 其余marks、词/句/段、复杂PDF命中 |
| C28 | R partial | Circle入口、手势、取消；手势未命中 | 目标消费/合并/8目标/排队归属不能记通过 |
| C29 | R partial | 删除→undo→redo→undo，UI记录正确 | 所有操作、键盘组合、跨会与输入框冲突 |
| C30 | R partial | Beautify点击完成并记日志 | 布局量化、失败/undo、密集大图 |
| C31 | R FAIL partial | 新PC文字导出包含；切模式后丢失 #73 | AI刷新与真实重开（源码同根因，未额外声称实测） |
| C32 | R FAIL partial | 扫描2页导航/新增空页；空页禁整场导出 #75 | 语音翻页、长缩略条、建议、忙时 |
| C33 | R+S partial/FAIL | Web存储刷新恢复；mock save拒绝无提示 #61 | 系统磁盘满、崩溃/退出flush、旧会议迁移 |
| C34 | R partial | 历史有会/无会，当前标题入口 | loading/错误retry、千条列表 |
| C35 | R partial | 无结果、中文转写/标题命中 | 多页跳转、快速输入竞争、摘要/板书命中 |
| C36 | R+S partial | 改名Enter生效，标题更新 | 空/长/IME、失败重试、blur/Esc所有路径 |
| C37 | R+S partial | 确认取消、确认删除当前会后空态 | 原生删除失败、素材清理与误复存 |
| C38 | S partial | 单页继续生成(cont.)和来源记录 | 多页选择/取消、原会hash不变、完整淡墨编辑 |
| C39 | R partial | 同输入导入PDF/Excalidraw/坏文件 | 系统选文件取消（见native smoke）、拖放/多选/MIME变体 |
| C40 | R FAIL partial | production两页PDF；新会/当前选择无取消 #76 | 双栏公式、文本层、40页、引用页 |
| C41 | R partial | 坏PDF报结构错误、扫描2页成功 | 加密/超大/导入中切会/中止 |
| C42 | R partial | 无效arXiv最终显示格式提示 | 真下载、合法版本号、重复点击/取消 |
| C43 | S partial | 无vision图片导入提示管理模型 | 真视觉识别、格式/超大/60块、隐私抓包 |
| C44 | R partial | 自有导出再导入4boxes/3lines/3texts；坏JSON | 未知字段、大文件、组/箭头语义完全保真 |
| C45 | R+S FAIL partial | 7项下载；纯转写/当前空页门禁 #75；诊断保存取消 | 原生路径无权限、大导出进度、多次点击 |
| C46 | R partial | Markdown生成且含示例记录（manifest） | 所有特殊字符/多页/中日英组合 |
| C47 | R partial | PNG/SVG生成；PC自由文字随Excalidraw导出 | 逐像素/截断/多底图/所有深浅模式 |
| C48 | R partial | Animated SVG生成 | 各宿主动画、reduce-motion与GIF/WebM |
| C49 | R partial | 整场PDF生成并再导入2页扫描 | 每种长文分页/裁切/字体准确性 |
| C50 | R partial/FAIL | Excalidraw往返与PC前后文本断言 #73 | 各编辑操作/全部结构保真 |
| C51 | R partial | offline新context file://打开板、摘要、6句 | 长会/多页/所有键控与浏览器兼容 |
| C52 | R partial | 示例终点拖动、首步、退出、1×2×4× | 录音/忙禁入、每步数据不写回完整断言 |
| C53 | R partial | 0步Fork禁用，中间分叉新增Board2 | 后续模型处理、跨页长期持久化 |
| C54 | R partial | 空/有内容侧栏、折叠、手改与多页记录 | 长会追随、阅读锁定、每种失败/回退 |
| C55 | R+S partial | Alice改名Esc取消；Alice→Alex成功；Alex→已有Bob合并；UI、保存IPC中的lines/history/node.by均Bob | 真原生文件重开、声纹身份、各页回放/undo归属、板上跳回 |
| C56 | S partial | 合成Markdown生成/再生成；Summarizing+Stop；取消后迟到结果未覆盖；远程失败+本地缺模型显错且旧摘要保留；恢复后重试成功 | 真实摘要质量 #68、长会分段进度/成功fallback、切会 #60 |
| C57 | R partial | 示例摘要进入编辑、改Markdown、结束编辑 | 剪贴板失败、清除撤销、持久化重开 |
| C58 | R+S partial | 两级术语保存、命中纠正、节点加术语 | 重复/坏格式、换会/继续会完整隔离 |
| C59 | R+S partial | 合成模型输出后PAD改字产1样本；cases JSON与DPO JSONL解析/字段核对，含改前改后；保存取消/权限拒绝/重试成功 | 原生真实路径/训练消费、跨会议大量样本、Eval实际执行质量 |
| C60 | R partial | 系统→浅→深、messy/weight滑杆 | 系统实时变更、全部参数导出一致性 |
| C61 | R FAIL partial | 中英即切；英文PC仍中文 #45 | 托盘/全部错误/所有长文、日文系统 |
| C62 | R FAIL partial | 帮助入口/快捷键列表；Esc不关 #74 | 每个组合、Mac Cmd、本地键布局 |
| C63 | S partial | 四文件预览、展开info、导出取消/成功 | 真zip落盘、全敏感数据模糊测试 |
| C64 | S partial | mock新版本、notes显示、检查入口 | 无更新/离线/实际自动时机 |
| C65 | S+O FAIL partial | 两次install无check，mock No update ready；Rust pending推导 #77 | 真签名、安装/重启、尾句保存、断电 |
| C66 | B | 当前agent无Mac；另有Linux旧壳smoke | Mac窗口/Dock/菜单/后台/进程清理 |
| C67 | R FAIL partial | Settings焦点/Esc/`/`、ModelPanel Esc；720无整页横溢出 | VoiceOver、所有焦点/对比度/触控、200% |
| C68 | O | 源码审查normalize/asset缺失路径 | 真崩溃、坏会、大量长会、缺底图用户反馈 |

不得用这份矩阵宣称“68组全通过”。所有剩余边界必须在重构发布准入中分配测试环境、负责人和验收步骤；真实Mac采集、数据恢复、编辑持久化与更新握手尤其不能豁免。
