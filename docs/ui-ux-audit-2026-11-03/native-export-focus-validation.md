# 原生导出返回焦点与紧凑顶栏

2026-11-05，接续 [紧凑顶栏](compact-header-validation.md)，覆盖 C04/C45/C67 的 Linux 桌面部分。真实 GTK「Save File」取消后，主窗口失去按钮焦点，按 Enter 无法重新打开「更多」；浏览器对照也记录到 `activeElement = BODY`。见[失败记录](evidence/implementation/native-export-focus/browser-baseline/export-handoff-check.json)和[原生取消后按 Enter](evidence/implementation/native-export-focus/native/baseline/save-cancel-enter.png)。

现在导出结束后的 React 提交阶段恢复焦点，目标是当前尺寸下存在的「更多」或「导出」。只在焦点已经丢到 body、没有打开的弹窗且仍是同一会议时恢复，保留正在输入的草稿及弹窗内的焦点。成功、取消和生成失败共用同一个收尾路径。产品只改 `src/App.tsx`，没有改保存格式、IPC、模型或新增依赖，见[产品补丁](evidence/implementation/native-export-focus/product.patch)。

| 检查 | 本批结果 |
| --- | --- |
| 生产浏览器交接 | [70 项通过](evidence/implementation/native-export-focus/regression/export-handoff/export-handoff-check.json)。新增紧凑/宽窗口取消、输入与弹窗焦点保持、完成前跨断点和生成失败检查；这些使用受控原生回复。 |
| 导出产物回归 | [92 项通过](evidence/implementation/native-export-focus/regression/exports/export-check.json)，实际生成七种格式，检查 PDF 页、图像与矢量内容、动画及断网离线回放。 |
| 原生保存取消 | 最终 Linux 二进制打开真实 GTK 保存框，Escape 取消后按 Enter 打开「更多」，见[最终窗口](evidence/implementation/native-export-focus/native/final/save-cancel-enter.png)。 |
| 原生保存成功 | 点击系统 Save 前目标不存在，之后真实生成 120 字节 Markdown，含原始原话；成功后 Enter 同样打开「更多」。见[文件核对](evidence/implementation/native-export-focus/native/final/export-after.json)及[保存后窗口](evidence/implementation/native-export-focus/native/final/save-success-enter.png)。 |
| 导入与设置返回 | 真实 Select File 取消后 Enter 打开「更多」；设置 Escape 关闭后可见「更多」焦点框。见[导入返回](evidence/implementation/native-export-focus/native/final/select-return.png)、[设置返回](evidence/implementation/native-export-focus/native/final/settings-canceled.png)。 |
| 原生保存失败与键盘恢复 | 独立测试目录改为只读，输入第二条合成原话后真实 EACCES，顶栏「保存失败」持续可见，原文件字节不变。Tab 进入滚动工具区显示「重试保存」；恢复原权限后 Enter 保存两条原话，原有首条内容及 ID 保持。见[重试焦点](evidence/implementation/native-export-focus/native/final/recovery-retry-focus.png)、[恢复后窗口](evidence/implementation/native-export-focus/native/final/recovery-saved.png)和[正常退出核对](evidence/implementation/native-export-focus/native/final/normal-quit-verification.json)。 |
| 冷启动 | 正常退出后，用同一最终二进制和该测试数据的副本重开；两条原话恢复，会议文件 SHA256 不变。见[冷启动窗口](evidence/implementation/native-export-focus/native/cold/restored.png)及[字节核对](evidence/implementation/native-export-focus/native/cold/restored-files.json)。 |
| 构建 | TypeScript、Vite、Linux 离线锁定原生构建及 `git diff --check` 通过。原生使用 `--features tauri/custom-protocol` 嵌入最终前端；195 个输入文件哈希复核一致。未重复运行上批 328 项 Node/Eval 或 Rust 单元套件。 |
| 用户预览 | [1430](evidence/implementation/native-export-focus/preview.json)核对 11,575 个构建文件及九个入口资源；[鼠标和触屏页面检查](evidence/implementation/native-export-focus/preview-smoke.json)实际打开会前准备、更多和设置，关闭后焦点正确，无页面异常。原 1423/1430 进程保留。 |

本批最终前端为 `index-C0nJyaAw.js`，原生 SHA256、源码和构建配置见[构建身份](evidence/implementation/native-export-focus/identity.json)。初版用单次动画帧恢复焦点，原生紧凑窗口通过，但跨尺寸时可能早于宽窗口按钮重新启用，因此最终改为提交后恢复。新增延迟保存测试也曾误用上一轮 resolver，已在每轮开始时清除。两类失败和修正前结果均保留在[调查记录](evidence/implementation/native-export-focus/investigation/notes.json)，不混作最终结果。

所有桌面操作运行于独立 Xvfb、D-Bus、XDG 目录和应用标识。记录为键盘输入的合成文本，没有使用用户会议、物理音频或模型推理；界面「板书不可用」符合这次无模型环境。使用 Cua 对已观察窗口进行前台输入，未用 WebKit 元素定向 GrabFocus。Ctrl+Q 会使测试驱动连接随清理关闭，以应用退出码 0 和文件核对确认正常退出。部分截图带有指针提示叠层，不据此判定被遮挡区域。

[归档核对](evidence/implementation/native-export-focus/verification.json)、[SHA256 清单](evidence/implementation/native-export-focus/SHA256SUMS)及[清理记录](evidence/implementation/native-export-focus/cleanup.json)随附。原生测试进程、桌面和两个私有预览均已结束；用户预览继续运行。

尚未构建 Windows ARM 安装包，也没有 Adreno 实机、Mac、VoiceOver、物理触屏或完整 G3/G4 结果。[Windows ARM 原生模型请求修复](windows-arm-local-model-validation.md)仍在工作区，旧安装版 `0.21.1-5` 不含修改，须新构建在原机器复测。Linux 结果不能证明 GPU/驱动侧问题已解决。未提交、推送或发布。
