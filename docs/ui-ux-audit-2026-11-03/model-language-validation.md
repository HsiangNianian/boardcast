# C61 续批：内置模型状态与恢复入口

2026-11-04，工作树基线 `4f9ab26`。接续真实 Linux 窗口发现的英文“未安装模型”提示；C61 和整体重构目标仍未关闭。

## 修改与复现

工作台、设置中的本地模型说明、模型管理中的启动失败提示，以及图片导入等待模型启动的错误，现在共用 `desktop.ts` 的 `llmProblem`。它在显示时读取当前语言，转换未安装模型、安装中缺少服务、启动失败和意外退出；空错误使用已有的不可用提示。日志路径、退出状态和未知错误保留原文，不改变原生状态对象。

未安装模型的状态按钮直接打开现有模型管理。远程模型正在检查或已经可用时，状态按钮仍打开设置。打开面板不会下载、选择或删除模型。

[旧构建对照](evidence/implementation/model-language/baseline-browser/check.json) 中，42 处文案比较有 36 处与目标文案不符，包含中文英文混用、英文恢复指引变更和空状态未显示兜底文本；两种语言均需先进入设置才能管理模型。35 项内容保留、操作和远程入口断言通过，最终文案断言按预期失败。未知错误在旧、新构建中均保持原文。

本批没有更改 Rust 或下载、保存逻辑。固定原生文本目前是识别依据，后端更改返回文本时须同步更新，代码中已注明限制。见[六个产品文件及检查补丁](evidence/implementation/model-language/product-and-tests.patch)。

## 检查结果

| 检查 | 结果与范围 |
| --- | --- |
| 生产 App，中英文 | [37 项断言、42 处文案比较](evidence/implementation/model-language/browser/check.json)全部通过；覆盖六种状态、四个显示入口、现有状态实时切换语言、未安装模型直接入口、远程检查/可用入口，以及原会议字节保持。原生回复受控，不代表真实模型故障 |
| 图片导入等待模型 | 实际图片选择触发 `llm_ensure`，分别返回启动失败和模型已被移除；显示相应语言，原会议不变。后一种用例模拟上次轮询仍有视觉能力、启动时才发现模型丢失，不绕过首次启动时的视觉能力检查 |
| 模型管理回归 | [原有 16 项](evidence/implementation/model-language/models-regression/models-check.json)通过，含删除前保存屏障、取消、失败重试、重复提交保护和下载进度订阅 |
| Node、类型与构建 | [8 项 Node](evidence/implementation/model-language/node-tests.txt)通过，含嵌套括号退出状态、中文/空格/换行日志路径、近似但未知消息原样保留；类型检查、播放器、主应用和 Linux Tauri debug 构建通过 |

页面运行器关闭 context 和浏览器后退出 0。编写检查时修正的选择器、计数和错误前缀，以及首次原生构建 CLI 路径错误，记录在[运行说明](evidence/implementation/model-language/harness-notes.txt)，未计为产品失败。

## 真实 Linux 窗口

使用独立应用标识 `games.mmstudio.boardcast.g2.modellang`、XDG 目录、D-Bus 和 Xvfb `:90`，操作实际 Tauri 窗口。

- 英文首次启动显示未安装模型的恢复指引；点击后直接打开模型管理。
- 在设置切换中文后，同一原生状态立即显示中文；[工作台](evidence/implementation/model-language/native-workbench-zh.png)与[设置](evidence/implementation/model-language/native-model-settings-zh.png)文字一致，点击状态直接打开[模型管理](evidence/implementation/model-language/native-models-zh.png)。
- 后台语言点击未生效、分类页签未提供可调用的辅助功能动作；依据新截图在隔离显示器内完成前台点击并核对结果，没有操作用户当前桌面。
- 模型目录仍无文件，没有下载或运行模型。[8 项原生核对](evidence/implementation/model-language/native-check.json)及对应辅助功能快照已归档。

## 构建身份与边界

生产入口 `index-BalD8_9i.js`，SHA-256 `052488fc7f7c2d5015d2d3eb43205222fdb84a0779f2ca56fddba5a4559805b9`；原生二进制 SHA-256 `d5df41340b6b230cadff43d7719cda018e9d3955dafeb875db8b69e06c6ec04e`，包含该入口。190 个源码/构建输入在验证期间保持一致；私有构建项目的 23 个 Rust、Cargo 和基础配置文件与当前工作树一致，见[构建身份](evidence/implementation/model-language/identity.json)。

真实原生路径只验证未安装模型及语言/入口切换；未实际制造模型启动崩溃，未下载模型或执行推理。未重跑完整 Node/Rust 集合。模型目录中的通用英文名称、下载详情及其他原生错误、全部菜单/快捷键、macOS/VoiceOver 和 G3/G4 继续验收，不把本批视为全部模型文案或 C61 完成。

未提交、推送或发布。测试预览、原生窗口、Cua 会话与私有显示器已停止，用户 1423/1430 服务保持原 PID，见[清理记录](evidence/implementation/model-language/cleanup.json)。
