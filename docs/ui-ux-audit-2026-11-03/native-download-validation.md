# C16 Linux 原生下载暂停与恢复

2026-11-05，HEAD `4f9ab26`。实际 Linux Tauri 下载在等待响应头或下一段正文时，点击“暂停”仍要等网络返回。现已修复；真实界面完成暂停、保留部分文件、冷启动续传、全部源失败后的重试、备用源完成及再次冷启动。[23 组证据核对](evidence/implementation/native-download/evidence-check.json)通过，C16 与完整 G1–G4 验收仍保留未测范围。

后续 [C17 删除恢复续批](native-model-removal-validation.md)补充实际原生应用从公共 ModelScope 获取缺失主模型与 tokens、复用已有 VAD 并验证全部哈希的证据。下文保留本批受控 TLS 源的原始验证范围。

## 根因与修改范围

旧 `AtomicBool` 只在网络 await 之后检查，不能唤醒正在等待的请求。共用下载器改用已安装 Tokio 的 `watch` 通知，在等待响应头及正文时选择取消分支；写入完成并 flush 后保留 `.part`，不在取消后继续尝试其他源。flush 错误现在正常返回失败，不能继续把文件标为已安装。

续传核对部分文件后，原先必须收到正文且达到进度节流时间才发出“下载中”事件，因此等待响应头时仍显示“校验中”。现于每次网络请求前发出下载阶段和源名称。原生记录和对应 Rust 回归均先复现失败，再通过。

本批只修改 `models.rs`、`models_tests.rs` 和 `Cargo.toml` 的 Tokio `sync/macros` feature；没有新增包或修改锁文件。[194 个输入的前后哈希及三文件清单](evidence/implementation/native-download/changed-files.json)、[最终源码](evidence/implementation/native-download/source/src-tauri/src/models.rs)与构建身份一起归档。没有改变模型目录、公共 URL、登记哈希或前端文案。

## 实际原生结果

| 场景 | 观测 |
| --- | --- |
| 旧版等待响应头时暂停 | 点击后 732、1363、2470 ms 仍有“暂停”按钮，连接未关闭；释放服务端回复后才恢复。见[旧版记录](evidence/implementation/native-download/baseline-pause-proof.json)。 |
| 新版等待响应头时暂停 | 303 ms 观察到连接关闭，741 ms 的快照已空闲；没有尝试备用源。 |
| 新版收到部分正文后暂停 | 317 ms 关闭连接，743 ms 的快照已空闲；保留 1,048,576 字节，逐字节等于真实模型前缀。见[暂停记录](evidence/implementation/native-download/fixed-pause-proof.json)和[截图](evidence/implementation/native-download/fixed-body-paused.png)。 |
| 关闭面板和冷启动 | 仍可“继续下载”，部分文件大小及 SHA-256 不变；没有自动发起下载。 |
| 三个源全部失败 | 按目录顺序收到三次 503，每次均带 `Range: bytes=1048576-`。界面保留错误和续传入口，部分文件不变。见[错误恢复](evidence/implementation/native-download/resume-error-proof.json)。 |
| 重试与备用源完成 | 首源 503 后备用源返回余下 238,185,265 字节；tokens 和 VAD 也经备用源完整下载。三个文件大小和登记 SHA-256 全部一致，`.part` 消失。见[文件与请求记录](evidence/implementation/native-download/resume-success-proof.json)。 |
| 安装后重启 | 仍显示“已就绪”和 SenseVoice“正在使用”，无新下载；文件哈希保持。见[冷启动截图](evidence/implementation/native-download/cold-installed-models.png)。原生日志记录模型预加载，但本批没有输入音频或评价转写效果。 |

时间从 Cua 点击调用开始计算，包含工具开销，是本次观察值，不是性能承诺。最终三次运行均先通过原生菜单退出，确认应用已结束，再停止私有桌面。实际会议 JSON 和远程配置在暂停、失败、续传及两次冷启动期间字节不变，见[持久化核对](evidence/implementation/native-download/persistent-after-final.json)。

## 数据、环境与回归边界

三个语音文件先通过独立 Python 下载器从目录中的 ModelScope URL 获取，并核对登记大小与 SHA-256，见[来源记录](evidence/implementation/native-download/fixture-download-result.json)。原生应用仍使用未改动的生产目录 URL，但这次在私有网络与挂载命名空间中，把三个源解析到受控 TLS 服务，以准确保持响应头、暂停正文和返回 503。服务传输真实模型字节并支持 Range；证书校验正常开启，信任仅限该应用进程。

这证明实际 Tauri、TLS、下载器、文件和界面的链路；不等于原生应用直接连接公共 ModelScope/hf-mirror 的网络验收。宿主 `/etc/hosts`、NSS 配置和命名空间前后保持，未改系统代理。独立 XDG、D-Bus、Xvfb/Openbox、Cua socket 不接入物理音频；合成会议与用户数据隔离。

- [Rust 下载测试](evidence/implementation/native-download/rust-model-tests-complete.log)：9 项通过，覆盖真实 TCP 停滞取消、提前取消无文件/网络副作用、续传阶段、部分文件保留、源回退、坏哈希与空间预检。Linux `/dev/full` 检查 buffered write/flush 失败不能安装。2.7 GB 真实 LLM 下载测试仍为 ignored。
- [模型组件回归](evidence/implementation/native-download/browser-dev/check-models/models-check.json)：开发 origin 的实际组件 fixture 16 项通过，无页面错误；包含删除保存屏障、订阅清理与恢复操作。没有称为生产页面或原生故障测试。
- 最终 Linux debug 原生构建通过；二进制 SHA-256 为 `5e3e8ff3c8b5db51b9a48019b98c14d0dec1e2d6fcf51eeeb70eb259e73ae0f9`。[构建身份](evidence/implementation/native-download/final-fixed-build/identity.json)对应最终源码。前端未变，复用 `index-AZtTPHuC.js`，SHA-256 为 `4c3a392becf1bb9d86608f6ff2b0c4a083eeffbbc0bae3a0c7ed23bcb7628725`，与 1430 预览一致；本批没有重复全量 Node、前端构建或无关 Rust 测试。

初期私有 NSS 路由与测试 CA 配置、组件 runner 误用生产 origin、测试编译断言及 Cua 生命周期的修正记录在[运行说明](evidence/implementation/native-download/harness-notes.json)。完成截图中的“已就绪”标签未出现在 Cua AX 标签列表，首次成功断言因此失败；检查实际截图、安装文件和“删除”操作后只执行证据核对，没有再次下载。没有为测试工具的假设改动产品。

证据目录不包含模型权重、二进制、证书私钥或用户会议。[哈希清单](evidence/implementation/native-download/manifest.json)可用 `python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/native-download/check-evidence.py` 核对；该脚本只检查捕获证据，不重新运行原生 UI。六次隔离应用均正常退出，42 个登记进程已结束，见[清理记录](evidence/implementation/native-download/final-cleanup.json)。1423/1430 预览保持可用，没有安装应用、提交 Git、推送或发布。

未验证原生公共下载源、真实磁盘满/损坏模型下载的完整 UI 恢复、LLM 激活、物理音频及模型质量。用户明确暂无测试 Mac，Mac/G3 与 G4 新用户任务试用继续保留。
