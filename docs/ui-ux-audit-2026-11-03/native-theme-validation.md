# C60 Linux 原生系统主题与偏好恢复

2026-11-05，基线 `4f9ab26`。本批补充 [外观与笔迹验收](appearance-validation.md) 中尚未执行的 Linux 系统主题路径，没有修改产品源码。复用启动保存续批的最终 Tauri 二进制，运行于私有 X11、D-Bus 和配置目录；使用本机已安装的 GTK Settings portal 与 `gsettings` 改变系统外观，没有模拟浏览器的媒体查询。

后续 [归档打开保存状态调查](archive-open-save-validation.md)已确认并修复首次打开时的 Unsaved 提示竞态：旧 render 误改新会议的保存状态。该提示与退出时补齐历史是两件事；下文保留本批当时的原始观察及主题检查基准。

[32 项证据核对](evidence/implementation/native-theme/native-theme-check.json)通过，包含 18 个实际窗口状态。系统偏好、portal 通知、无障碍标签、窗口截图和会议文件哈希分别保留。

| 验证 | 实际结果 |
| --- | --- |
| 跟随系统 | 实际 `prefer-light → prefer-dark → default` 及反向切换生效，工作台、原话栏与手绘板同步更新。[系统深色](evidence/implementation/native-theme/final-hand-drawn-dark.png)、[系统浅色](evidence/implementation/native-theme/final-hand-drawn-light.png)。 |
| 手动覆盖 | 系统深色时选择浅色，系统浅色时选择深色，工作台和白板保持用户选择；切回 System 后恢复自动跟随。[浅色覆盖](evidence/implementation/native-theme/verified-light-on-dark.png)、[深色覆盖](evidence/implementation/native-theme/verified-dark-on-light.png)。GTK 原生菜单栏继续跟随系统设置，应用内选择控制网页工作台与白板。 |
| 自由编辑 | 通过实际界面切换到 Free edit，确认 Rectangle 等工具已加载，再切换系统深浅色；画布和工具栏均更新。[浅色画布](evidence/implementation/native-theme/verified-free-system-light.png)、[深色画布](evidence/implementation/native-theme/verified-free-system-dark.png)。自由编辑沿用 Excalidraw 的深色滤镜，纸色与手绘板不同，未要求两者像素相同。 |
| 冷启动 | 正常退出后启动不同进程，分别验证 [System/系统深色](evidence/implementation/native-theme/cold-system-dark.png)、[Light/系统深色](evidence/implementation/native-theme/cold-light-on-dark.png)、[Dark/系统浅色](evidence/implementation/native-theme/cold-dark-on-light.png)。主题选择、会议和画布模式均恢复。 |
| 内容与保存状态 | 正式检查的 18 个状态均显示 Saved，会议 SHA-256 均为 `63b33523213f923d52e2d2e8112f85e842ed1377d8902dffc38fd14f0cc923c6`；后续三次正常退出后文件仍逐字节相同。[最终会议](evidence/implementation/native-theme/after-fourth-quit-meeting.json)。 |

系统服务的 [SettingChanged 实录](evidence/implementation/native-theme/second-run/portal-monitor.txt)包含 `org.freedesktop.appearance/color-scheme` 的 `0/1/2`，分别对应无偏好、深色和浅色；含义见 [XDG Settings 官方协议](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)。本批使用 GTK 3.24.52、WebKitGTK 2.52.6，具体软件包版本与二进制哈希见 [identity.json](evidence/implementation/native-theme/identity.json)。不据此扩大为其他桌面 portal、Wayland 或 Mac 的验收。

## 样本准备与证据边界

输入复用此前生产浏览器的中英日混合 [合成会议](evidence/implementation/native-theme/fixture.json)。该样本有原话而 `history` 为空，首次从会议库打开时显示 Unsaved changes；退出时文件补入一条上下文历史、布局和缩略图，修订号由 1 变为 2，图、原话、底板及回放步骤未变。因此，首次运行仅作为样本准备，不能声称首次打开至退出全程没有改写文件。之后冷启动与再次从会议库打开均显示 Saved；以 [首次正常退出文件](evidence/implementation/native-theme/after-first-quit-meeting.json) 为正式主题检查基准。

检查脚本最初缺少 PIL，改用已有 GdkPixbuf；Free edit 的首次后台点击未生效，随后用私有桌面的前台输入并核对工具栏；纸色检查也按源码分别处理手绘 `#fbfbf8` 与自由编辑 `#fbfbf9`。这些自动化修正见 [记录](evidence/implementation/native-theme/harness-notes.txt)，不作为产品缺陷或通过次数。截图中的黑色形状是 Cua 光标标记。

四次原生应用退出码均为 0，私有进程与显示已清理。[清理记录](evidence/implementation/native-theme/cleanup-final.json)同时保留一项限制：宿主的主题偏好在前后均为 `prefer-dark`，但整个宿主 dconf 文件的哈希在期间发生变化，原因未确定，不能宣称宿主全部配置逐字节未变。所有显式主题写入均指定私有 D-Bus 和配置目录，该目录有独立 dconf 数据库。

本批二进制 SHA-256 为 `2b2962a2910c5dc8dc5325e7ea52836a610746ecb796c4925794fd04d38ec3aa`，复用已有构建，未重复构建或运行完整 Node/Rust 测试。1430 的实际响应与原生冻结前端相同，1423/1430 均 HTTP 200，见 [预览身份](evidence/implementation/native-theme/preview-identity.json)。[源码哈希](evidence/implementation/native-theme/source-final.json)、[核对脚本](evidence/implementation/native-theme/check-evidence.py)及[归档清单](evidence/implementation/native-theme/manifest.json)一并保留。

未重新验证导出、真实音频或模型；没有测试 Mac，VoiceOver、原生 Wayland、其他 G2/G3 与 G4 用户试用仍保留。完整重构未完成，未提交、推送或发布。
