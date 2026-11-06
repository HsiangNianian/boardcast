# 会前准备验证

2026-11-03，当前工作树；对应方案 5.1 与状态链第 6 节的会前部分。

Preparation 使用现有 `voice.status`、`voice.models`、`voice.devices` 与 `voice-models-changed`。打开面板只读能力；不调用采集、资源下载或语音策略切换。SenseVoice 检查已安装资源；Apple 同时使用真实 availability、选中语种资源状态和既有 `appleLanguageState`，Auto 保留 SenseVoice 与三个识别语种资源依赖。`voice.status.installed` 对 Apple 只代表平台，不能独立作为就绪证据。

音源选定不等于已检测到声音；输入详情明确尚未测量，系统声音权限仅在对应来源下解释。选中的麦克风已不在设备列表时要求明确重选。主动作只在用户点击后调用 App 的 `onStart()`；成功才关闭，失败保持面板，支持取消启动。未准备板书仍可“先记录原话”，未准备转写可进入准备或先打字。网页不调用 native IPC，明确浏览器范围并允许文字输入。主动作与关闭固定可达，清单独立滚动。

已验证：

- TypeScript 检查通过。
- `scripts/ui/check-preparation.cjs` 通过。实际组件配合符合生产协议的模拟 capability，覆盖无隐式采集/下载、SenseVoice 就绪、缺板书仍记录、缺转写阻止收音、Apple 不支持/待下载/已安装/Auto 依赖、能力刷新、检测失败重试、设备消失、系统音源、启动失败/取消/成功、720×480 主动作始终可见、480px 网页仅文字路径。
- [1280×800 面板](evidence/implemented-preparation-1280.png)与 [720×480 窄窗](evidence/implemented-preparation-720.png)为实际组件＋模拟桌面能力截图。

运行方式见 [浏览器检查说明](../../scripts/ui/README.md)。这些初始截图不代表真实麦克风输入、macOS 权限或 Apple 资源下载已经验收；未更改 ASR 默认或音频策略。随后新增的显式短时输入电平预览及其独立验证见 [input-preview-validation.md](input-preview-validation.md)。
