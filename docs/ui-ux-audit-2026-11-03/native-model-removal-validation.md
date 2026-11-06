# C17 Linux 模型删除失败与恢复

2026-11-05，HEAD `4f9ab26`。真实 Linux 权限故障复现：语音模型的主文件和词表已经删除，VAD 删除失败，取消确认框后仍显示“已就绪”和 SenseVoice“正在使用”，没有继续下载入口。现已修复；[24 组证据核对](evidence/implementation/native-model-removal/evidence-check.json)覆盖原生失败、取消、公共源补下载、原确认框重试及冷启动，C17 的完整跨模型范围仍保留。

## 修改

原生 `model_delete` 先卸载模型，再逐个删除文件；中途 `?` 返回跳过了末尾的重新加载与状态通知。现在保留删除结果，始终按剩余文件重新加载和发布状态，再返回原始错误。LLM 的选择记录仍只在成功删除时清理。

`ModelPanel` 原先只在成功后刷新目录。失败路径现在也刷新目录，同时保留原始操作错误和确认框；后续刷新失败不会覆盖原始错误。没有让用户重新打开面板才能看见真实状态。

产品只改 `models.rs` 和 `ModelPanel.tsx`；现有组件 fixture 与检查各扩展一个文件。没有改变保存屏障、模型文件名、哈希、公共 URL、默认语音策略或依赖。见[本批补丁](evidence/implementation/native-model-removal/product-and-tests.patch)与[四文件清单](evidence/implementation/native-model-removal/changed-files.json)。

## 真实桌面结果

所有原生操作来自同一实际 Tauri/WebKit UI，使用 UID 1000，在独立模型目录改变父目录权限制造 EACCES，没有模拟删除 IPC。

| 场景 | 结果 |
| --- | --- |
| 旧版中途失败 | 主模型和 tokens 已不存在，VAD 还在；界面仍显示 Ready/In use 和删除按钮。见[旧版截图](evidence/implementation/native-model-removal/before-stale-models.png)及[文件核对](evidence/implementation/native-model-removal/before-partial-delete-proof.json)。 |
| 新版首个文件删除失败 | 三个文件及哈希保持，错误与确认框保留。取消后仍可使用完整模型；原生日志新增一次成功预加载，证实卸载后恢复。 |
| 新版部分文件删除失败 | 错误保留；取消后 SenseVoice 提示下载所需资源，模型条目出现“继续下载”，不再显示“已就绪”。见[恢复截图](evidence/implementation/native-model-removal/fixed-partial-after-cancel.png)和[两种失败核对](evidence/implementation/native-model-removal/fixed-failure-proof.json)。 |
| 取消后继续下载 | 由实际原生应用从公共 ModelScope 获取缺失的主模型和 tokens，约 83.33 秒完成；VAD 复用已有文件。三个文件大小和登记 SHA-256 均匹配，界面恢复“已就绪”。见[下载中](evidence/implementation/native-model-removal/public-redownload-progress.png)、[完成截图](evidence/implementation/native-model-removal/public-redownload-complete.png)及[文件/连接记录](evidence/implementation/native-model-removal/public-redownload-proof.json)。 |
| 原确认框内重试删除 | 再次制造部分删除失败，恢复目录权限后直接点击原确认按钮；剩余 VAD 被删除，确认框关闭，提供重新下载入口。见[重试核对](evidence/implementation/native-model-removal/retry-delete-proof.json)。 |
| 正常退出及冷启动 | 模型文件和 `.part` 均未重新出现，没有活动下载；会议恢复“已保存”和原有五条原话。会议 JSON 与远程配置在操作、退出和冷启动后字节不变，见[冷启动](evidence/implementation/native-model-removal/cold-start.png)和[持久化记录](evidence/implementation/native-model-removal/persistent-after-final.json)。 |

公共下载这一步使用宿主正常网络命名空间、系统证书校验和未修改的目录 URL；应用没有代理或证书环境覆盖。进度截图显示 ModelScope，实际应用持有公网 TCP/443 连接，`.part` 从缺失增长到超过 200 MB 后完成。它补充了[上一批隔离 TLS 下载](native-download-validation.md)的边界，但不代表已逐一验证公共备用镜像。耗时包含网络与工具开销，不是性能承诺；没有服务端请求日志，也没有评价转写质量。

## 回归、身份与清理

- [19 项模型组件检查](evidence/implementation/native-model-removal/browser-models/check-models/models-check.json)通过。新增首文件失败保留、部分删除后刷新、直接重试清理三个场景；更正惯性使用的可访问树父级定位后，旧产品回归先在缺少 Resume 处失败，新产品通过。
- 生产 App 的[原生协议检查](evidence/implementation/native-model-removal/browser-production/check-native-shell/native-shell-check.json)通过，包含真正删除前的停止、尾句、保存顺序与保存失败阻止删除；这是受控 IPC，不能代替上表的原生文件证据。[中英文模型检查](evidence/implementation/native-model-removal/browser-production/check-model-download-language/check.json)21 项、40 处文案比较通过，均无页面脚本错误。
- [Rust 下载回归](evidence/implementation/native-model-removal/rust-model-tests.log)9 项通过，保持上一批取消、续传、校验和写入失败保护；2.7 GB 真实 LLM 下载测试仍 ignored。本批删除错误路径由实际原生权限故障验证，没有把这 9 项旧下载测试算成删除测试。
- 类型检查、播放器和主应用构建、locked/offline Linux debug 原生构建通过。没有重复全量 Node/Eval 或无关 Rust 测试。原生二进制 SHA-256 为 `99ee3601839851958a9deef54d2b8a27a5c0386951519be51f21721766cc02aa`；入口 `index-ie0Baq8R.js` 的 SHA-256 为 `ef8e0a46904c28ee591251a67b9962a00f6b571a6d5ef8344c4cf5a253f7126d`。[构建身份](evidence/implementation/native-model-removal/fixed-build/identity.json)的 196 个输入与结束时一致，1430 预览返回同一入口。

独立 XDG、D-Bus、AT-SPI、Xvfb/Openbox 与 Cua socket 不接入物理音频。初始模型使用前一批自有缓存的硬链接，仅更改当前测试目录权限和目录项，没有修改源权重或用户模型。旧版、新版、冷启动三个应用都先通过原生菜单正常退出，之后才停止私有桌面；18 个登记进程已结束，没有带当前私有运行目录的活动进程，宿主 hosts/NSS 与网络命名空间不变，见[清理记录](evidence/implementation/native-model-removal/final-cleanup.json)。1423/1430 服务保持运行。

一次只读路径断言写成 Path 下标，发生在删除失败后、重试之前；重新读取实际文件与确认框后只提交待执行的重试。公共下载的首个 AX 快照 JSON 被同名进度指标覆盖，原截图保留，并在同一次下载中另捕获独立实时快照。原始记录及[运行说明](evidence/implementation/native-model-removal/harness-notes.json)保留这些边界，未据此重复下载或改动产品。

归档不包含模型权重、二进制、真实用户内容或凭据。`python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/native-model-removal/check-evidence.py` 检查捕获证据及[哈希清单](evidence/implementation/native-model-removal/manifest.json)，不会重新操作桌面。没有安装应用、提交 Git、推送或发布。

未验证真实 LLM/视觉模型的切换与删除故障、Windows 文件占用、物理音频和模型效果。用户暂无测试 Mac，Mac/G3 与 G4 新用户任务试用仍保留，完整 C01–C68/G1–G4 未关闭。
