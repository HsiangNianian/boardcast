# Linux 原生会前输入检查

2026-11-05，接续 C02 和[显式输入预览](input-preview-validation.md)。本批补齐真实 Tauri IPC → CPAL 输入 → 面板电平的 Linux 链路，没有修改产品代码。

## 环境与证据

沿用[桌面示例批次](desktop-demos-validation.md)最终原生构建，SHA-256 为 `4b0143a5aea906b0dbe07043a3002696b65b240debbc769bab4190fe263488bd`；重新核对[二进制身份](evidence/implementation/native-preparation/identity.json)和[相关源码哈希](evidence/implementation/native-preparation/source-identity.json)。独立 XDG、D-Bus、Xvfb 和 Cua 窗口，模型未安装。

在实际 PipeWire/PulseAudio 服务创建专用 null sink，再将其 monitor 映射为可选的 `BoardcastPreparationQA` 输入；通过界面明确选择该输入和「麦克风」来源。440 Hz、48 kHz 的合成 WAV 只播放到该虚拟 sink。采集流的进程 ID 和 source index 与这台应用、该虚拟输入逐项匹配，未使用默认或实体麦克风。原系统默认设备不变。

[18 项证据核对通过](evidence/implementation/native-preparation/proof.json)：

| 任务 | 实际结果 |
| --- | --- |
| 启动、打开准备页 | 读取能力和列出设备时，应用没有采集流，见[准备页前后](evidence/implementation/native-preparation/preparation-no-capture.json)。缺少模型时仍可显式检查输入。 |
| 静音与有声 | 同一个实际输入从 `0.0` 变为 `0.1414400935` RMS，见[静音读数](evidence/implementation/native-preparation/silent-meter.json)、[有声窗口](evidence/implementation/native-preparation/tone-meter.png)和[实际采集流](evidence/implementation/native-preparation/tone-active.json)。 |
| 到期与停止 | 到期显示检查已结束且流消失；另一次在采集中点击停止，流消失并显示已停止。见[到期](evidence/implementation/native-preparation/auto-timeout.json)、[显式停止前](evidence/implementation/native-preparation/explicit-active.json)和[停止后](evidence/implementation/native-preparation/explicit-release.json)。 |
| 关闭与先打字 | 两个入口均从实际采集状态释放输入。「先打字」返回后直接通过窗口键入文字，无额外聚焦操作，见[焦点窗口](evidence/implementation/native-preparation/type-focus.png)。 |
| 原话保存 | 输入检查未生成会议；随后键入 `PREP_TYPED_20261105` 并提交，真实会议文件保留唯一原话，板书状态为模型不可用。见[保存证据](evidence/implementation/native-preparation/typed-proof.json)。 |
| 最小窗口与退出 | 720×480 原生窗口下，关闭、停止检查和先打字均可见且可用，见[窗口](evidence/implementation/native-preparation/compact-active.png)。采集中通过原生菜单退出，流由 1 个变为 0 个，见[退出前](evidence/implementation/native-preparation/before-quit-active.json)和[退出后](evidence/implementation/native-preparation/after-quit-release.json)。 |
| 冷启动 | 所选音源保持，未自动采集；从会议库重开并再次冷启动后，原话可见且文件字节不变，见[原话窗口](evidence/implementation/native-preparation/final-cold-transcript.png)和[文件核对](evidence/implementation/native-preparation/cold-proof.json)。 |

第一次停止尝试到达时已经自动到期，未把它算作显式停止成功；随后单独重测了仍有采集流时的停止。紧凑布局先从详情返回白板再打开准备页。冷启动保留了关闭侧栏的偏好，最终再次打开侧栏核对原话；这些操作说明见[运行记录](evidence/implementation/native-preparation/run-notes.json)。截图黑色指针来自 Cua 覆盖层，原图未修改。

三次运行的 21 个登记进程均退出，私有 X socket 已移除；播放结束后才移除两个专用音频模块，虚拟输入/输出均不再存在，默认音源未变。见[进程核对](evidence/implementation/native-preparation/process-proof.json)及[音频清理](evidence/implementation/native-preparation/audio-cleanup.json)。

## 保留边界

本批没有产品代码变动，没有重复构建或重跑已通过的单元/组件测试。只证明 Linux 所选合成麦克风的预览链路及相关返回、保存、退出行为；真实语音识别、系统声音/双路、物理设备热插拔、Mac/Windows 权限和 G3/G4 仍未完成。没有提交或发布。
