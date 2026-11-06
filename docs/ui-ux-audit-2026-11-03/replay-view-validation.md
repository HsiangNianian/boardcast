# C51/C52 浏览页面不延长回放

2026-11-05，基线 `4f9ab26`。修复首次进入自由编辑、翻页和重开时，把自动生成的画布误记成编辑的问题。[前一批原生数据对照](evidence/implementation/replay-view/observed-native-history.json)显示，保存记录原有 11/3 步，实际导出的 HTML 却变成 12/4 步；新增步骤的图差异为空，时间戳落在浏览期间。

产品改动只有 [App.tsx 中的 4 行](evidence/implementation/replay-view/product-change.patch)：没有历史场景时，先确认当前场景与自动生成内容一致，再将其作为比较基准，不追加回放步骤。沿用现有 `mergeScene`、`sceneChanged` 和记录入口。空场景仍须先返回：初版把空场景也设成基准，浏览器通过后，原生 PDF 启动仍误增一步，见[初版原生回放](evidence/implementation/replay-view/native-page2-replay.png)。最终调整顺序后，真实编辑、撤销和重做仍正常记录。

| 验证 | 结果与证据 |
| --- | --- |
| 浏览器前后对照 | [旧版 18 项中 4 项失败](evidence/implementation/replay-view/before-complete/check-replay-view/replay-view-check.json)：两页原本各有 2 步，浏览后变成 3 步，应用与离线 HTML 均受影响。[最终 19 项全部通过](evidence/implementation/replay-view/final/check-replay-view/replay-view-check.json)，增加直接以自由编辑模式冷重开的检查；覆盖翻页、模式切换、窗口尺寸变化、实际 HTML 下载、拖动、绘制、删除最后一个对象、撤销/重做，以及外部 Excalidraw 导入。 |
| Linux 原生时间线 | 使用此前真实导入、标注的两页 PDF 测试记录，在私有桌面启动最终二进制。原生页面恢复原有 [3:23](evidence/implementation/replay-view/final-native-page1-replay.png) 和 [1:44](evidence/implementation/replay-view/final-native-page2-replay.png)；切回页面后底图与标记一致。 |
| 系统保存与离线产物 | 经真实 GTK 对话框保存 [native-final.html](evidence/implementation/replay-view/exports/native-final.html)。保存前 3 秒、7 次采样均无目标文件，修改时间位于明确 Save 点击期间，之后只剩主窗口。禁网打开实际文件，两页分别保留 11/3 步与原时间戳，底图可解码，无 HTTP 请求或脚本错误。[15 项核对全部通过](evidence/implementation/replay-view/native-check.json)，[离线第二页](evidence/implementation/replay-view/offline-native-page2.png)。 |
| 退出与冷启动 | 最终二进制正常退出后在同一私有数据目录重新启动，[回放仍为 1:44](evidence/implementation/replay-view/cold-native-replay.png)。两次退出后的会议文件均与测试前逐字节相同；应用退出码均为 0，私有显示服务已清理。 |
| 回归与构建 | [310 项 Node/Eval](evidence/implementation/replay-view/final-node-tests.txt)、[前端构建](evidence/implementation/replay-view/final-build.txt)及 [Linux locked/offline debug 构建](evidence/implementation/replay-view/final-build/native-build.txt)通过。最终生产浏览器另通过应用回放 31 项、PDF 换页 8 项、七格式导出 92 项，以及自由场景改名、模型候选、样式、撤销和续会分支检查，见[日志](evidence/implementation/replay-view/final-browser.txt)。模型响应使用受控 fixture。 |

最终原生二进制 SHA-256 为 `974f7e6dca22f641425a08c738f9549ad128247d20bebadc3763fed230b002fe`，见[构建身份](evidence/implementation/replay-view/final-build/identity.json)。1423/1430 保持原预览进程，1430 当前入口为 `/assets/index-DXZakcAc.js`，入口和播放器与磁盘产物逐字节相同，见[预览身份](evidence/implementation/replay-view/preview-identity.json)。[归档清单](evidence/implementation/replay-view/manifest.json)含 238 个文件、35,378,268 字节，另附清单本身；初版失败与自动化修正保留在[验证备注](evidence/implementation/replay-view/validation-notes.json)。

本次防止新产生的多余步骤，不自动删除既有历史。未改写用户会议，未提交或发布。用户暂无测试 Mac；真实音频/模型、触摸/笔、VoiceOver、签名更新、其余 G2/G3 与 G4 用户试用仍待验证，整体重构未关闭。
