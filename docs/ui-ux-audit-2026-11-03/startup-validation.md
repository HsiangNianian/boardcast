# C01/B0 启动恢复期间的新内容保存

2026-11-05，基线 `4f9ab26`。检查生产页面的启动体验时发现：旧会议尚在读取，用户已经提交的新原话和白板却不进入自动保存，保存栏保持空白，浏览器关闭保护也未启用。[修改前的实际页面](evidence/implementation/startup/before-restore-pending.png)及[探测结果](evidence/implementation/startup/probe.json)记录了这一状态；随后用真实 IndexedDB 保留旧会议、延迟其读取完成回调，[复现等待保存超时](evidence/implementation/startup/before-real/check-startup/startup-check.json)。

[产品改动](evidence/implementation/startup/product-change.patch)仅涉及 App 和中英文文案：自动保存与回放记录不再等待旧会议恢复；只有当前保存状态仍为空闲、且没有新内容时，迟到的旧会议才可自动打开。这也保护了用户已经画过、又删除最后一个对象的空白会议。恢复等待期间显示“正在打开上次的会议…”，新编辑开始后显示正常保存状态。沿用已有保存器、任务代次和会议 ID 检查；原生退出、托盘和全局快捷键仍等待初始恢复完成。

| 验证 | 结果与证据 |
| --- | --- |
| 生产启动与字体 | [58 项检查](evidence/implementation/startup/final-background/check-startup/startup-check.json)全部通过，无页面脚本异常。真实字体请求挂起时，欢迎层约 2011ms 后消失，工作台使用回退字体；2 个字体请求失败且启用减少动态时约 36ms。`/`、Space、跳过按钮及覆盖在背景设置按钮上的点击均未触发后台操作。 |
| 欢迎层兜底 | [独立组件 7 项](evidence/implementation/startup/component-measured/check-splash/splash-check.json)通过。Wordmark 永不报告就绪时，DOM 观察器测得覆盖层约 3014ms；自动化等待为 3322ms，两者分别记录。三秒是计时预算，包含主线程调度误差，不是主线程卡顿下的硬实时保证。 |
| 慢恢复期间输入 | 中英文实际提交两句话，旧读取尚未交付时已写入真实 IndexedDB，并分别保存 2 个白板节点和 2 个回放步骤。提交后要么已保存，要么关闭保护已生效；旧原件内容保持不变。[英文记录](evidence/implementation/startup/final-background/check-startup/typed-en-records.json)、[中文页面](evidence/implementation/startup/final-background/check-startup/typed-zh-pending.png)。 |
| 空白会议与草稿 | 自由编辑实际绘制矩形、保存，再删除最后一个对象。迟到的旧读取不会替换这场已编辑过的空白会议，重开后原样保留。未发送草稿则随正常旧会议恢复保留，显式发送后追加到旧会议。[空白记录](evidence/implementation/startup/final-background/check-startup/draw-delete-records.json)。应用撤销会留下编辑说明，所以此处使用“删除最后一个对象”验证真正无内容的分支。 |
| 恢复反馈与窄窗 | 中英文等待状态在 1280、720、640px 窗口中可见，外壳无横向溢出。[中文窄窗](evidence/implementation/startup/final-background/check-startup/typed-zh-loading.png)。未提交内容时不会生成空会议记录。 |
| Linux 原生保存与恢复 | 最终二进制正常打开此前的 13 页测试会议，再通过真实界面新建会议并键入两句话，显示 [Saved](evidence/implementation/startup/native-saved.png)，对应[真实 JSON](evidence/implementation/startup/native-saved-meeting.json)。无板书模型时保留原话和未绘制原因。正常退出后重新启动，[两句均恢复](evidence/implementation/startup/cold-restored.png)。原有会议与 13 个底图不变，全部 15 个记录/素材文件在两次退出后哈希一致；两次应用退出码均为 0。[6 组原生核对](evidence/implementation/startup/native-check.json)通过，私有桌面已清理。 |
| 回归与构建 | [最终 311 项 Node/Eval](evidence/implementation/startup/final-node-tests.txt)、[前端构建](evidence/implementation/startup/final-build.txt)和 [Linux locked/offline debug 构建](evidence/implementation/startup/final-build/native-build.txt)通过。保存逻辑修复后的[既有回归](evidence/implementation/startup/regressions.txt)覆盖启动退出等待、保存失败阻止退出、尾句保存顺序、撤销重做、19 项回放及异步任务隔离；这些回归运行后仅增加恢复提示，最终提示由本次 58 项和原生流程检查。 |

最终二进制 SHA-256 为 `2b2962a2910c5dc8dc5325e7ea52836a610746ecb796c4925794fd04d38ec3aa`，见[构建身份](evidence/implementation/startup/final-build/identity.json)。1423/1430 保持原预览进程；1430 的 `/assets/index-DIlO0nPA.js` 与磁盘和原生构建冻结产物相同，见[预览身份](evidence/implementation/startup/preview-identity.json)。[源码身份](evidence/implementation/startup/source-final.json)、[归档清单](evidence/implementation/startup/manifest.json)和[验证备注](evidence/implementation/startup/validation-notes.json)保留本批边界与自动化修正。

慢读取由浏览器延迟真实 IndexedDB 的完成回调来控制，模型回复也是 fixture；原生本批未注入磁盘读取延迟，不能将正常冷启动视为该故障的原生证明。未增加依赖或改变 ASR 策略，未提交或发布。用户暂无测试 Mac；真实音频/模型、输入设备、其余 G2/G3 与 G4 验收继续保留，整体重构尚未完成。
