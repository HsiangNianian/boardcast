# C53 回放中间分支

2026-11-06，基线 `4f9ab26` 的当前工作区。新增生产界面检查，补齐第 0 步、中间步、后续生成、自由对象独立编辑和重开验收；本批没有产品代码改动。执行者为 Codex，输入均为合成记录和模型响应。

## 生产浏览器

[`check-replay-fork.cjs`](../../scripts/ui/check-replay-fork.cjs) 通过真实导入、编辑器、IndexedDB 和界面操作创建两张源白板。第一张含图节点、自由矩形、笔迹和图片，并在中间历史之后新增话题及矩形；第二张有独立内容。

[16 项检查通过](evidence/implementation/replay-fork/browser/report.json)：

- 第 0 步禁用分支并提供解释，浏览回放不写回会议。
- 中间步分支追加第 3 张白板和一条说明，保留所选图、早期自由对象、图片和回放前缀的时间；后来的话题及绘图不进入分支。
- 后续模型请求只包含分支内容，原两张白板的图、布局、场景、素材和历史保持。编辑分支自由对象名称也不影响源对象。
- 冷重载后存储记录完全一致；末步再次分支得到第 4 张白板，返回源板仍显示原来的后续话题和对象名。共四次模型提交，无未捕获页面错误。

对象比较仅排除 Excalidraw 重建时的 `version/versionNonce/updated/index`，保留种子、样式、坐标、绑定、素材和步骤时间。首次脚本错误把被动翻页当作应保存的内容；改为检查界面 `aria-current`。另一次错误来自直接比较上述编辑器修订元数据。两项都是测试假设修正，没有修改产品保存行为。

## 真实 Linux 桌面

复用[自由对象批次](free-objects-validation.md)的最终二进制，SHA-256 为 `49189178c8353e38d57e6d4ff05defe10318031092c9cf964e5ee8f53bbe5fc3`；被检查的五个产品/逻辑文件哈希未变，见[身份核对](evidence/implementation/replay-fork/identity-check.json)。测试窗口保留此前批次名称。使用独立 XDG、D-Bus、Xvfb、无物理音频配置，通过 Cua 操作真实窗口和 Tauri 文件/IPC；模型是仅监听回环地址的合成 HTTP 服务。

1. 从会议库打开源记录，通过键盘回到第 0 步，确认分支禁用，源文件字节不变；第 2 步内容列表只显示历史内容。[中间步窗口](evidence/implementation/replay-fork/native-status/middle-contents.png)。
2. 分支创建第 3 张白板，通过真实输入提交一次模型请求，保存 `Native branch addition`；请求无未来话题或另一张板的内容。再将自由对象改名为 `Native branch-only shape`，原两页内容保持。[实际文件](evidence/implementation/replay-fork/native-status/edited-meeting.json)。
3. 正常退出、在新的隔离目录冷启动，确认分支与改名恢复；返回原板仍显示 `Future topic`、原自由对象和后来画的矩形。[冷启动](evidence/implementation/replay-fork/native-cold/restored.png)、[原白板](evidence/implementation/replay-fork/native-cold/original-settled.png)。
4. 一份会议、五份素材及锁文件在退出、冷启动、仅浏览原板、再次退出之间哈希一致。两个原生运行均退出码 0，登记的 18 + 17 个进程和合成模型服务均结束；1423/1430 预览保留。

跨 Chromium→WebKit 的首次实际保存有明确缓存差异：源板 SVG 内嵌字体从子集变为另一份字体载荷，移除该 `<style>` 后 SVG 完全相同；第二张板的手绘盒尺寸/缩略图重新计算，区域坐标仅有浮点末位变化。所有变化路径与前后值保存在[结果](evidence/implementation/replay-fork/result.json)。原图、自由对象、素材原始字节和步骤内容没有变化；后续模型与改名以原生首次写出记录为基线，仅排除编辑器修订元数据后，原两页完全一致。这里不声称浏览器记录首次写入原生时所有布局缓存逐字节不变。

## 可复核与边界

只读运行 `python docs/ui-ux-audit-2026-11-03/evidence/implementation/replay-fork/check-evidence.py` 可复核 31 项归档断言；它不重新操作 GUI。新脚本的 16 项生产检查已在现有完整构建通过；本批没有产品改动，未重复构建或全量单元测试。[检查期间的配置/操作偏差](evidence/implementation/replay-fork/investigation.json)单独记录。

Windows 本地模型通道的 4 项针对性测试也重新通过。真实 Linux 本地模型预热和生成证据见 [Windows ARM 修复说明](windows-arm-local-model-validation.md)；本批合成模型检查不能替代它。尚无新 Windows ARM 安装包、Adreno 原机复测、Mac 实机/VoiceOver、物理音频和 G4 用户试用；完整 G3/G4 不关闭。
