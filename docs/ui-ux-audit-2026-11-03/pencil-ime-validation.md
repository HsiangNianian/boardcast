# 画布改名、别名与中文输入法

2026-11-05，接续[文字编辑批次](pencil-text-validation.md)。产品修改限于 `App.tsx`、`freeScene.ts` 和 `session.ts`，没有增加依赖或改变记录格式。[本批补丁](evidence/implementation/pencil-ime/product-and-tests.patch)以本轮开始时的工作区为基线。

连续改字原来会把 `My edited topic c`、`…co` 等半成品加入别名，保存后还会进入模型上下文。现在每次编辑复用编辑前的名称和别名，再通过既有改名规则生成当前草稿；移动、标记和其他节点继续保留。草稿仍自动保存，旧记录中已有的别名不做猜测性清理。

新模型请求在读取板书前等待编辑结束，已经发出的请求继续等待到编辑结束再应用；取消会阻止旧请求在新会议中继续。没有提供编辑等待回调的 Session 调用保持原有调度。

真实 Rime 检查还发现：把长标题改为“你好”后，自动缩短下划线会多记一步。现在只允许刚提交的语义编辑吸收随后生成的排版，更新该步骤的最终场景。移动对象、撤销和重做仍独立记录。

| 验证 | 结果与证据 |
| --- | --- |
| 失败基线 | 两项别名单元、两项请求等待单元及[旧版别名页面](evidence/implementation/pencil-ime/baseline-alias/report.json)失败；[独立改名页面](evidence/implementation/pencil-ime/baseline-reflow/report.json)重现重复步骤。 |
| 单元与 Eval | [328 项通过](evidence/implementation/pencil-ime/node-tests.log)，包括真实旧名保留、未完成草稿排除、撤销、跨次改名、请求前等待和取消隔离。 |
| 生产页面 | [10 组全部通过](evidence/implementation/pencil-ime/production-report.json)：文字编辑 30 项、绘图并发 18 项、手势 10 项、回放 19 项、导出 64 项、启动 58 项，以及场景往返、生命周期、原句重试和本地模型请求。 |
| 构建 | TypeScript、生产前端和 Linux 原生锁定离线构建通过；[最终构建身份](evidence/implementation/pencil-ime/identity.json)与[预览资源核对](evidence/implementation/pencil-ime/preview.json)已保存。 |
| 归档核对 | [40 项证据检查](evidence/implementation/pencil-ime/verification.json)通过，可运行同目录的 `verify-evidence.py` 复核。 |

首次排版合并过宽，既有回放检查发现移动后重做漏记步骤；限定到刚提交的语义编辑后，全套页面检查通过。首次新增浏览器用例点到了空白区域，随后改为明确的话题节点并先适配视图；另一次英文输入沿用 Rime，产生无效合成文本。修正测试输入后重新验证，没有把这些结果当作模型故障。[失败记录与说明](evidence/implementation/pencil-ime/investigation/notes.json)均保留。

使用独立 XDG、Xvfb、D-Bus、Fcitx5/Rime 和真实 Tauri/WebKit 窗口，输入与记录均为自有测试数据：

1. 实际按键输入 `nihao`，[候选窗口](evidence/implementation/pencil-ime/native/verified-candidate-desktop.png)出现；Escape 取消候选并保留编辑框和原名，Enter 只把拼音提交到编辑框。
2. 空格选出“你好”，文本撤销恢复 `nihao`，重做恢复中文；全过程草稿保存、别名仅保留原名，白板历史仍为初始一步。
3. 普通 Escape 结束编辑后恰好增加一步，下划线最终形状写入同一步回放；一次全局撤销/重做恢复原名/中文，见[最终文字](evidence/implementation/pencil-ime/native/verified-finished.png)和[保存记录](evidence/implementation/pencil-ime/native/verified-global-redo-meeting.json)。
4. 正常退出后冷启动，[会议文件逐字节不变](evidence/implementation/pencil-ime/native/cold-proof.json)。随后用原名 `Canvas input test` 发出英文指令，真实 Qwen 返回 `M n1 星标`，给“你好”加星而不新增节点；[原生窗口](evidence/implementation/pencil-ime/native/cold-model.png)和[最终记录](evidence/implementation/pencil-ime/native/final-meeting.json)相符。追加星标后已正常保存退出，没有再次冷启动验证该次星标。
5. 两次最终构建运行都完成真实模型预热；原四页图、已有原话和源测试文件保留，没有音频采集流。四次运行登记的 76 个进程已结束，[清理核对](evidence/implementation/pencil-ime/native/cleanup-proof.json)未发现该私有环境遗留进程。

本轮复核用户 Windows ARM 诊断：服务 `ready`、Adreno 已识别、前端 `Failed to fetch`，不足以判定 GPU 或驱动根因。此前的[原生请求及预热修复](windows-arm-local-model-validation.md)在当前构建的 10 项页面检查和真实 Linux 预热中继续通过；没有新的 Windows 安装包或 Windows ARM 实机结果。用户暂无测试 Mac，其他输入法、平台验收和 G3/G4 仍保留。工作区未提交、推送或发布。
