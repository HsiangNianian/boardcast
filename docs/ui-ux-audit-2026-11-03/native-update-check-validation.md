# C64：真实更新检查、超时与重试

2026-11-05，基线 `4f9ab26` 的当前工作树。真实 Linux 应用在代理接受连接但不返回结果时，手动检查等待 **143 秒**仍停在“检查中”；启动后的自动请求也没有结束。释放代理后才显示底层网络错误。见[旧版观察](evidence/implementation/native-update-check/baseline-observation.json)和[旧版界面](evidence/implementation/native-update-check/baseline-still-checking.png)。

现在清单检查有 30 秒期限，超时释放实际请求；共享 IPC wrapper 给设置、原生菜单及自动检查提供一致的中英文错误。手动失败说明检查网络或代理后重试，自动失败继续静默。期限仅作用于清单检查；包下载、验签和安装流程沿用现有行为。产品改动为 `updater.rs`、`desktop.ts` 和两份字典，未新增依赖。

| 场景 | 本轮证据 |
| --- | --- |
| 原生超时 | 自动、英文设置、中文设置三次真实请求分别在 29.990、29.990、29.983 秒关闭连接；手动提示出现后“检查更新”恢复可用。 |
| 真实联网重试 | 英文设置在超时后直接访问既有 GitHub 更新地址，返回“已是最新版”；中文原生菜单先经历代理 502，再直接重试并得到相同结果。TLS 透传，没有替换原生清单或关闭证书验证。 |
| 窄窗恢复 | [720×480 中文错误界面](evidence/implementation/native-update-check/fixed-zh-compact-error.png)中，错误、检查更新和关闭按钮均可见。 |
| 冷启动与会议 | 关闭自动检查后正常退出，重新打开 27.8 秒内没有更新请求，原话恢复且显示“已保存”；会议与锁文件在检查、退出和冷启动后逐字节保持。 |
| 生产页面协议 | 中英文设置和菜单覆盖超时重试、无更新、版本说明、以后再说；检查与推迟不会下载、安装或改变会议。自动检查在启动八秒后执行，失败不弹窗。新版本场景使用受控 IPC；本轮真实公共源没有新版本。 |

[25 项证据核对](evidence/implementation/native-update-check/evidence-check.json)通过。现有维护面板的下载/签名/保存/安装失败重试、Dialog、生产 App 保存与退出屏障回归通过；[12 项 Node](evidence/implementation/native-update-check/node-checks.log)、[4 项 Rust 更新状态检查](evidence/implementation/native-update-check/rust-updater-tests.log)、类型检查、前端及私有原生构建通过。未重复无关全量测试。

新原生二进制 SHA-256 为 `660756ec771cacdb3c00a35a82a6879975b6d6bd1e92cee2a4bc57205100d396`，冷启动进程文件已核对；193 个记录的源码输入与当前工作树一致。见[构建身份](evidence/implementation/native-update-check/fixed-build/identity.json)、[运行身份](evidence/implementation/native-update-check/cold-live-identity.json)和[预览身份](evidence/implementation/native-update-check/identity.json)。三个测试应用正常退出，18 个登记的原生进程及代理已停止，1423/1430 原预览保留。

测试使用独立桌面、目录和一条实际键入的合成原话。中文设置的一次联网重试在完成前被关闭，未计为成功；后续原生菜单的成功结果单独等待并取证。输入路由、基线构建差异与复跑边界见[运行说明](evidence/implementation/native-update-check/harness-notes.json)，文件清单见[归档](evidence/implementation/native-update-check/manifest.json)。

本轮未下载更新包、验签、安装或重启到新版本，也未执行真实音频、Mac/VoiceOver 和 G4 用户试用。C65 及完整 G3/G4 门槛继续保留；未提交、推送或发布。
