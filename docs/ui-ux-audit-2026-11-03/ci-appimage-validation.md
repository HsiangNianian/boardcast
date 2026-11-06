# CI AppImage 的真实启动、生成与会议恢复

2026-11-06，直接运行产品提交 `c97846e` 的 GitHub Actions Linux AppImage，没有重新构建或替换包内程序。下载完整 ZIP 后核对大小和 GitHub 提供的 SHA-256，再提取 AppImage。独立 Xvfb、D-Bus、XDG 数据与 Cua 端点隔离测试；模型为用户已有 Qwen3.5-4B-Q4_K_M 的独立副本，不打开物理音频。[14 项证据核对](evidence/implementation/ci-appimage/evidence-check.json)通过。

## 产物身份

- [CI 运行](https://github.com/HsiangNianian/boardcast/actions/runs/42188601525)，`boardcast-linux-x64` artifact `13488434290`；ZIP 为 879,021,236 字节，SHA-256 `7f5ef1d294402661fe767e019d38f91c4f4c46d05efc67a5df390fe5bd28d2ed`。
- `appimage/boardcast_0.21.0_amd64.AppImage` 的 SHA-256 为 `8f0cbf2d752eb7d967eb773b8ce4cb90ab857088a3ffe0cbc418ccd8ee35fc4f`。
- 两次进程均来自该 AppImage 的不同临时挂载，包内可执行文件 SHA-256 均为 `b20b3924f46f15d0ff1789d4e8295da39212a169a2d786b8533943da9f3b5cd7`。随包 llama-server、SenseVoice、tokens、VAD 和 CAM++ 均存在，语音模型两次预加载成功。

## 实际结果

| 路径 | 证据 |
| --- | --- |
| 首次启动和预热 | 窗口依次显示启动、预热、就绪。模型日志包含完整预热推理；实际使用包内 Vulkan 引擎与 NVIDIA RTX 3060 Laptop GPU，未将健康检查代替预热成功。 |
| 输入到板书 | 在原生窗口粘贴并提交一句合成流程，原话先保存；随后保存 4 个节点、3 条连线，来源为 Built-in，无远程回退。界面显示板书完成与已保存。 |
| 摘要到复盘 | 实际本地生成 Alice/Bob/Carol 三项待办，系统剪贴板读回与保存摘要逐字一致；确认结束后进入复盘，显示 1 条原话、1 张板、0 个未完成项。 |
| 重新启动 | 旧应用、模型和挂载消失后启动同一包，恢复原话、图形、摘要与已保存状态，第二次实际预热完成。再次复制摘要逐字一致，两份会议/锁文件哈希保持。 |
| 最终退出 | 原生 Quit 后先观察到窗口消失与进程结束，再清理启动器；退出码为 0，模型、私有桌面进程和 AppImage 挂载均消失。用户模型原件哈希、大小、inode、mtime 及原 1423/1430 预览保持。 |

模型加载在两次运行约为 89/79 秒，预热推理约为 37.1/3.86 秒；这些是本机日志中的观测，未形成跨设备性能承诺。[首次生成](evidence/implementation/ci-appimage/generated-view.png)、[复盘](evidence/implementation/ci-appimage/review-ready.png)、[冷启动就绪](evidence/implementation/ci-appimage/cold-ready.png)及[完整产物身份](evidence/implementation/ci-appimage/artifact.json)可在本地查看。104 个归档文件共 3,682,150 字节，另有 SHA-256 清单；不包含安装包、模型或用户原始诊断。

## 测试调整与边界

第一次发送 Quit 后，测试启动器清理过早，应用实际以 -15 结束；不将这次记为正常退出。记录在 [退出核对](evidence/implementation/ci-appimage/first-quit-proof.json)，正常退出由最后一次独立验证。下载工具未采用预期续传文件名，实际进行了四路重新下载；旧片段保留，最终完整 ZIP 大小与摘要均通过核对。

这是 CI AppImage 的直接运行与应用冷启动，未执行系统包管理器安装、正式签名的原位升级或跨版本数据迁移。语音仅验证随包资源预加载，没有验证物理录音。原 Adreno 设备、Mac/VoiceOver、实际升级与完整 G3/G4 继续保留。Gtk 菜单加速键启动警告仍出现在日志，未宣称消除所有底层警告。
