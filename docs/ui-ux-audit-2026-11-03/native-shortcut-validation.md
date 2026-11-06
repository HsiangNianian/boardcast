# C14/C61/C62 续批：全局快捷键冲突与恢复

2026-11-04，HEAD `4f9ab26`，未提交。真实 Linux X11 注册冲突下，新组合失败会保留旧组合及其设置；解除占用后可直接按同一组合重试。启动失败明确显示当前没有有效快捷键，主动关闭后立即释放系统占用。

## 修复

- `shell.rs` 先解析、注册新组合，成功后才移除旧组合；相同组合直接返回。移除旧组合失败时回滚新组合，错误继续返回前端。
- `App.tsx` 按顺序提交快捷键修改，只在系统确认成功后保存偏好。较早的响应不会覆盖较新的选择；末次失败显示最后成功的组合。
- 设置增加处理中状态、关联错误说明与中英恢复提示。出错时滚动到提示，保留录入焦点；长按重复事件不再重复提交。
- `global-hotkey` 0.8.0 的 X11 清理原先只排队发送释放请求，没有等待完成。真实失败后，部分按键仍被占用，导致同一组合无法录入重试；关闭快捷键也会延迟释放。现通过检查释放请求完成清理。使用保留许可证的本地依赖补丁，其他平台源码不变；来源、最小修改和移除条件见 [补丁说明](../../src-tauri/vendor/global-hotkey/BOARDCAST-PATCH.md)。

## 真实前后对照

占用程序只在私有 Xvfb 上持有 Ctrl+Shift+K 的 NumLock 变体，不注入输入。NumLock 关闭时，实际按键仍能进入设置；库注册全部锁定键变体时收到真实 X11 BadAccess。所有窗口和键盘操作通过 Cua，偏好值从私有 WebKit SQLite 只读核验。

原始构建在失败后注销了旧 Ctrl+Shift+Space，并保存了失败的 Ctrl+Shift+K，见 [旧注册记录](evidence/implementation/native-shortcut/baseline/shortcut-occupancy.json)与 [旧偏好值](evidence/implementation/native-shortcut/baseline/stored-shortcut-after-failure.json)。第一阶段修复保住旧组合，但原生重试仍失败：另一个 X11 客户端确认普通 K 组合仍被占用，而冲突程序的 NumLock 组合已释放，见 [残留占用](evidence/implementation/native-shortcut/intermediate/probe-k-after-release.json)。

依赖内新增一个真实 X11 回归，覆盖失败后的部分清理、旧绑定保留、解除冲突后重试、替换与关闭后的立即释放。它在补丁前[明确失败](evidence/implementation/native-shortcut/build/x11-before.txt)，补丁后与原有两个解析测试一起 [3 项通过、0 跳过](evidence/implementation/native-shortcut/build/x11-after.txt)。常规无图形环境默认跳过这项硬件环境测试；复跑命令见补丁说明。

最终桌面构建核验：

- 冲突后输入框保持 Ctrl+Shift+Space，实际偏好也保持原值；最小化后旧组合仍[恢复窗口并打开模型准备页](evidence/implementation/native-shortcut/final/old-shortcut-desktop.png)。
- 解除占用后直接按 Ctrl+Shift+K，[提示消失、字段更新](evidence/implementation/native-shortcut/final/retry-result.png)，偏好成功保存；旧组合可被其他 X11 客户端注册，新组合从后台唤起窗口。
- 正常退出后，在同一私有配置下先占用 K 再启动新进程。设置显示 [Off 与没有有效快捷键的提示](evidence/implementation/native-shortcut/final/startup-conflict.png)，之前保存的 K 偏好仍保留。解除占用后，同一输入框重试成功。
- Backspace 关闭后，偏好变为空，其他客户端立即可以注册全部四种锁定键变体。再次 Backspace 不报错；最小化后 K 不再显示窗口。
- 中文错误主体随语言切换；原始系统错误作为详情保留。中文截图来自第一阶段原生构建，其前端与最终构建相同，见 [中文提示](evidence/implementation/native-shortcut/intermediate/conflict-zh-visible.png)。

[23 项证据核对](evidence/implementation/native-shortcut/native-shortcut-check.json)通过。[复核脚本](evidence/implementation/native-shortcut/verify-native-shortcut.py)只检查归档结果；字段文字与启动失败提示另由截图人工核对，不把截图复核称为自动 UI 测试。

## 构建、回归与预览

[生产协议回归](evidence/implementation/native-shortcut/build/browser/native-shell-check.json)通过：失败保留旧值、同组合重试、中英提示切换、重复按键、连续修改排队、后一次失败回退到前一次成功、修改后排队关闭、启动失败及显式关闭。原有收音焦点保护、退出保存屏障、停止失败、画布撤销重做等检查继续通过。连续请求与成功收音使用受控 IPC fixture，不能当作实际音频证据。

本轮类型检查、9 项 Node 快捷键/语言测试、前端生产构建、最终原生 debug 构建、Rust 格式和 diff 空白检查通过。最后只修改 Rust 依赖补丁，因此前端回归使用同一份已通过的生产产物。原生二进制和源码哈希见 [构建身份](evidence/implementation/native-shortcut/build/identity.json)，原生 SHA-256 为 `d7eb548414c2d0ffdb6e683abe1ac4c941554df60f1f64f2769218ae976e6eb1`。

1430 已更新，保留旧哈希资源、最后原子替换入口；1423/1430 均通过[新的浏览器上下文检查](evidence/implementation/native-shortcut/build/preview-check.json)。私有原生进程、占用程序和显示服务已清理，1438 测试预览关闭，用户的两个预览服务保留。原始基线进程因测试会话结束而受控终止；中间和最终应用正常退出，不把基线清理算作正常退出验收。归档附 [SHA-256 清单](evidence/implementation/native-shortcut/manifest.json)。

## 剩余边界

未执行旧绑定注销失败及回滚再失败的系统故障注入，也未穷举所有键盘布局、锁定键和组合。依赖补丁需要在升级时保留此回归，符合上游修复后再移除。

首实例 `--toggle-mic` 已由后续 [CLI冷启动专批](native-cli-validation.md)接续。原生 Wayland、真实音频与录音状态、其他托盘宿主仍待核验。用户明确暂时没有测试 Mac；Mac、VoiceOver、签名安装更新、G3/G4 和完整 C01–C68 验收继续保留。未提交、推送、安装或发布。
