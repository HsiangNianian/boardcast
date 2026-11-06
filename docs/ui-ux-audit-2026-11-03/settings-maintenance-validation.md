# 设置、Dialog 与维护流程验证

2026-11-03，当前工作树。此记录只覆盖本批实施，不替代完整方案的 Mac 发布门槛。

2026-11-05 更新：[C65 下载与安装前保存](native-update-download-validation.md)已将下载和安装拆成两个明确步骤。下载期间可以返回会议，校验完成后等待用户点击“保存并重启”，再执行下述保存 hook 与安装流程；本页记录中的旧版自动衔接行为已被替换。

已接入六分类设置、顶部分类选择（窄窗）、分类滚动位置、即时外观切换、远程连接未保存提醒及保留原表单功能。共享原生 Dialog 已接入设置、模型、会议库、诊断、更新；App 的快捷键说明与导入模态由主任务接入。

- [桌面设置 1280 × 800](evidence/implemented-settings-1280.png)
- [窄窗设置 720 × 480](evidence/implemented-settings-720.png)

截图来自真实 SettingsPanel 的独立 Vite fixture，使用网页可用能力。开发环境的 node_modules symlink 导致字体资源受 Vite allow-list 限制，因此它们证明布局与可达性，不证明最终字体呈现。完整构建主题截图由主任务另外记录。

## 已运行

- `node node_modules/typescript/bin/tsc --noEmit`：通过。
- `git diff --check`：通过。
- `cargo test updater::tests --lib -- --nocapture`：4 项通过。覆盖下载失败保留同一版本、重复操作互斥、已验签包跨检查与安装失败保留、缺少验签包拒绝安装，以及真实更新配置公钥解析。
- `scripts/ui/check-dialogs.cjs`：通过。实际 SettingsPanel/Dialog 的初始焦点、两端 Tab 循环、背景 inert、`/` 隔离、六分类、嵌套 Escape、忙碌禁止关闭、远程确认/未保存草稿、720px 无整页横向溢出、分类滚动恢复、父子同时关闭后的入口焦点恢复。
- `scripts/ui/check-maintenance.cjs`：通过。实际 UpdatePanel、DiagnosticsPanel 和 IPC wrapper 搭配受控 native bridge，覆盖下载/签名/保存/安装四阶段失败；保存失败不安装；每次安装重试重新执行保存 hook；保留下载不重下；诊断取消不报成功且预览保留；失败与成功重试。续批增加多语言日志、Apple/普通错误、Windows路径及凭据样本检查，确认ZIP字节与预览一致，640×400中英导出按钮可达。

通用运行方式见 [scripts/ui/README.md](../../scripts/ui/README.md)。此次使用已有 Playwright 包与 `/usr/bin/chromium`；未新增项目依赖。两个浏览器检查都接受独立 `(page, baseUrl)`；fixtures 不是生产构建入口。

## 维护流程合同

`update_download` 下载并验证签名，保留当前版本与校验后的字节。`UpdatePanel` 随后等待 App 的 `onBeforeInstall()`，成功后才调用 `update_install`。每次安装重试都重新执行该 hook。错误分为下载、签名、会议收尾、安装；签名失败不能绕过；下载错误不会卸载收音/模型。安装失败保留校验包并尝试重新启动本地模型，明确提示收音已停止，不声称已经恢复录音。Rust 不跨下载 I/O 持有状态锁，下载/安装进行中拒绝重复操作，检查更新不会替换正在处理或已下载的版本。

诊断导出沿用 `saveFile(): Promise<string | null>`：系统保存取消返回 null，保留预览且不通知成功；操作期间禁重复提交，失败可见并可重试。续批 [C63原生验收](native-diagnostics-validation.md) 在真实ZIP中复现英文日志泄漏，现改为数值/固定信号摘要，错误文件与模型日志文件改为JSON；真实Linux取消、权限失败及重试已验证。

未执行真实签名下载、平台安装、重启、macOS VoiceOver 或真实收音尾句保存。浏览器的受控桥与 Rust 状态单测不能替代这些原生验收。原生菜单不在本子任务范围。
