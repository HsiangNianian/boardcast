# C42 arXiv 原生下载、取消与恢复

后续 [原生取消与新会议恢复](native-arxiv-cancel-validation.md)已补上真实连接/正文传输中断、重新导入、新建会议目的地与冷启动。下文保留本批当时的实现和验收边界，不再用其中“仅忽略迟到结果”描述当前产品。

2026-11-05，基线 `4f9ab26`。在隔离的 Linux Tauri 桌面中输入真实的[带版本号论文链接](https://arxiv.org/abs/1706.03762v7)，实际下载、解析并保存；本次 arXiv 流程没有模拟 IPC 或替换网络响应。产品改动只有 [arXiv 链接提取的一行及配套单测](evidence/implementation/native-arxiv/product-change.patch)：整个输入必须是支持的链接、裸 ID 或带前缀 ID，避免把 `notarxiv.org`、其他站点路径和夹杂说明文字中的片段误认成链接。后端原本就只请求规范的 arxiv.org 地址，本次不是修复向相似域名发送数据的问题。

| 验证 | 结果与证据 |
| --- | --- |
| 修改前后对照 | [旧解析器失败](evidence/implementation/native-arxiv/parse-tests-before.txt)，[修复后通过](evidence/implementation/native-arxiv/parse-tests-after.txt)。新增 9 种拒绝输入，保留版本号、旧式分类 ID、大小写域名、查询参数、片段及子域名。最终桌面实际输入相似域名，显示[无效 ID 提示](evidence/implementation/native-arxiv/final-invalid-error.png)。 |
| 真实下载与预览 | 修复前后的二进制均下载成功，预览显示 [13 张白板、跳过 2 页参考文献](evidence/implementation/native-arxiv/final-ready.png)。修复前取消预览后没有会议文件；最终版本确认导入前也没有会议文件，见[取消后](evidence/implementation/native-arxiv/after-cancel-files.json)及[提交前](evidence/implementation/native-arxiv/final-precommit-files.json)。 |
| 提交与落盘 | 选择“Add to this meeting”后显示 [Saved](evidence/implementation/native-arxiv/final-saved.png)，保存 13 页、13 个独立底图引用和一条含版本号的导入说明。当前会议沿用自动标题 `Paper outline`；本次没有走新建会议目的地。[实际会议记录](evidence/implementation/native-arxiv/committed-meeting.json)、[第 3 页图表](evidence/implementation/native-arxiv/final-page3.png)。 |
| HTTP 失败与重试 | 输入语法合法但不存在的 `0000.00000`，真实返回 [404](evidence/implementation/native-arxiv/final-404-downloading.png)。点击重试后重新显示同一 ID 的[下载状态](evidence/implementation/native-arxiv/final-404-retried.png)，再次返回 404；原输入和目标第 3 页保留，取消后会议文件逐字节未变。 |
| 重复导入与取消 | 再次输入同一论文，重新到达[准备完成的预览](evidence/implementation/native-arxiv/final-repeat-ready.png)。取消后仍为原有 13 页，会议文件逐字节未变，没有追加页面或导入说明。 |
| 正常退出与冷启动 | 同一最终二进制、同一私有数据目录正常退出后重新启动，实际[恢复第 3 页](evidence/implementation/native-arxiv/cold-open.png)。会议 JSON 和 13 个底图文件在两次退出后哈希均不变；13 张已保存 JPEG 在独立禁网 Chromium 中[全部解码成功](evidence/implementation/native-arxiv/asset-decode.json)。两次正式测试的应用退出码均为 0，私有显示服务已清理。 |
| 回归与构建 | [311 项 Node/Eval](evidence/implementation/native-arxiv/node-tests.txt)、[前端构建](evidence/implementation/native-arxiv/build.txt)、[Linux locked/offline debug 构建](evidence/implementation/native-arxiv/final-build/native-build.txt)通过。生产浏览器[导入任务 14 项](evidence/implementation/native-arxiv/regressions/check-import-task/import-task-check.json)全部通过、无页面异常；其中故障和模型响应仍是受控 fixture。 |

[12 组原生证据核对](evidence/implementation/native-arxiv/native-check.json)通过。最终二进制 SHA-256 为 `06e8255c85901063b4fee863fc7c836ff6237ace2515651a6319fda49b9880db`。1423/1430 保持原预览进程，1430 当前入口 `/assets/index-B_-usnkt.js` 与磁盘及原生构建冻结产物逐字节一致，见[预览身份](evidence/implementation/native-arxiv/preview-identity.json)。[源码身份](evidence/implementation/native-arxiv/source-final.json)确认本批没有改动 App、PDF 处理、任务面板或 Rust 下载实现；[归档清单](evidence/implementation/native-arxiv/manifest.json)记录截图、原生状态、构建与测试结果。二进制、完整前端和论文底图保留在隔离缓存，未将其重复加入仓库。截图黑色形状是 Cua 光标覆盖层，自动化启动修正见[验证备注](evidence/implementation/native-arxiv/validation-notes.json)。

本次覆盖真实 HTTP 404，不代表 DNS 失败、断网和超时均已实测。取消仍只保证不接纳迟到结果，不中断 Rust 下载传输；重复提交、新建会议目的地和其他平台的原生变体继续保留。用户暂无测试 Mac；真实音频/模型、输入设备、签名更新、其余 G2/G3 与 G4 仍待验证，整体重构未关闭。未改写用户会议，未提交或发布。
