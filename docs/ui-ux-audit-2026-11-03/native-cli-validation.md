# C14/B0 续批：CLI 冷启动与会议恢复

2026-11-04，HEAD `4f9ab26`，未提交。`boardcast --toggle-mic` 在应用关闭时也会处理收音请求，先完成会议恢复；缺模型时显示准备入口。启动期间收到两次切换请求会相互抵消，退出会清除尚未处理的收音请求。

## 修复与证据

旧程序仅在单实例插件接到第二次启动时读取 `--toggle-mic`。私有 Linux 桌面上，带参数首次启动仍停在[普通空工作台](evidence/implementation/native-cli/baseline/first-launch.png)，再次执行同一命令才打开[语音准备页](evidence/implementation/native-cli/baseline/second-launch.png)。原生二进制、参数及测试启动器已归档，未打开真实音频设备。

现在首次启动也进入现有 `shell::toggle`。原生暂存切换意图，前端在会议恢复完成、事件订阅成功后，通过 `shell_take_toggle` 领取一次；事件与启动检查同时到达不会执行两遍。快捷键录入焦点保护、失败时显示窗口及原有收音路径继续复用。两个尚未领取的切换请求抵消，不新增命令参数或依赖；中英文 README 同步说明。

补充状态测试复现了一个退出边界：先收到收音请求，再请求退出，若退出随后被取消，尚未领取的请求会再次生效。见[修复前失败](evidence/implementation/native-cli/build/exit-race-before.txt)。现将入队、领取和退出检查放在同一锁下；收到退出请求即清除待处理收音请求，退出期间不接收新收音请求。取消退出后必须重新发起收音。最终[4 项 Rust 状态测试](evidence/implementation/native-cli/build/rust-tests.txt)通过，包含原有退出去重和保存失败重试。

## 实际运行范围

- 首次 CLI 启动直接显示语音模型准备页；普通无参数启动恢复旧会议并保持未收音。
- 实际输入并保存 `CLI_RESTORED 首次启动先恢复这句记录` 后，正常退出再带参数冷启动。旧原话和已保存状态仍在，准备页正常显示；第二实例 CLI 和最小化后的全局快捷键也能打开恢复入口。
- 最后的退出状态修正重新构建了原生程序，再次完成[旧会冷启动、准备入口和正常退出](evidence/implementation/native-cli/final/first-launch.png)。前后会议 JSON 字节及 revision 1 完全相同。

生产浏览器使用真实 App 和受控 IPC，验证延迟会议恢复期间不启动采集、恢复后只执行一次、第一句新转写追加到旧会、启动期间早到请求、两次请求抵消、监听安装与启动领取竞争，以及已有退出请求阻止采集。原有快捷键冲突、输入焦点、保存失败和停止失败保护继续通过，见[最终协议结果](evidence/implementation/native-cli/build/browser-final/native-shell-check.json)。其中成功收音和新转写来自 fixture，不是实际麦克风识别。

调用方 `probe-hold-focus.cjs` 同步新协议后，在独立 Xvfb 的可见 Chromium 重新执行，实际页面切换产生失焦/隐藏事件；[21 项检查](evidence/implementation/native-cli/build/hold-focus/check.json)全通过，没有打开音频设备。没有用 headless 强制焦点替代真实页面失焦。

[20 项归档核对](evidence/implementation/native-cli/native-cli-check.json)通过。[复核脚本](evidence/implementation/native-cli/verify-native-cli.py)只读取捕获的 UI、文件、测试及清理结果，不模拟原生行为。类型检查、生产前端构建、最终原生 debug 构建、Rust 格式和 diff 空白检查通过。

## 构建与剩余边界

完整原生首次启动/普通启动/第二实例/后台快捷键路径在收尾修正前构建执行；其[独立身份](evidence/implementation/native-cli/build/identity-before-exit-race.json)保留。最终构建补上退出取消状态回归，并重新执行上述冷启动、旧文件保留和退出；[最终身份](evidence/implementation/native-cli/build/identity.json)中的原生 SHA-256 为 `5260f88f090409f34fb9a412ebc2f7d26e0d8e464a344718635933444df66c50`。两个阶段前端产物相同。

1430 已更新并保留旧哈希资源，1423/1430 均通过[新浏览器上下文检查](evidence/implementation/native-cli/build/preview-check.json)。原生测试进程正常退出，可见测试 Chromium、私有显示服务和1438测试预览已清理；用户两个预览服务保留。归档有 [SHA-256 清单](evidence/implementation/native-cli/manifest.json)。未提交、推送、安装或发布。

启动迟到事件、两次抵消及退出取消的精确竞争分别由生产协议 fixture 和真实 Rust 状态测试证明，未声称已在原生音频硬件上复现所有竞争。Wayland 桌面绑定、真实收音、Mac、VoiceOver、签名安装更新及 G4 用户试用仍待验证；用户已明确暂时没有测试 Mac，整体 C01–C68/G1–G4 目标继续。
