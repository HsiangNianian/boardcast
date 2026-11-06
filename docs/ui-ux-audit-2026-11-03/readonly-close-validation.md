# C33/C68 旧档案的只读浏览与退出

2026-11-06。本批接续 PDF 板号验收中发现的「未编辑档案退出时被重写」现象，修复会议恢复、任务收尾和保存提示的三个判断。产品只改 `src/App.tsx`；复用现有内容比较、保存队列和旧板回放步骤，不改变存储格式。[本批差异](evidence/implementation/readonly-close/product-and-tests.patch)和[最终源码哈希](evidence/implementation/readonly-close/source-final.json)可逐项核对。

原实现有三个独立触发点：

- 退出或切会无条件用全部原话重建模型上下文。旧档案的上下文为空或与原话不同，就会触发保存；磁盘不可写、素材缺失时还会阻止只读退出。现在仅在确有未完成原话要取消时重建。
- 旧档案未保存回放步骤时，首次浏览另一张板才补建初始步骤，导致非预期写入。现在打开时为所有旧板准备初始步骤，再记录未编辑基线；真正编辑后的记录仍能回放原板。
- 翻页和渲染缓存变化先被标记为未保存，500 ms 后才比较内容。现在先使用现有 `writer.unchanged` 判断；只读浏览保持「已保存」，不会误触发离页保护。正在写入的旧快照仍由原有保护逻辑处理。

四份反例日志分别保留了[旧档案退出失败](evidence/implementation/readonly-close/baseline-shell.log)、[缺失素材只读退出失败](evidence/implementation/readonly-close/baseline-missing.log)、[浏览旧板产生一次写入](evidence/implementation/readonly-close/baseline-legacy-browsing.log)及[短暂未保存和离页拦截](evidence/implementation/readonly-close/baseline-view-status.log)。后两项是在真实桌面检查中发现后补入回归，并在修复前的生产构建上失败；未把中间版本算作最终通过。

| 最终验证 | 结果及证据 |
| --- | --- |
| 保存、取消、重试、回放逻辑 | [28 项 Node 测试通过](evidence/implementation/readonly-close/node-status-tests.log) |
| 原生协议下的完整生产界面 | [全部通过](evidence/implementation/readonly-close/browser/final/check-native-shell/native-shell-check.json)：4 组空/独立上下文 × 快速/延迟打开，浏览和退出零写入、即时保存提示和离页保护正确；随后真实界面编辑能保存并保留旧板回放。原有停止失败、写入失败、尾句保存后才确认退出、重复请求和撤销/重做检查仍通过。这里使用受控 IPC，不冒充真实磁盘或音频。 |
| 缺失素材 | [只读退出、翻页保留原件、编辑后阻止退出及文字救援通过](evidence/implementation/readonly-close/browser/final/check-missing-assets-close/missing-assets-close-check.json) |
| 相关回归 | [生产构建日志](evidence/implementation/readonly-close/browser-related-status.log)：27 项录音切换、17 项生成中翻页、19 项浏览/回放、25 项实际 PDF 板号及 92 项七格式检查通过 |
| 构建 | TypeScript、生产前端、[Linux 原生离线锁定构建](evidence/implementation/readonly-close/native-status-build.log)通过；独立播放器未改动，沿用已有产物 |

真实 Linux 使用独立 XDG、D-Bus、Xvfb 和应用标识 `games.mmstudio.boardcast.g2.readonlyclose`。最终二进制 SHA-256 为 `2f075edfaa0a57b440dabf982f6d5f4184af50dae76fcb7d4191e2f302051119`，见[构建身份](evidence/implementation/readonly-close/identity.json)。没有启用物理麦克风或模型资源，输入和档案均为合成数据。

1. 打开含 5 张板、底图、自由矩形和 2 条原话的旧档案，切到第 2、4 张板。最终窗口仍显示原保存时间，[两份原档案字节完全不变](evidence/implementation/readonly-close/native-status/browsed-records.json)。[自由图形与保存状态](evidence/implementation/readonly-close/native-status/board-four.png)已实际查看。
2. 通过真实 GTK 保存框点击 Save，生成 [3 页 PDF](evidence/implementation/readonly-close/native-status/boardcast-20261106-1100.pdf)。点击前文件不存在，点击后有真实文件与[成功提示](evidence/implementation/readonly-close/native-status/exported.png)，[原档案仍不变](evidence/implementation/readonly-close/native-status/exported-records.json)。PDF 哈希与上一批已逐页核对的板号产物相同。
3. 缺失素材旧档案可切会、退出和冷启动，原件不变；[缺失素材冷恢复](evidence/implementation/readonly-close/native-cold/restored.png)来自数据修复阶段。最终二进制再次从这些实际数据启动，[仍正确提示缺图并显示原话](evidence/implementation/readonly-close/native-status/restored.png)。
4. 最终二进制中新建会议并实际输入一句话，[新记录成功写入且两个旧档案保持原字节](evidence/implementation/readonly-close/native-status/new-edit-proof.json)。正常退出后，直接复制实际应用数据冷启动，[新原话恢复](evidence/implementation/readonly-close/native-status-cold/restored.png)，[三份文件在再次退出后字节一致](evidence/implementation/readonly-close/native-status-cold/after-quit-records.json)。模型未安装，因此该句按「原话已记录、板书未生成」保存。

所有 5 次独立原生运行正常退出，共核对 101 个注册测试进程均已结束；私人预览也已结束。Ctrl+Q 后启动器立即清理驱动，控制命令可能返回 socket 关闭错误，因此应用退出以启动器的 `app: 0`、进程身份和文件核对为准。每次运行的 `cleanup-proof.json` 保留了这些证据。

现有 1423/1430 预览进程保留，1430 发布最终构建；11575 个文件及 10 个 HTTP 资源与产物逐字节一致，旧哈希资源保留，见[发布核对](evidence/implementation/readonly-close/preview-publication.json)。发布后的原生协议与缺失素材流程[再次通过](evidence/implementation/readonly-close/user-preview-status.log)。

本批只关闭上述已复现的只读保存问题，不代替全部 C33/C68、G3/G4 验收。Windows ARM 新安装包与实机、macOS/VoiceOver、物理音频和目标用户试用仍待完成；未提交、推送或发布安装包。
