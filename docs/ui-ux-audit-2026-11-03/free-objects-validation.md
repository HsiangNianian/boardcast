# C31 自由对象与第一笔绘制

2026-11-06，基线 `4f9ab26`。本批补齐当前支持的 12 种对象的代表样本，并修复空白会议第一笔被页栏挤短的问题。C31 的全部编辑状态、并发组合及跨平台验收仍未完成。

## 修复

空板开始绘制时，首次 `onChange` 让页面变为非空，页栏随即出现，画布在指针仍按下时右移 80px。实际从 `(350,210)` 拖到 `(500,310)`，旧版保存的矩形宽度只有 70，预期为 150。删除最后一个自由对象也会改变画布原点。

`App.tsx` 复用一个 `showPages` 条件，让自由编辑从空板起就显示现有页栏；容器宽度与页栏使用同一条件。没有增加绘图坐标补偿或新状态。见[修复前记录](evidence/implementation/free-objects/browser/baseline/failure.json)、[失败断言](evidence/implementation/free-objects/browser/baseline/report.json)和[本批产品差异](evidence/implementation/free-objects/product.patch)。

## 浏览器生产构建

新增 [check-free-objects.cjs](../../scripts/ui/check-free-objects.cjs)，使用真实画布输入、文件选择、IndexedDB 与实际下载；仅模型回复为合成内容。[26 项检查](evidence/implementation/free-objects/browser/final/check-free-objects/report.json)通过：

- 实际创建矩形、椭圆、菱形、箭头、直线、笔迹、文字、图片、框架和网页嵌入共 10 类对象。箭头双端绑定、框架成员与图片附件均核对。
- 第一笔完整保存 150×100；创建、删除最后一个对象时画布原点不变，撤销恢复。
- 仅自由对象的白板，经过双视图、切页、重新打开、模型增量、Excalidraw 导出再导入后，保留对象字段、身份和关联。
- 额外导入 `magicframe` 与 `iframe`，验证全部 12 类对象经编辑器初始化、重新打开、模型增量及再次导出仍保留字段。`iframe` 使用静态合成 HTML，没有脚本；外部嵌入使用保留域名，无实际站点访问。

比较保留原始 `seed`、样式、坐标、文件 ID、绑定、链接和 customData，仅排除内部版本/更新时间/排序索引；JSON 无法表达的 `undefined` 与负零按序列化结果比较。没有通过删除业务字段让检查通过。

原有模型与拖动并发 18 项、文字编辑 30 项、回放 19 项、场景双视图及工作区检查通过。44 项场景、附件、导入和步骤测试、类型检查、生产前端及 Linux 原生离线锁定构建通过，见[检查汇总](evidence/implementation/free-objects/checks.json)。

## Linux 原生窗口

最终二进制 `49189178c8353e38d57e6d4ff05defe10318031092c9cf964e5ee8f53bbe5fc3`，使用独立 XDG、D-Bus、Xvfb、应用标识和 Cua socket；未访问用户会议、模型或物理音频。

1. 在真正空白窗口选择矩形，实际拖动 150×100；原生会议文件保存相同尺寸。删除最后一个对象，页栏保持；实际 Ctrl+Z 恢复。见[第一笔落盘](evidence/implementation/free-objects/native-status/first-drag-record.json)及[撤销窗口](evidence/implementation/free-objects/native-status/undo-deletion.png)。
2. 通过系统文件选择器导入浏览器实际导出的 16 个对象，其中 12 个自由对象分别覆盖 12 类类型，另 4 个为两个生成节点及其文字。双视图显示正常，数据逐字段保持，原矩形会议不变。见[可编辑视图](evidence/implementation/free-objects/native-status/imported-ready.png)。
3. 通过系统保存对话框导出 Excalidraw；所有自由对象字段及内嵌图片数据保持。见[实际保存文件](evidence/implementation/free-objects/native-status/boardcast-20261106-1134.excalidraw)。
4. 正常退出后，用实际产生的数据与配置冷启动；切回自由编辑，图片加载完成、静态 iframe 内容恢复。见[冷启动窗口](evidence/implementation/free-objects/native-cold/restored-ready.png)。两份会议、三个素材文件在退出、冷启动和只读切视图后逐字节一致，见[最终文件核对](evidence/implementation/free-objects/native-cold/after-quit-proof.json)。

两次应用均退出 0；40 个登记进程均已结束。第二次退出的 Cua 客户端因服务随应用关闭返回 1，应用退出码及进程身份独立核对，没有将观察端断开当作应用崩溃。原 1423/1430 预览进程保留，1430 更新为相同生产前端。

## 边界与复核

这是每种类型的代表样本，未穷尽所有旋转、裁剪、层级、复杂框架及并发编辑组合。Linux 原生实际创建了矩形，其余类型经系统文件导入验证；本批原生没有运行模型，模型增量属于生产浏览器的受控回复检查。嵌入对象的 HTML/链接和编辑视图保持，手绘视图使用 Excalidraw 标准占位图，不宣称离线复现任意网站。其他视觉导出格式与外部嵌入站点仍需单独验收。

测试过程修正了 Chromium 文件选择器检测、空心矩形的点击位置、JSON 中缺失字段/负零和初始化脚本误作用于沙箱子 frame 的断言；记录见[测试说明](evidence/implementation/free-objects/investigation.json)。没有据此修改产品逻辑。复核脚本 `python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/free-objects/check-evidence.py` 只读取归档，不启动应用。

Windows ARM 安装包/实机、Mac、物理音频与完整 G3/G4 仍保留；本批没有提交、推送或发布。
