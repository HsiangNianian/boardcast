# C65：下载期间继续会议、验签与安装前保存

2026-11-05，基线 `4f9ab26` 的当前工作树。旧版下载时禁用“以后再说”，Escape 无法关闭，会议背景也无法获得焦点；真实 Linux 下载在 6% 停滞 96 秒仍未恢复。见[组件观察](evidence/implementation/native-update-download/baseline-browser/probe-download/probe.json)和[原生界面](evidence/implementation/native-update-download/baseline-still-downloading.png)。

现在先“下载更新”，校验完成后再由用户选择“保存并重启”。下载期间可以关闭面板并继续会议；重新打开会保留进度、错误或已校验内容，完成下载不会自动停止会议或安装。复用同一个 UpdatePanel 保存任务状态，仅在更新版本变化时重建；安装前收尾和安装阶段仍禁止关闭。原生请求增加 30 秒连接及单次读取期限，持续传输不受 30 秒总时长限制。

| 实际路径 | 验证结果 |
| --- | --- |
| 下载停滞、网络失败与重试 | 接收 64 KiB 后停滞的请求在 30.004 秒关闭；HTTP 502 与被篡改的下载内容分别显示下载/签名错误，原面板可以直接重试。 |
| 返回会议和慢下载 | 两次下载期间分别在约 4.9 秒实际输入并保存原话。慢传输持续 35.704 秒后通过真实插件验签，显示[等待明确安装](evidence/implementation/native-update-download/fixed-ready-visible.png)，没有自动安装。 |
| 真实保存故障 | 已校验后，将测试会议目录改为只读并输入第三句。安装前实际 EACCES 保留原文件，显示[收尾失败](evidence/implementation/native-update-download/fixed-save-error.png)。恢复原权限后重试，第三句先保存，再进入安装阶段。 |
| 安装失败与内容保留 | 插件再次验证签名后拒绝非安装包格式。程序文件不变，重复安装与重新检查更新沿用已校验字节，没有再次下载。中文 [720×480 界面](evidence/implementation/native-update-download/fixed-install-error-compact.png)的错误、稍后和重试按钮可见。 |
| 正式配置与冷启动 | 使用相同产品源码、正式更新配置的新进程恢复三句原话，会议与锁文件字节不变，并成功检查实际 GitHub 更新地址。 |

[31 项证据核对](evidence/implementation/native-update-download/evidence-check.json)通过；维护面板、Dialog、生产 App 原生协议与保存/退出屏障回归通过，[5 项 Node](evidence/implementation/native-update-download/node-checks.log)、[4 项 Rust](evidence/implementation/native-update-download/rust-tests.log)、类型检查和前端/原生构建通过。没有重复无关全量测试。产品改动涉及 App、UpdatePanel、updater 和中英文词典，没有新增任务管理器或依赖。

原生下载使用新生成的测试签名和 1 MiB 普通文本样例，它故意不含 ELF/gzip 文件头，因此安装测试在触碰程序文件前返回 `invalid updater binary format`。这证明真实验签及失败恢复，不代表真实安装包升级成功。最初本机 TLS 证书未获信任；随后测试源仅监听回环 HTTP，允许该传输的选项只存在于隔离构建配置。项目正式更新地址、公钥与 HTTPS 要求未改；最后的冷启动构建移除了测试覆盖，实际公共 HTTPS 检查通过。私钥及私钥生成日志已删除，归档只保留公钥、签名和样例。

私有下载构建、正式配置构建及前端身份分别见[下载构建](evidence/implementation/native-update-download/fixed-build/identity.json)、[正式配置构建](evidence/implementation/native-update-download/public-build/identity.json)和[预览核对](evidence/implementation/native-update-download/identity.json)。193 个记录的源码输入与当前工作树一致。四个应用进程正常退出，24 个登记的原生进程及两个测试服务已停止，1423/1430 原预览保留。具体输入、故障与复跑边界见[运行说明](evidence/implementation/native-update-download/harness-notes.json)，归档见[清单](evidence/implementation/native-update-download/manifest.json)。

真实安装包下载/升级/重启、正式发布密钥对应的下载验签、原生未知总量进度、真实收音尾句与模型恢复、Mac/VoiceOver 和 G4 用户试用仍待验收。未实际安装、提交、推送或发布。
