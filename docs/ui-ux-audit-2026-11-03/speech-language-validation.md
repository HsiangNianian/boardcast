# C61 续批：收音、输入检测与语音模型错误

2026-11-04，HEAD `4f9ab26`。接续模型下载提示，处理会前准备、输入预览、语音设置和底栏的语音错误。C61 和整体目标仍未关闭。

## 修改与复现

新增共享显示函数 `speechMessage`，对照实际 Rust/Swift 来源翻译 52 条固定消息、12 类带详情的前缀和采样格式错误。设备、路径、格式和底层诊断完整保留，未知消息与不符合格式的近似错误保持原文。识别仍依赖原生固定文本，原生格式变化时需要同步字典，代码已注明限制。

App 保存原始语音错误，在渲染时转换；权限错误、Apple 语种资源状态和自动识别失败因此能随界面语言更新。原始代码仍决定是否打开语音准备入口，也仍进入原有诊断记录。没有改变原生收音、音源选择、取消/停止或会议保存逻辑。会前准备和语音设置使用同一转换函数。

相同脚本检查前后生产构建：旧版 43 项操作保护断言通过，36 处文案中 15 处不符，最终文案断言按预期失败；新版 44 项断言、36 处文案全部通过。见[旧版结果](evidence/implementation/speech-language/baseline-browser/check.json)、[新版结果](evidence/implementation/speech-language/browser/check.json)及[本批补丁](evidence/implementation/speech-language/product-and-tests.patch)。

## 验证

| 范围 | 结果与边界 |
| --- | --- |
| 生产 App | 中英文启动错误、权限、未知设备诊断、Apple 准备入口、活动错误切换语言、运行中停止与尾句不完整提示、能力读取和模型选择失败均通过。浏览器注入原生 IPC/事件，不连接真实音源 |
| 恢复与数据 | 打开会前准备不收音；输入检测失败保留音源；显式重试和停止释放检测；能力错误阻止开始，恢复后显式开始成功。已有会议在失败启动和模型操作前后完全一致，没有自动下载模型 |
| 原有回归 | [输入预览](evidence/implementation/speech-language/input-preview-regression/input-preview-check.json)与[会前准备](evidence/implementation/speech-language/preparation-regression.txt)通过，覆盖无模型检测、静音/未知电平、迟到事件、超时、关闭等待释放、权限、开始前取消及窄窗布局 |
| Node / 构建 | [16 项定向 Node 检查](evidence/implementation/speech-language/node-tests.txt)、类型检查、播放器、主应用和 Linux Tauri debug 构建通过。单测校验固定消息确实存在于原生源码，未知/原型名称、复杂路径和嵌套诊断保持完整 |

测试编写时修正了切换语言后仍使用中文弹窗名称定位，以及能力刷新、停止尚未完成就读取结果的等待问题；最终前后构建共用修正后的脚本。详情见[运行说明](evidence/implementation/speech-language/harness-notes.txt)。没有为这些测试假设改变产品状态机。

## 真实 Linux 验证

独立应用标识 `games.mmstudio.boardcast.g2.speechlang`，隔离 XDG、D-Bus 和 Xvfb `:90`。应用进程实际继承不存在的 Pulse socket 与空 ALSA 配置，避免连接实体音源。通过 cua-driver 在真实会前准备点击“检查输入”，底层返回设备不可用，界面显示“无法打开麦克风：”并保留完整英文设备诊断；再次显式检测得到相同错误，仍显示尚未收音且重试可用。切换英文后实际错误保持原文。见[中文错误](evidence/implementation/speech-language/native-input-error-zh.png)、[英文错误](evidence/implementation/speech-language/native-input-error-en.png)及[12 项证据核对](evidence/implementation/speech-language/native-check.json)。

关闭会前准备后回到工作台，输入没有被开启；检查时没有打开的声音设备文件描述符。新建隔离目录仅有 WebKit 缓存和偏好，没有模型、会议记录或音频文件。原生语言单选的后台点击未生效，根据新截图在私有显示器前台完成切换，没有操作用户桌面。

## 构建与剩余边界

最终入口 `index-CeutEAnY.js`，SHA-256 `c3c560d18e58071caad12c0d14bc343370ff52d81e0f78a7135c7749e5c8a37f`；二进制 SHA-256 `376f014aae7d9cbd44cb90ab0c7fc6a76b7606f4d09eb2c174f0adf093f8d65b`，嵌入对应入口与样式。192 个源码/配置输入一致，私有构建项目的 23 个原生文件与工作树一致，见[身份记录](evidence/implementation/speech-language/identity.json)。

本批没有成功打开真实音源、运行语音模型或验证 macOS 权限/Apple SpeechAnalyzer；没有重跑全量 Node/Rust。真实音频、VoiceOver、安装更新、60 分钟现场与 G4 用户试用仍待验收，完整 C01–C68 范围保留。

本批私有预览和原生进程已清理，用户 1423/1430 服务仍为原 PID；1423 提供当前工作树的开发预览。未提交、推送或发布。见[清理记录](evidence/implementation/speech-language/cleanup.json)与[文件清单](evidence/implementation/speech-language/manifest.json)。
