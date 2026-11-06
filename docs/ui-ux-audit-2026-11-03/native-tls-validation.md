# C21/C22 Linux HTTPS 与认证代理续批

2026-11-05。真实 Linux Tauri/WebKit、Rust IPC、TLS 握手与 HTTP CONNECT 代理，使用私有 CA 和合成凭据。发现并修复证书、代理认证及隧道失败都只显示 `Can't connect: error sending request` 的问题。最终中英文错误提示、原句重试、配置保持和冷启动均已验；[57 组证据核对](evidence/implementation/native-tls/tls-check.json)通过。

## 修复与构建身份

`src-tauri/src/remote.rs` 保留超时优先级、URL 去除和凭据脱敏，从底层错误链识别证书验证、代理认证、代理连接三类已知原因，仅返回固定错误码。`src/remote.ts` 在原有 IPC 调用入口翻译这三个错误码，未知错误仍原样抛出；中英文提示说明应检查的地址、证书、时间或代理设置。没有放宽证书检查或增加绕过开关。

本批改动仅涉及上述两个产品文件、两个语言字典及新的传输回归测试；Rust 回归放在原有远程测试模块。错误分类依赖当前 reqwest/rustls 的原因文本，源码已标注依赖升级时复测；没有把全部底层详情或代理凭据返回界面。

- 修复前复用[归档打开修复](archive-open-save-validation.md)的原生构建：`28757fbac8f370ef28eafcfdaf9bb0f641bdea90c4a3c6727f4e00f1d559e47e`。
- 修复后原生二进制：`97f8bd12018bf93dd26ad57ee5bbaf68b53ba8530dd5567431b80545c84862e0`。嵌入 `index-D9XCZvDX.js`，SHA-256 为 `1833abbbc97eafb08e2ef498576ed8c8924b3cc716d371f6f398375863c76ca9`；1430 预览返回相同字节。
- [源码前后哈希及改动清单](evidence/implementation/native-tls/changed-files.json)、[构建身份](evidence/implementation/native-tls/fixed-build/identity.json)及[预览核对](evidence/implementation/native-tls/preview-final.json)已归档。没有将修复前的首次配置流程说成在新构建重新执行。

## 实际桌面结果

| 场景 | 观测 |
| --- | --- |
| 首次 HTTPS 测试与启用（修复前构建） | 认证 CONNECT 成功，实际协商 TLS 1.3；测试仅发送固定探测且不创建配置。域名同意后启用，配置权限为 `0600`。 |
| 不受信任 CA、证书域名不匹配 | 修复前两者均为泛化错误；修复后英文与中文均提示证书验证失败及检查方法。TLS 服务记录证书拒绝，模型 HTTP 层未收到探测正文。 |
| 代理拒绝认证 407 | 两种语言均提示检查代理用户名和密码；按钮恢复可用，不误报为模型 API Key 错误。没有建立 TLS 或发送模型正文。 |
| 代理返回 502、直接断开 CONNECT | 两种语言均提示检查代理设置及可达性；该传输层未保留具体 HTTP 状态，界面不编造 502 原因。恢复代理后同一表单测试成功。 |
| 已保存配置下点击 Test | 12 个最终中英文测试结果均保留同一配置哈希；测试、放弃草稿及冷启动结束后，保存内容与 `0600` 权限仍不变。 |
| localhost HTTPS 绕过故障代理 | 最终应用的 `NO_PROXY/no_proxy` 均为空，代理处于拒绝认证状态；草稿改为本机 TLS 地址后仍能直接测试成功。随后明确放弃草稿，原远程配置保持。 |
| 代理故障期间输入、随后重试 | 原话保存为失败并显示重试入口；恢复连接后更新原句 ID，未新增重复句子或白板节点。修复前为第 2 句，最终中文构建再次验证第 3 句，revision 分别为 2→3、4→5。 |
| 正常退出及冷启动 | 三次原生菜单退出的应用退出码均为 0。最终冷启动恢复“已保存”和三句原话，只发送固定连接探测；会议文件在退出、重开及再次退出期间逐字节不变。 |

完整[请求元数据](evidence/implementation/native-tls/events.json)有 14 次模型 HTTP 请求，其中 2 次为 curl 预检、12 次来自原生应用。原生请求仅 3 次含合成会议标记，对应首次成功发送和两次显式重试，其余 9 次均为固定探测。模型接口没有收到 `Proxy-Authorization`，CONNECT 没有携带模型 Bearer 凭据。失败阶段没有模型 HTTP 请求，不将代理握手计作内容已上传。

可对照[旧提示](evidence/implementation/native-tls/native-test-proxy-auth.png)、[新英文代理提示](evidence/implementation/native-tls/fixed-en-proxy-auth.png)、[中文证书提示](evidence/implementation/native-tls/fixed-zh-untrusted.png)与[最终冷启动](evidence/implementation/native-tls/fixed-final-cold-start.png)。部分截图的黑色形状为私有显示上的 Cua 光标覆盖层，保留原图。

## 回归与隔离

- 新增 IPC 错误翻译回归先失败后通过，检查中英文、未知错误原样保留及正常成功结果。最终[312 项 Node/Eval](evidence/implementation/native-tls/node-tests.txt)通过，无跳过。
- [Rust 远程测试](evidence/implementation/native-tls/rust-remote-tests.txt)8 项通过，包含实际 CONNECT 407/502/断开和 30 秒响应正文超时；需要真实模型服务的 1 项仍跳过。
- 同一生产构建的[连接错误 30 项](evidence/implementation/native-tls/browser/check-remote-errors/remote-errors-check.json)及[隐私 31 项](evidence/implementation/native-tls/browser-fixed/check-remote-privacy/remote-privacy-check.json)通过。隐私检查首次使用的通用启动器缺少文档要求的测试域名映射，失败截图和记录保留；补齐仅作用于测试 Chromium 的映射、禁用该浏览器代理后重跑通过。其他脚本断言/采集路径修正见[运行说明](evidence/implementation/native-tls/harness-notes.json)，未因此重复提交原话。
- 类型检查、完整前端构建、Linux locked/offline debug 构建及 `git diff --check` 通过。构建仍有既有的大 chunk 提示。

运行目录 `/home/hsiangnianian/.cache/boardcast-native-tls-mianf1pr` 使用私有 Xvfb/Openbox、D-Bus、AT-SPI、XDG 数据和 Cua socket；禁用物理音频。证书信任与代理变量只传给这组测试进程，未安装 CA、修改系统 DNS 或系统代理。CONNECT 仅允许 `boardcast-tls.test:443`，内部只转发到本机 TLS 服务；应用自身尝试访问的 PostHog/GitHub 目标均被该代理拒绝，不能将其算作模型请求或公网验证。

私有应用/桌面进程已退出，TLS 和代理两个端口关闭，见[清理核对](evidence/implementation/native-tls/final-cleanup.json)。1423/1430 原预览进程保留。归档包含 246 个文件、10,026,161 字节及[哈希清单](evidence/implementation/native-tls/manifest.json)，不包含证书私钥、二进制或完整构建目录。`python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/native-tls/check-evidence.py` 只核对已捕获证据，不重新操作桌面。

## 剩余边界

本批验证 Linux 原生传输和受控响应，不代表真实供应商推理、本地模型成功回退、Mac 证书/系统代理或真实音频已通过。用户已说明暂时没有测试 Mac；G3 实机与 G4 用户验收继续保留，完整 C01–C68/G1–G4 未关闭。没有安装应用、提交 Git、推送或发布。
