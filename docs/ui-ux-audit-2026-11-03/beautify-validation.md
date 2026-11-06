# C30 自动美化的冲突、超时与重试

2026-11-04，基线 `4f9ab26`。自动美化现在会等待模型生成结束；美化期间发生自由编辑时，旧结果会失效，避免撤销旧排版时误删新画的图形。只切换手绘/自由编辑视图仍可正常完成整理。排版器启动或编译卡住时，20 秒超时会结束后台任务并恢复重试；开始重试也会清除上一次失败提示。

复用现有保存流程的 `sceneContent` 判断用户内容是否变化，避免把可重建的视图投影当作编辑。修改集中在 App 的美化入口、现有内容比较函数的导出、排版器超时清理及两条中英文提示。没有新增依赖、会议字段或原生代码。显式美化仍会整理手绘布局，一次撤销恢复原先手动调整的坐标；自由编辑拥有的节点位置及独立图形保持。

| 检查 | 结果与边界 |
| --- | --- |
| 旧版对照 | [33 项检查中 6 项不符](evidence/implementation/beautify/before/beautify-check.json)。忙时可点击且没有说明；新增自由图形没有使旧结果失效；撤销后两只独立矩形只剩一只；排版器尚未启动时，等待 21 秒仍不能恢复操作。 |
| 最终美化流程 | [42 项全部通过](evidence/implementation/beautify/final/beautify-check.json)。27 节点、24 连线的长图实际交给本地 D2/TALA 整理，18 个流程节点的坐标有限且互不重叠；所有节点仍渲染，文字、标记、ID 和关系保持，一次撤销/重做、重复整理和重开均通过。见[长图截图](evidence/implementation/beautify/final/long-arranged.png)。 |
| 并发与所有权 | 实际鼠标拖动生成节点并新增两只矩形。等待排版时继续绘制，会得到已有的“板书有新变化”提示，旧结果不进入撤销记录；重新美化及一次撤销保留两次独立绘制。位置、尺寸、颜色、跨视图、保存重开和跨页旧结果隔离均经生产页面检查。只切换视图的补测也通过。 |
| 失败与超时 | 错误输入实际送入 D2 解析器，检查原布局保留及不重载重试。分别阻塞 Worker 的启动和编译消息，实际执行两次 20 秒超时；启动消息释放前已确认 Worker 结束。三个浏览器加载阶段共记录 13 个 Worker，均调用真实终止方法；没有替换成功的排版结果或加速计时。 |
| 中英文反馈 | 模型请求等待期间，美化禁用并说明原因；回放和空板也禁用。640×400 中文界面实际经历失败与键盘重试，恢复后旧错误消失，操作按钮仍在视口内；见[失败](evidence/implementation/beautify/final/chinese-failure.png)与[恢复](evidence/implementation/beautify/final/chinese-recovered.png)截图。 |
| 实际产物 | 下载 [Excalidraw](evidence/implementation/beautify/final/beautified-free.excalidraw)并核对两只独立图形；[回放 HTML](evidence/implementation/beautify/final/beautified-replay.html)在新建断网 context 中打开，选择第二张板并定位到最终步骤，确认自由编辑层，见[离线截图](evidence/implementation/beautify/final/offline-arranged.png)。本批没有重跑全部七种格式。 |
| 相邻回归与构建 | [双视图回归](evidence/implementation/beautify/scene-regression/scene-roundtrip-check.json)通过，覆盖 PC 改名/拖动、PAD 再编辑、模型冲突/采用、统一撤销、纯自由对象页面、重开和回放分支。[29 项定向测试](evidence/implementation/beautify/node-targeted-final.txt)、[完整 297 项 Node/Eval](evidence/implementation/beautify/node-full-final.txt)、[完整生产构建](evidence/implementation/beautify/build-final.txt)及 `git diff --check` 通过。 |

首轮冲突保护比较了完整生成场景，导致纯视图切换误报变化；[中间构建检查](evidence/implementation/beautify/initial/beautify-check.json)记录了这个失败，最终改用现有用户内容比较后通过。同一次中间运行还暴露成功重试后保留旧错误的问题，最终中英文检查已覆盖清除。测试自身先后修正了未先选中就拖动、两个 Undo 按钮未限定范围、过早读取保存状态及离线页未切到第二张板的问题，均与产品失败分开记录，见[测试说明](evidence/implementation/beautify/harness-notes.json)。

当前固定的 `@d2lang/d2` 0.1.34 在 `dispose()` 中等待启动完成，因此不能用它取消卡住的启动。超时分支直接终止该实例的 Worker，普通成功/失败继续使用公开的 `dispose()`。这处适配依赖当前版本的 Worker 字段，源码已有 `shortcut:` 注释；升级依赖时须重验启动和编译超时，待公开清理接口支持中断启动后移除。

证据归档包含 90 个文件、12,821,274 字节及独立[哈希清单](evidence/implementation/beautify/manifest.json)。最终入口 `index-wS2M7mzo.js`，SHA-256 `f38ed16a44d8079e0dbcfa0476c75cc996629c2ff3a3b201179991f835a106a1`；播放器 SHA-256 `85d00ed75cb412645e017011354841983a5fdfdf5eb14e38e5e59c4829d994dd`，均与实际服务字节一致，见[构建身份](evidence/implementation/beautify/identity-final.json)。1423 开发预览和 [1430 生产预览](http://127.0.0.1:1430/)保留原进程，检查浏览器均已关闭，见[清理记录](evidence/implementation/beautify/cleanup.json)。

本批只声明上述浏览器与 Node 验证。模型 HTTP 回复受控，没有实际音频或模型质量结论；未重建原生应用，Mac、真实输入设备、VoiceOver 与 G3/G4 仍未验。C27 其他复杂材料、C38 完整续会及全方案剩余准入继续，整体重构未完成。未提交或发布。
