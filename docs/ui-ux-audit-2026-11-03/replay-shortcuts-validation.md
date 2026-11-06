# C52/C62 续批：回放退出快捷键与可运行预览

2026-11-04，HEAD `4f9ab26`。当前开发预览为 `http://127.0.0.1:1423/`，生产预览为 `http://127.0.0.1:1430/`；本批已刷新生产构建。核心重构已落地，完整 C01–C68 / G1–G4 验收仍在进行。

## 修改

回放中的按键分支原先在处理通用快捷键前返回，导致帮助列出的 Ctrl/Command+Shift+R 只能进入、不能退出。现在该组合可以退出回放；进度滑杆上的 Esc 和退出组合也生效，滑杆方向键保留原生单步行为。文本输入、模态窗口、合成 IME composition 标记和按钮的空格操作继续隔离。

快捷键帮助在 Mac 平台显示 Command，重做显示与原生菜单一致的 Shift+Command+Z。实际按键映射和全局快捷键注册未更改。

检查用户预览时另发现两个环境问题：运行中的 Vite 缓存与磁盘上的依赖预构建不一致，使主界面与自由编辑加载不同的 React chunk，切换后白屏；重新加载服务后已恢复。工作树的 `node_modules/.pnpm` 链接到共享依赖目录，其字体请求返回 403；开发服务现在允许读取该目录的实际路径。没有新增依赖。

## 验证

- [修复前回放检查](evidence/implementation/replay-shortcuts/baseline-keys/content-replay-check.json)通过前 15 项后，在同一组合键退出回放处超时，页面无 JavaScript 错误。
- [开发页面](evidence/implementation/replay-shortcuts/dev-final/content-replay-check.json)与[生产页面](evidence/implementation/replay-shortcuts/production-final/content-replay-check.json)各 31 项通过：Control/Meta 进入及退出、画布/滑杆焦点、滑杆 Esc、单步推进、文本/模态/IME 隔离、回放只读与存储内容保持、双视图和字体加载。两者均无页面脚本错误、无字体 HTTP 错误。
- [原生外壳协议 fixture](evidence/implementation/replay-shortcuts/native-shell-fixture/native-shell-check.json)通过：退出前停止/保存、失败阻止退出、尾句保存顺序、文本/双画布撤销重做、预览停止、复盘与模型删除保存屏障。
- [4 项快捷键单测](evidence/implementation/replay-shortcuts/shortcut-tests.txt)通过；[pnpm build](evidence/implementation/replay-shortcuts/build.txt)完成类型检查、播放器和主应用构建。未重跑全量 Node/Rust，也未重新构建或启动原生应用。

Mac 帮助通过覆盖浏览器 `navigator.platform` 验证显示，见[截图](evidence/implementation/replay-shortcuts/production-final/replay-shortcuts-mac-help.png)，不代表 Mac 真机验收。截图关闭动画以避开弹窗淡入的中间帧。字体检查曾误将正常缓存响应 304 判为错误，已改为只记录 HTTP 400 及以上并复跑。

生产入口 `index-BMK1zIca.js`，SHA-256 `3f2b0a43931c138747738317aff165479026b96c19eda6b4ece3a01e1264d5ab`。本批补丁、测试脚本和产物摘要见[归档目录](evidence/implementation/replay-shortcuts/)。

所有本批测试浏览器已关闭；1423/1430 服务保持原 PID 供用户体验。实际系统快捷键、真实输入法、Mac 音源/权限、VoiceOver、60 分钟现场会议与 G4 用户试用继续保留。未提交、推送或发布，整体目标未关闭。
