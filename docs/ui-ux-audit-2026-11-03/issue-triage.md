# Issue 去重与处置记录

日期：2026-11-03；代码基线 `4f9ab26a46f07cc5575460fd7431f1a995e705cc`。创建前检查全部 **58 条 issue（31 open、27 closed）** 的标题、正文与验收范围；原始快照见 [existing-issues.json](existing-issues.json)。创建后再次读取 GitHub，确认下列问题已存在且为 open。连接器有读取权限但创建返回 403，实际写入使用用户已登录的 GitHub CLI。

## 新增问题

优先级是本次审查建议，不是已设置的 GitHub priority 字段。

| Issue | 优先级 | 已确认的证据 | 与已有问题的区别 |
| --- | --- | --- | --- |
| [#73 PC 编辑丢失](https://github.com/HsiangNianian/boardcast/issues/73) | P0 | PC 新增文字，导出中存在；PAD→PC 后画布和再次导出均丢失 | #9 是笔迹/底图一致性；#53 是动态导出。本项是编辑持久化 |
| [#74 弹窗键盘焦点](https://github.com/HsiangNianian/boardcast/issues/74) | P1 | Settings 初始焦点留背景、Shift+Tab 穿透、Escape 无效、/ 聚焦背景输入 | #19 是收音快捷键；#45 是父项。此处给出模态交互独立验收 |
| [#75 导出禁用条件](https://github.com/HsiangNianian/boardcast/issues/75) | P1 | 无模型的原话记录无法导出；有两页 PDF 时新增空页也封锁整场导出 | #23 的基础导出已关闭；#53 不涉及共用入口的错误门禁 |
| [#76 PDF 导入取消](https://github.com/HsiangNianian/boardcast/issues/76) | P2 | production preview 的导入去向条只有新建/追加，Escape 不取消 | #32 的选页/引用索引不包含放弃导入；#12 基础导入已关闭 |
| [#77 更新失败重试](https://github.com/HsiangNianian/boardcast/issues/77) | P1 | Rust 下载前 take Pending，失败不恢复；UI 再点只 install；源码证据加模拟前端调用，不是实际下载失败复现 | #17 基础更新已关闭；没有同样失败恢复验收。未执行真实安装 |
| [#78 编辑工具条越界](https://github.com/HsiangNianian/boardcast/issues/78) | P1 | 英文 1280×800 选左侧节点，Rename 的 x=-74.609/right=-8.969，完全在视口外 | #14 是编辑基础能力；#74 是模态焦点。本项是白板操作的几何可达性 |

## 复用已有问题，不重复新开

本轮汇总已[追加到 #45](https://github.com/HsiangNianian/boardcast/issues/45#issuecomment-6598261798)，父 issue 保持开放。
保存命令拒绝后没有可见失败提示的前端补测，也已[追加到 #61](https://github.com/HsiangNianian/boardcast/issues/61#issuecomment-6598394598)；这不是实际磁盘损坏或已丢失数据的声明。

| 发现或计划输入 | 复用 | 处理 |
| --- | --- | --- |
| 保存失败不可见、失败仍切会 | [#61](https://github.com/HsiangNianian/boardcast/issues/61) | 保持现有跟踪，重构 P0/P1 首批处理 |
| 旧模型结果串入新会议 | [#60](https://github.com/HsiangNianian/boardcast/issues/60) | 作为状态/异步生命周期基础门槛 |
| PC 编辑器固定中文、整体层级与视觉 | [#45](https://github.com/HsiangNianian/boardcast/issues/45) | 归入整体重构与国际化验收 |
| PC 不显示导入底图、笔迹表示差异 | [#9](https://github.com/HsiangNianian/boardcast/issues/9) | 保留已有兼容性范围，勿与 #73 混为一谈 |
| 动态 SVG 的顺序/一致性、GIF/WebM 缺失 | [#53](https://github.com/HsiangNianian/boardcast/issues/53) | 沿用原验收，不把生成文件当内容正确 |
| 真实 Mac 权限、切换、双路采集 | [#69](https://github.com/HsiangNianian/boardcast/issues/69)、[#34](https://github.com/HsiangNianian/boardcast/issues/34) | 本轮原生 Linux/模拟不能关闭它们 |
| Apple 双路语言、字幕时延/计量 | [#70](https://github.com/HsiangNianian/boardcast/issues/70)、[#71](https://github.com/HsiangNianian/boardcast/issues/71) | 保持采集/转写/生成独立状态，待 Mac 实测 |
| 摘要事实性与负责人归属 | [#68](https://github.com/HsiangNianian/boardcast/issues/68)、[#44](https://github.com/HsiangNianian/boardcast/issues/44) | 流程设计要求可溯源、可修订，不扩张 ASR 优化任务 |
| 远程返回不可解析时只有压缩错误栈 | [#72](https://github.com/HsiangNianian/boardcast/issues/72) | 复用错误阶段与安全诊断验收 |

## 没有立为产品缺陷

- dev server 的 PDF `Invalid workerSrc type` 在 production build + preview 不复现，属于本轮依赖软链接/开发环境边界。
- 扫描 PDF 两页的导入、分页以及有底图页面上的导出入口实测可用；未把它误报为“所有扫描件无法导出”。
- 源码提示的历史重命名/删除失败反馈、缺失附件恢复、更新前最终保存等风险保留在覆盖与方案中；没有运行证据时不伪称发生数据丢失。
- 不为第三方应用的风格偏好或硬件不可用新开 bug；把这些作为设计选择和发布验收条件。
