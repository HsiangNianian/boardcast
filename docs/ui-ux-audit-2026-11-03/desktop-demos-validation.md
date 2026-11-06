# 桌面无模型示例与首次编辑保存

2026-11-05，基线 `4f9ab26`，接续 C02/C03。桌面端现在可以直接播放内置录制示例；浏览示例不新增个人会议，首次编辑会进入正常保存流程。

## 问题与修改

- 桌面端原先跳过示例索引加载，未安装模型时按钮一直显示「等待模型」。现改为网页和桌面共同加载已有 `public/demos`，模型就绪时仍沿用原有实时示例。
- 原先只有新增一句话才会解除示例的免保存标记；直接改名、编辑摘要、自由绘图或修改本会术语都会被保存入口跳过。修复前生产页面实测出现 8 项失败，见[对照记录](evidence/implementation/desktop-demos/baseline-complete/check-demo-edits/report.json)。
- 保存入口复用现有 `meetingContent` 比较内容，实际编辑转为个人会议；回放、切换视图和自动生成的排版不会误存。打开会议时同步本会隐私和术语的引用值，避免初始快照混入上一场会议的设置。
- 桌面示例下方复用「配置白板模型」文案，直接打开「白板与模型」设置。

产品只修改 `App.tsx` 和 `demo.ts`；没有新依赖或存储格式。[本批补丁](evidence/implementation/desktop-demos/product-and-tests.patch)及[文件哈希](evidence/implementation/desktop-demos/changed-files.json)不混入此前 UI 改动。

## 验证

| 范围 | 结果 |
| --- | --- |
| 生产页面新增回归 | [37 项通过](evidence/implementation/desktop-demos/final-browser/check-demo-edits/report.json)。真实网页存储及受控桌面 IPC 分别覆盖无模型入口、改名后切会、摘要/自由绘图/术语保存及刷新恢复、已有会议保持、仅切换视图不保存；桌面配置入口正确，未发出推理、录音或下载命令。 |
| 现有示例回归 | [55 项通过](evidence/implementation/desktop-demos/browser-regressions/check-demos/demos-check.json)：全部 26 个中英示例、完整步骤、历史隔离、坏 JSON 后重试、新增原话转为个人会议。 |
| 回放与内容列表 | [31 项通过](evidence/implementation/desktop-demos/browser-regressions/check-content-replay/content-replay-check.json)，覆盖回放只读、编辑返回、快捷键和场景保持。 |
| 保存单元检查 | [8 项通过](evidence/implementation/desktop-demos/meeting-save-tests.log)，包括并发保存、失败重试、未改内容和自由绘图手势。 |
| 构建 | TypeScript、[生产构建](evidence/implementation/desktop-demos/build.log)、[Linux 原生离线锁定构建](evidence/implementation/desktop-demos/native-build.log)通过。11575 个前端文件与原生构建输入逐字节一致，见[构建身份](evidence/implementation/desktop-demos/identity.json)。 |

55/31 项回归在共享保存修复后运行；随后只调整桌面配置链接，最终构建重跑 37 项新增检查并进行下述原生验收。早期校验脚本的 IndexedDB 读取及矩形控件点击问题已修正，未计入产品缺陷，见[脚本说明](evidence/implementation/desktop-demos/test-notes.json)。

## Linux 原生窗口与文件

使用独立 XDG、应用标识、Xvfb、D-Bus 和真实 Tauri/WebKit 窗口；未安装模型，未接入物理音频。通过 Cua 驱动界面，直接读取应用写出的真实文件核对。

1. 模型未安装时示例按钮可用，内置资源可以播放；首次仅浏览后会议库及实际 `meetings/v2` 目录均为空。见[窗口](evidence/implementation/desktop-demos/native/native-library-empty.png)和[目录证据](evidence/implementation/desktop-demos/first-run/untouched-proof.json)。
2. 最终构建的配置链接打开正确设置分区，见[设置窗口](evidence/implementation/desktop-demos/native/final-settings.png)。
3. 播放示例后，将主题改成 `Sample decision persists`，再新建会议。应用写出一个会议文件，包含 6 个节点、7 行记录和 6 个步骤，其中一行记录改名。见[保存证据](evidence/implementation/desktop-demos/saved-proof.json)。
4. 再播放一个未修改的示例，会议库仍只有原先的个人会议，其文件字节不变。见[会议库](evidence/implementation/desktop-demos/native/final-library-one.png)与[文件比较](evidence/implementation/desktop-demos/library-proof.json)。
5. 从会议库打开个人会议，正常退出后冷启动，画布和内容列表均恢复改名结果，文件仍逐字节相同。见[冷启动窗口](evidence/implementation/desktop-demos/native/cold-name.png)与[核对结果](evidence/implementation/desktop-demos/cold-proof.json)。文件 SHA-256 为 `94d0a9f418b55b109c8dfd0648ab21a31715513c73f6b4a2d10d58ef9a4c0130`。

最终原生二进制 SHA-256：`4b0143a5aea906b0dbe07043a3002696b65b240debbc769bab4190fe263488bd`。首次运行的中间构建身份单独保留。三次运行的 21 个登记进程均已退出，私有 X socket 已移除，见[清理核对](evidence/implementation/desktop-demos/process-proof.json)。截图中的黑色指针图形来自测试驱动覆盖层，原图未修改。

## 剩余边界

本批原生验收使用 Linux；没有 Windows ARM 或 Mac 实机，没有重新测试真实音频或模型实时生成。Windows ARM 预热修复的边界继续见[专门报告](windows-arm-local-model-validation.md)。本批不代表 G3/G4 或整体重构完成，没有提交、发布或安装到用户应用目录。
