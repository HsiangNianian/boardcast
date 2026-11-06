# 收音中新建与切换会议确认

依据 [完整方案 §8.2](refactor-plan.md#82-保存与跨会隔离)。本批完成共享切会入口的确认与取消语义；不代表全部 C01–C68 或 G2/G3/G4 完成。

## 实际行为

- 收音中或收音正在启动时，新建、打开其他会议、从其他会议回放/导出、接着旧板开会、导入为新会议都会先显示“停止并切换 / 留在当前”。默认键盘焦点在“留在当前”。
- 等待选择时仍接收原话并自动保存当前会议。取消或 Escape 不停止收音、不取消正在启动的输入、不清除输入草稿；会议库和导入预览保留，且不把取消显示为保存失败。
- 确认后复用原有停止、尾句收尾、捕获和保存边界。重复确认只执行一次；目标会议在当前会议保存确认后才加载。停止失败、保存失败均不切会；停止成功但保存失败后可直接重试，无需再次确认已停止的输入。
- 从会议库取消时恢复原动作按钮的焦点。触发按钮在等待过程中会被禁用，因此在打开确认框前记住触发点，并在取消后下一帧恢复。
- 原生关闭会撤销尚未批准的切会意图，再保存当前会议并答复退出请求。更新安装、已明确确认的删除和维护操作继续使用原有保存边界；不会再叠加含义不符的切会提示。
- 未收音的新建仍是直接操作。追加材料到当前会议沿用原有导入提交行为。

## 验证

生产浏览器预览 `http://127.0.0.1:1430` 使用真实 React App；输入、存储和原生关闭通过受控 Tauri 协议夹具响应。最终入口和源码 SHA-256 见 [构建身份](evidence/implementation/recording-switch-build.json)。

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| 收音切换与取消 | 27 项通过，0 个浏览器脚本错误 | [机器报告](evidence/implementation/recording-switch-check.json) |
| 停止、尾句和保存顺序 | 暂停停止回复、暂停保存回复、重复批准、停止失败、保存失败及重试全部通过 | 同上 |
| 原生外壳协议回归 | 启动时退出、退出拒绝/重试、复盘、模型删除、文本/双画布撤销重做等通过 | [机器报告](evidence/implementation/native-shell-check.json) |
| 会议库与导入 | 各 14 项通过 | [会议库](evidence/implementation/library-check.json)、[导入](evidence/implementation/import-task-check.json) |
| 异步隔离 | LLM、摘要、导入各自的迟到成功/失败，共 6 条路径通过 | [机器报告](evidence/implementation/lifecycle-check.json) |
| 实际组件夹具 | 会议库失败与取消、嵌套 Dialog 键盘/焦点、维护下载/保存/安装失败重试通过 | [运行日志](evidence/implementation/recording-switch-ui.txt) |
| 中文紧凑窗口 | 720 × 480 的说明和取消按钮可见、可操作 | [截图](evidence/implementation/recording-switch-zh.png) |
| 静态与构建 | `tsc --noEmit`、生产 Vite 构建及 diff 空白检查通过 | [构建日志](evidence/implementation/recording-switch-build.txt) |

复跑命令（使用已有 Playwright 和 Chromium，无新增依赖）：

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-recording-switch.cjs check-native-shell.cjs check-library.cjs check-lifecycle.cjs check-import-task.cjs
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1423 check-meetings.cjs check-dialogs.cjs check-maintenance.cjs
```

测试最初发现并修复了嵌套确认取消后焦点丢失。另纠正两处测试假设：单页会议可省略 `pages`；保存时可以读取当前会议核对 revision，禁止提前读取的是目标会议。

## 仍未验证

本批没有使用真实麦克风，没有在用户的会议目录写入文件，也没有真正退出或安装应用。既有 Linux 原生存储证据有独立的构建身份；本批协议结果不把它扩展成当前构建的原生音频证明。真实 macOS 收音尾句、权限、VoiceOver、长会议和安装/重启仍需 G3；G2 剩余故障和 G4 用户试用继续保持未完成。
