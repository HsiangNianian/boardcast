# C38 用旧板开始新会

2026-11-05，工作树基线 `4f9ab26`。续会现在继承原会议的“仅本地”限制和术语；多页选择会移入键盘焦点，取消后返回原入口。原会议名、所选页码和新旧墨迹说明直接显示在白板上，关闭侧栏、缩小窗口或打开离线回放时仍可阅读。

自由编辑与导出的继承内容也使用淡墨。显示投影复用原有图形所有权和首个回放步骤，不改写保存的透明度，不增加会议字段或依赖。编辑旧自由对象后使用正常墨色，一次撤销恢复原位置及淡墨；改名与新标记独立判断。手绘旧文字按当前背景保留至少 4.5:1 的对比度；用户原本低对比度的颜色或透明度不再额外淡化。同词但不同坐标的标记不再误判为旧标记。

| 检查 | 结果与证据 |
| --- | --- |
| 旧版对照 | [24 项中 9 项不符](evidence/implementation/continue/before/continue-check.json)：选择器焦点、取消焦点、仅本地继承及重开、侧栏外来源、自由对象/文字淡墨、后续编辑的旧标签、离线来源说明。原件内容未改写，此项原本就通过。 |
| 续会主流程 | [最终 43 项](evidence/implementation/continue/final/continue-check.json)：从三页会议选择第二页、取消不创建会议、历史会议与当前单页两条入口、隐私与术语、模型修改及实际自由绘制、保存重开。原会议的完整序列化内容在多次续会和编辑后保持相同 SHA-256。 |
| 自由编辑与撤销 | 真实鼠标绘制矩形、键盘移动已有对象、一次 Undo，实际下载四份 Excalidraw 核对旧、新、移动及撤销后的透明度。保存状态始终保留原始透明度；[自由编辑截图](evidence/implementation/continue/final/continued-free.png)、[移动产物](evidence/implementation/continue/final/continued-moved.excalidraw)、[撤销产物](evidence/implementation/continue/final/continued-undo.excalidraw)。 |
| 图片与原件独立 | 第三页只有自由对象及资料图，实际加载、续会、保存、重开。源图与续会图分别存在各自会议的 IndexedDB 附件空间，数据 URL 字节完全一致。真实删除源会议后，原附件已删除，续会附件仍在。 |
| 缺失与重试 | 删除所选页的原始附件后，保留当前会议和选择器，明确提示恢复原图；补回后同一入口成功。选页过程中删除源记录也不会打开空白新会，恢复后可直接重试，旧错误会清除。见[缺图提示](evidence/implementation/continue/final/missing-source-image.png)。 |
| 预览缓存 | 历史来源页缺少可重建的 SVG 缓存不阻止续会。复制时丢弃这个失效缓存，防止它进入首个回放步骤、导致新会议无法保存；当前显示与离线历史仍按实际图形重建。中间失败保存在 [缓存恢复对照](evidence/implementation/continue/cache-failure/continue-check.json)及运行日志中。 |
| 窄窗与离线 | 640×400 中英文来源说明没有被顶部滚动区裁剪；[中文](evidence/implementation/continue/final/chinese-compact.png)、[英文](evidence/implementation/continue/final/compact-origin.png)。缩窗保留阅读位置，“显示整板”可重新适配；同时核对元素范围和实际截图的文字像素。下载的[回放 HTML](evidence/implementation/continue/final/continued-replay.html)在新建断网浏览器中打开，来源与新旧图形可见，见[截图](evidence/implementation/continue/final/continued-offline.png)。 |
| 回归与构建 | [双视图完整编辑](evidence/implementation/continue/regression-scene/scene-roundtrip-check.json)、[术语 30 项](evidence/implementation/continue/regression-glossary/glossary-check.json)、[模式说明 27 项](evidence/implementation/continue/regression-mode/mode-intro-check.json)、[七格式 92 项](evidence/implementation/continue/regression-exports/export-check.json)通过。另有[完整 301 项 Node/Eval](evidence/implementation/continue/node-full-final.txt)、[生产构建](evidence/implementation/continue/build-final.txt)和差异格式检查。 |

测试使用生产 App、真实 IndexedDB、指针/键盘编辑和文件下载。三页归档会议由受控数据构造，节点与自由矩形先经过实际 UI 生成；模型 HTTP 回复受控，不代表真实模型质量或录音验收。图片资源故障只作用于隔离浏览器的合成会议。

初轮来源说明放在顶栏时，窄窗会裁剪；离线播放器增加顶层行也压缩了内容区，最终统一放入白板正文的提示区域。测试自身修正了过早打开空数据库、附件夹具哈希格式、编辑器就绪前读取点击坐标等问题，详见[测试说明](evidence/implementation/continue/harness-notes.json)。截图预览中的笔迹疑问经原 PNG 像素核对确认完整，没有保留未经证实的滤镜改动。

当前 [1430 生产预览](http://127.0.0.1:1430/)及 1423 开发预览继续运行。实际服务入口和播放器字节与本地构建一致，见[构建身份](evidence/implementation/continue/identity-final.json)；归档包含 95 个文件、7,356,430 字节及独立[哈希清单](evidence/implementation/continue/manifest.json)。检查浏览器已关闭，见[清理记录](evidence/implementation/continue/cleanup.json)。

此浏览器批次为 G1 和持久化/故障测试，没有重建或安装桌面应用。随后 [Linux 原生续批](native-continue-validation.md)重新编译并运行当前 Tauri，完成 28 项证据核对和 5 项离线 HTML 检查；[后续 12 项复核](native-render-validation.md)定位了无标题原生确认框造成的截图黑块，并用真实键盘补齐正常 Save 的时间顺序证据。C38 核心 Linux 流程已有运行证据，不代表全方案 G2 关闭。Mac 暂不可用，真实音频/模型、触摸/笔、VoiceOver 和 G4 用户试用仍待验。C27 复杂资料及全方案剩余准入继续，整体重构未完成。未提交或发布。
