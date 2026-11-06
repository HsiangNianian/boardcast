# C61/C62/C66 续批：Linux 原生菜单与退出恢复

2026-11-04，HEAD `4f9ab26`。本批使用当前工作树构建的真实 Tauri 应用，在独立 Xvfb、Openbox、D-Bus 和 AT-SPI registry 中操作原生菜单、页面控件和实际会议文件。本批没有修改产品代码；24 份原生源文件与构建配置和工作树一致，嵌入的前端也与 1430 预览逐字节一致。

## 实际操作与结果

[35 项证据核对](evidence/implementation/native-menu/native-menu-check.json)全部通过。Cua 的菜单动作采用原生可访问接口；WebKit 页面输入采用隔离桌面中的真实前台事件。工具返回 `effect: unverifiable` 的动作，均另读窗口状态、可访问树、截图或实际文件确认结果。归档脚本只核对这些存档，不重新驱动应用。

- 从原生 `boardcast → Settings…` 打开设置，通过页面切换中英文，原生六组菜单随之更新。设置打开时，原生帮助命令保持模态边界；关闭设置后，中文原生帮助菜单能打开快捷键，Escape 能关闭。语言切换后 Ctrl+, 仍可打开设置。
- 原生导入命令打开真实 GTK 文件选择器，取消后只留下主窗口。文件选择器使用隔离环境的系统语言英文。中文、英文会议库入口都能打开实际会议库。
- 在输入框输入 `Native menu audit draft`，原生 Edit → Undo 清空草稿，Redo 恢复。已查看[输入](evidence/implementation/native-menu/draft.png)、[撤销](evidence/implementation/native-menu/draft-undo.png)、[重做](evidence/implementation/native-menu/draft-redo.png)截图；提交后，真实原生会议文件仅保存一次该句，板书未配置的失败状态没有丢掉原话。
- 原生 Export 打开应用的范围和格式选项；原生 Diagnostics 打开实际诊断预览面板。Escape 可以分别关闭。本批只验证这些入口和取消，不重复认定完整导出或诊断 ZIP 已在这里重测。
- 原生 New Meeting 打开会前准备；关闭准备后是新的空会议。再次从原生会议库选择旧会议，可以恢复原话和失败状态，原始会议文件仍在。
- Window → Zoom 实际最大化到 1440×979，再次选择恢复 1280×800。View → Toggle Full Screen 设置真实 X11 全屏标志并占满 1440×1000，再次选择解除标志、还原窗口。尺寸和 `_NET_WM_STATE` 均有读回记录。
- 选回中文后，从原生菜单退出，窗口消失、应用退出码为 0。完整关闭隔离桌面后冷启动同一二进制，中文菜单、所选会议和原话自动恢复；前后原话数组与记录 ID 一致，仍只有一条记录。冷启动后使用原生“关闭窗口”，Linux 应用同样正常退出。

已查看[中文设置与原生菜单](evidence/implementation/native-menu/zh-settings.png)和[冷启动恢复](evidence/implementation/native-menu/cold-reopened.png)。两轮隔离桌面结束后，测试进程、可访问服务、D-Bus、X11 socket 均已清理，用户原有的 1423/1430 预览进程保留。

## 构建与复核

应用标识 `games.mmstudio.boardcast.g2.nativemenu`，debug 构建、不打包；[配置](evidence/implementation/native-menu/native-build-config.json)、[原生构建日志](evidence/implementation/native-menu/native-build.txt)、[24 份源文件哈希](evidence/implementation/native-menu/native-source-check.json)、[二进制与前端身份](evidence/implementation/native-menu/identity.json)和[预览核对](evidence/implementation/native-menu/preview-check.json)已保存。实际运行步骤、隔离启动脚本和 Cua 调用结果一并归档，共 94 个产物、1,762,136 字节，另有[哈希清单](evidence/implementation/native-menu/manifest.json)。

运行日志保留了 GTK 菜单/accelerator 提示、appindicator 弃用提示，以及空 ALSA 配置下的设备枚举错误。本批结论来自上述实际操作和读回结果，不代表日志已无告警。没有升级系统库或 Cua，也没有重跑无关的 Node/Rust 全量测试。

## 仍未验证的范围

用户已确认暂时没有测试 Mac；当前连接的 Cua Space 也只有暂停中的 Linux 环境，本批未使用该 Space。Mac 菜单、Dock、VoiceOver、真实输入法、真实音频与 60 分钟现场会、签名更新安装和 G4 用户试用继续保留。Linux 托盘、原生剪贴板和其余快捷键组合也不能由本次结果代替。

后续的[原生剪贴板专批](native-clipboard-validation.md)已经修复并验证 Linux X11 文本、标题弹窗及两对象画布的剪切/复制/粘贴与全选；其余平台、格式和组合仍按专批边界保留。

退出检查发生在未收音、已保存的状态；正在收音时的尾句和保存失败恢复仍应依据各自的专门证据。本批没有提交、推送或发布，整体重构目标保持进行中。
