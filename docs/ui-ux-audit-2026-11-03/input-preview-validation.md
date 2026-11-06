# 显式输入电平预览

2026-11-03，方案 §5.1 的“检查输入”。打开会前面板仍只读取能力；用户点击检查按钮后才打开所选音源。检查不要求转写模型，不调用 Engine/Apple SpeechAnalyzer，不发送 `voice-partial`、`voice-final` 或会议录音状态，不保存音频或原话。

`voice.inputCheck.start(id, device, source)` 与 `stop(id?)` 使用独立请求 ID；`voice-input-check` 事件带 ID、starting/active/stopped/error、真实设备名及可选 RMS 读数。未收到样本时不显示 0；真实静音样本可以显示 0。原生线程最多运行 10 秒，取消、到期、错误会释放 CPAL stream 和系统声音 helper。停止指定旧 ID 不影响新请求；提前到达的取消也会被记住。正式收音、停止/卸载以及 App 切会/复盘的转换锁会清理预览。

Preparation 在关闭、先打字、管理模型与正式开始之前等待当前预览释放；卸载时也发送取消。等待预览释放期间取消正式开始，或面板已卸载，都不会在释放完成后继续调用正式收音。活跃检查期间不切换来源。系统声音权限失败保留所选来源并显示原有系统权限说明。所有文字已提供中英文；网页不提供原生采集按钮。

macOS 的系统声音仍复用独立 `capture-system` helper。Ready 等待现在可被预览的取消标志打断，失败/超时会回收 helper。普通录音的既有 25 秒 Ready 上限保留；SenseVoice、动态复查、VAD、Apple Auto 选择和正式录音音频策略没有调整。

已通过的证据：

- TypeScript 编译检查；修复生产编译问题后，非测试的 `cargo check --manifest-path src-tauri/Cargo.toml --lib` 和 `cargo build --manifest-path src-tauri/Cargo.toml --lib` 均通过。后者实际编译并链接 Linux 库，原始日志见 [native-production-build.txt](evidence/implementation/native-production-build.txt)。这不代表 Tauri 打包、原生应用运行或 macOS 构建已通过。
- `voice::preview::tests` 的两个普通测试：未知读数与实测静音区别、RMS、旧请求取消和有界取消记录。生产编译修复后再次通过，虚拟输入项仍按默认配置 ignored；日志见 [native-preview-tests.txt](evidence/implementation/native-preview-tests.txt)。
- `apple_speech::tests::system_capture_startup_cancel_and_timeout_reap_an_unready_helper`：使用不发送 Ready 的合成子进程，取消和超时均在一秒内返回并回收子进程。它验证 Rust 等待/回收逻辑，不代表已验证 macOS 权限对话框。
- 实际 Linux CPAL 输入测试 `virtual_input_meter_receives_samples_and_releases_capture`：显式创建 `BoardcastPreviewQA` null sink，只打开其 monitor，将 440 Hz 合成 WAV 送入该虚拟 sink，收到非零 RMS；释放 stream 后接收通道断开。测试没有使用默认或实体麦克风，也没有向真实扬声器播放。虚拟模块 `536870916` 已卸载，随后确认 QA sink 不在设备列表中。
- `scripts/ui/check-input-preview.cjs`：真实 Preparation 组件配合受控 IPC，覆盖缺模型仍可检查、无隐式采集、未知/静音/有声读数、显式停止、系统权限失败保持来源、到期、旧事件隔离、关闭等待释放、启动期间 Escape、先打字和正式收音前释放。结果见 [input-preview-check.json](evidence/implementation/input-preview-check.json)。原有 `check-preparation.cjs` 也再次通过。
- 面板截图：[实际组件的受控电平](evidence/implementation/input-preview-active.png)、[720×480 的停止检查与关闭入口](evidence/implementation/input-preview-720.png)。截图读数由测试 IPC 提供，真实 CPAL 信号验证是上面的独立虚拟音源测试。
- 最终生产 App 的受控 native bridge 检查确认：仅预览、未正式收音时请求关闭，`voice_input_check_stop` 仍先于退出确认。见 [native-shell-check.json](evidence/implementation/native-shell-check.json) 的 `previewStoppedBeforeExit`。
- 同伴只读复审发现并修复了“正式开始正在等待预览释放，此时取消仍会继续开始”的竞态。新增 held-stop 真实组件检查确认取消后 `onStart` 调用次数不增长，后续正常开始仍能成功；证据字段 `cancelBeforeRecording: true`。仅在浏览器响应中临时移除取消 guard 时，该回归准确失败（实际启动 1 次、预期 0 次），工作树保护保留。已结束请求的同 ID 迟到电平也不能重新激活 UI。

此前 `cargo test --lib` 的 50 项通过仅覆盖测试配置：预览曾调用只在 `cfg(test)` 下存在的 `Mixer::pop`，因此未能发现生产配置中的 E0599。B0 的独立生产构建发现后，已改为正式路径使用的 `pop_tracks` 加 `mix`，并完成上面单列的非测试编译与链接。不能把原有单测通过解释为当时的生产构建通过。

Linux 虚拟输入测试默认 ignored，必须显式提供隔离设备和 WAV：

```sh
BOARDCAST_PREVIEW_TEST_DEVICE='Monitor of BoardcastPreviewQA' \
BOARDCAST_PREVIEW_TEST_WAV=/tmp/boardcast-preview-synthetic.wav \
cargo test --manifest-path src-tauri/Cargo.toml virtual_input_meter --lib -- --ignored --nocapture
```

可复现脚本 `bash scripts/ui/check-virtual-input.sh` 会创建专用 null sink、生成合成 WAV、运行该测试并通过退出 trap 清理，已有同名设备时会拒绝运行。测试允许清空 CPAL 停止时已排队的最后一段回调，要求一秒内通道断开；播放进程先退出再检查断言，避免失败清理时把未结束播放转移到其他输出。结果：[virtual-input-check.txt](evidence/implementation/virtual-input-check.txt)。

不得将该测试改为默认麦克风。完整的原生 IPC→预览线程→面板链路、macOS 实体权限/采集、Windows loopback、硬件拔插和真实系统声音 helper 仍需 G2/G3；本次并未把浏览器模拟或 Linux 合成音测试表述为这些平台已通过。

2026-11-05 续批：[Linux 原生准备页](native-preparation-validation.md)已补齐所选合成麦克风的真实 IPC→采集流→面板电平链路，以及停止、关闭、先打字、退出和冷启动。上述系统声音/双路、实体设备及其他平台边界继续保留。
