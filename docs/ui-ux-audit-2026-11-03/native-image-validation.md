# C43 Linux 图片导入与缺件恢复

2026-11-05，HEAD `4f9ab26`。修复图片导入失败后的配置入口：桌面版直接打开本地模型管理，网页版直接打开“板书与模型”。原文件和导入目标保留，关闭配置后可以直接重试。本批真实视觉识别、公共源组件下载、格式/大小/60 区域、远程启用下的本地路由与冷启动完成 [39 组捕获证据核对](evidence/implementation/native-image/evidence-check.json)。完整 C01–C68/G1–G4 仍未关闭。

## 改动与前后对照

产品只改 `App.tsx` 三行，复用已有模型管理、设置初始分类和导入任务；没有新依赖或 Rust 改动。两个既有浏览器检查补充入口、原文件重试、真实图片解码与存储断言。相对本批开始状态的 [补丁](evidence/implementation/native-image/product-and-tests.patch)与[哈希](evidence/implementation/native-image/changed-files.json)不混入此前重构。

旧版真实桌面点击“配置本地看图模型”进入[通用与外观](evidence/implementation/native-image/before-wrong-general.png)；中英文四个桌面协议场景和网页本地配置入口也先失败。修改后桌面直接进入[本地模型](evidence/implementation/native-image/fixed-direct-models.png)，网页选中“板书与模型”，设置关闭后原文件可重试。

## 真实桌面结果

Linux Tauri/WebKit 使用独立 XDG、D-Bus、AT-SPI、Xvfb/Openbox 与 Cua socket。已有真实 4B 权重复制到隔离目录，初始没有视觉组件；图像均为确定生成的合成测试材料。识别使用真实 llama-server，没有模拟视觉回复。

| 场景 | 实际结果与证据 |
| --- | --- |
| 缺件后下载、原图重试 | 从原生文件选择器导入 `workflow.png`，出现缺件错误；修复后的按钮直接进入模型管理。通过 UI 从公共 ModelScope 下载 672,423,616 字节的视觉组件，[SHA-256 与登记值一致](evidence/implementation/native-image/downloaded-vision-proof.json)。关闭面板后原文件仍在，直接重试成功；提交前原会议/配置均未改变。 |
| 新会议与实际文字 | PNG 识别出“会议流程、Plan、Build、Review”，明确选择新会议后写入一张板与图片附件。见[实际白板](evidence/implementation/native-image/image-imported-board.png)、[会议 JSON](evidence/implementation/native-image/image-new-meeting.json)和[保存核对](evidence/implementation/native-image/image-import-proof.json)。 |
| JPEG、WebP、大图 | JPEG、WebP 与 3600×2400 PNG 均读出相同四段文字，逐张明确追加。大图分出 17 个区域，4 个读到文字；保存底图为 1800×1200。每次追加前全部会议字节不变，追加后前面的整张板记录不变。见 [JPEG](evidence/implementation/native-image/remote-jpeg-proof.json)、[WebP](evidence/implementation/native-image/native-webp-proof.json)、[大图](evidence/implementation/native-image/native-large-proof.json)。 |
| GIF 与坏图片 | 静态空白 GIF 显示无文字提示，明确提交后保留背景。损坏 PNG 显示解码错误；取消后全部会议字节不变。见 [GIF](evidence/implementation/native-image/native-gif-preview.png)与[坏图](evidence/implementation/native-image/native-broken-proof.json)。 |
| 识别中取消 | 96 区域图片识别到 `1/60` 时取消，导入关闭、已有五张板和所有会议文件保持不变。见[取消核对](evidence/implementation/native-image/native-cancel-proof.json)。本条证明 UI/数据取消，不声称立即停止 llama-server 内正在执行的计算。 |
| 60 区域上限 | 重新选择同一 96 区域图片，真实识别得到 `T00` 至 `T59`；提交前明确显示[只读取前 60 区域](evidence/implementation/native-image/native-cap-preview.png)。保存完整范围的 1600×1200 底图，未识别的后 36 区域仍能看见。见[结果](evidence/implementation/native-image/native-cap-proof.json)。 |
| 远程启用时的图片去向 | 通过 UI 启用隔离的 OpenAI 兼容接口，当前会议没有勾选“只用本地”。随后 JPEG/WebP/大图/GIF/取消/上限流程仍使用独立的真实本地视觉服务；接口只收到两次文本连接探测，图片请求为零。见[路由核对](evidence/implementation/native-image/remote-image-routing-proof.json)与[本地服务](evidence/implementation/native-image/remote-jpeg-ready-engine.json)。接口本身位于 loopback，不是第三方供应商或外网抓包。 |
| 冷启动恢复 | 最终六张板、六份图片附件、原两场合成会议在正常退出和冷启动后逐字节一致；选中的第六张板及导入记录恢复，真实视觉服务再次健康就绪。见[冷启动截图](evidence/implementation/native-image/cold-ready-board.png)、[文件核对](evidence/implementation/native-image/persistent-after-cold-final.json)和[引擎记录](evidence/implementation/native-image/cold-ready-engine.json)。 |

六份实际落盘附件另外通过已有 Chromium 解码、尺寸检查及内容哈希核对，见[附件记录](evidence/implementation/native-image/native-saved-assets.json)。底图沿用现有 JPEG 编码与缩放规则，“保留完整范围”不表示保留原始编码字节。远程测试设置先经 UI 停用，再在应用退出后恢复隔离目录最初的空配置；原两场会议始终不变，见[最终原件核对](evidence/implementation/native-image/persistent-originals-final.json)。没有操作用户实际会议或模型目录。

## 构建与检查

- 最终 Linux debug 二进制 SHA-256：`32f6aa08e7fd3290379b8c0a51c067c018154201ce13faed6814b537dc83ccbb`。前端 `index-D6VTQ4Ea.js`，SHA-256：`af01417a93cb87a01311068d441ae6bf584ca265e460d4a5317334fc4189ad06`。见[构建身份](evidence/implementation/native-image/fixed-build/identity.json)。
- 引擎为实际检查的 [b11954 Linux x64](evidence/implementation/native-image/engine-identity.json)，使用 RTX 3060 Laptop 6 GB 的 Vulkan1、真实 4B 与对应 F16 mmproj；保持默认显存适配。所有被引用的最终健康检查均为 200，未将加载期间的 503 当作失败或重启任务。
- [中英文模型检查](evidence/implementation/native-image/browser-final-model/check-model-language/check.json)：56 项、42 处文案比较通过，包含四个图片配置入口和原文件重试；原来的远程 checking/ready 路由保持。
- [生产导入检查](evidence/implementation/native-image/browser-image-final-r2/check-import-task/import-task-check.json)：21 项通过，使用真实 Chromium 解码/IndexedDB、受控视觉回复；覆盖 PNG/JPEG/WebP/GIF、坏图、大图、60 次请求上限、取消、保存屏障与网页本地地址恢复。真实视觉证据来自上表。
- [4 项 Node 图片检查](evidence/implementation/native-image/node-image-tests.log)、类型检查、播放器/主应用构建、locked/offline Linux debug 构建及 diff 检查通过。未改 Rust，因此没有重复无关 Rust 或 Node 全量测试。
- 捕获 198 个跟踪输入；原生构建后仅继续扩展 `check-import-task.cjs` 的格式断言并重新通过，其他 197 个输入（包括全部产品输入）与构建时一致。不是在旧二进制上宣称验证后续产品改动。

三次应用均先从原生菜单正常退出，确认应用和引擎已结束，再停止自有桌面。24 个登记进程已结束，无残留隔离运行进程；1423/1430 原预览进程保留。见[清理记录](evidence/implementation/native-image/final-cleanup.json)。[39 组证据核对脚本](evidence/implementation/native-image/check-evidence.py)只读取归档并写核对结果，不重放 UI 或下载模型；[413 个文件的清单](evidence/implementation/native-image/manifest.json)排除了模型权重、二进制和浏览器资料目录。工具参数、AX 字段及两处新增检查断言的纠正记录在[运行说明](evidence/implementation/native-image/harness-notes.json)。部分截图包含 Cua 光标覆盖层。

[1430 生产预览](http://127.0.0.1:1430)返回本批前端字节；1423 的两份 HTML 也均返回 200。没有安装应用、提交 Git、推送或发布。

本批固定合成样例不代表任意照片、手写、动画/多帧 GIF 或极大文件的识别质量与内存表现。其他硬件、Windows、真实音频、签名安装更新未验；用户暂无测试 Mac，Mac/G3 与 G4 新用户任务试用继续保留。
