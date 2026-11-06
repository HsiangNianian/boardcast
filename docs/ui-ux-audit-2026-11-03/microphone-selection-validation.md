# C09/C10 续批：指定麦克风失联后的恢复

2026-11-04，HEAD `4f9ab26`。本批修复正式收音与会前输入检测的设备选择规则不一致；完整 C10/G3 仍待验收。

## 问题与修改

正式收音的旧逻辑在找不到用户指定的麦克风时，继续打开系统默认输入；会前检测已经拒绝这种情况。用户也能直接从底栏开始或在收音中切换，因此会前检查不能保护所有入口。设置里的旧说明还明确承诺“会改用系统默认”。

两条原生路径现在共用 `microphone`：指定非空设备名必须匹配该设备，失联就返回既有错误；未指定设备或明确选择系统默认才读取默认设备。枚举失败保留原始诊断。已有来源路由、先停止再启动、取消、ASR 和会议保存保持不变。

中英文设置说明和缺失设备标签同步修改，错误给出“选择其他麦克风或系统默认后重试”。热切换失败保留所选设备并停止收音，重选后由用户显式再次开始。见[本批补丁](evidence/implementation/microphone-selection/product-and-tests.patch)。

## 已验证

| 检查 | 结果与范围 |
| --- | --- |
| 原生问题复现 | 先把旧选路表达式提取到正式调用的函数，保持旧行为；在真实 CPAL/ALSA 枚举下，已存在的指定设备与默认设备检查通过，不存在设备错误断言失败，因为旧逻辑返回了默认设备。见[修复前失败](evidence/implementation/microphone-selection/before-fix-rust.txt)和[当时源码](evidence/implementation/microphone-selection/before-fix/src-tauri/src/voice.rs) |
| 原生修复 | [同一检查通过](evidence/implementation/microphone-selection/microphone-rust.txt)：精确名称、默认/空名称、失联名称、大小写和尾空格。使用显式 Alsa host 和内容校验过的独立 null PCM 配置，仅枚举元数据，没有打开收音流 |
| 输入预览回归 | [两项 Rust 检查通过](evidence/implementation/microphone-selection/preview-rust.txt)：无读数与实际静音区分、按请求取消。需要虚拟 sink 和 WAV 的既有采集检查仍为 ignored，本批没有重跑 |
| 生产界面 | [52 项断言、40 处文案比较通过](evidence/implementation/microphone-selection/browser/check.json)。新增中英热切换先停止再尝试指定设备、失败保留选择、缺失标签、显式改默认且等待再次开始；原有权限、Apple 准备、重试、停止与会议内容保护继续通过。原生 IPC/事件受控，不代表实际采集 |
| 前端复现 | [旧生产构建](evidence/implementation/microphone-selection/baseline-browser/check.json)的 51 项保护断言通过，4/40 处新增恢复文案不符，最终文案断言按预期失败。原生静默回退由上面的 CPAL 检查证明，而非浏览器模拟 |
| 类型与构建 | 16 项定向 Node、类型检查、播放器、主应用和 Linux Tauri debug 构建通过；Rust 格式和 diff 空白检查通过。未重跑全部测试 |

已查看[中文缺失设备提示](evidence/implementation/microphone-selection/browser/zh-missing-microphone.png)和[英文提示](evidence/implementation/microphone-selection/browser/en-missing-microphone.png)。原有会议在启动、切换失败、重选及模型操作后保持原样，没有自动下载模型。

初始独立 ALSA 配置缺少读取设备提示所需的 control 元数据定义；补齐后再执行有效的修复前/后对照。原生预览 import 的换行格式修正后重新构建了最终二进制。具体边界见[运行说明](evidence/implementation/microphone-selection/harness-notes.txt)。

## 身份与剩余范围

入口 `index-CuFHmqom.js`，SHA-256 `920c52fc73ff138b5184afa17784b80a97e19739acb2bf6a5601309c89939c08`；原生二进制 SHA-256 `63f45399a7a3ffd5a10c7e2732b52f9ef2a052b99f05a10018894d2ae2cb50bb`。确认最终入口与样式嵌入二进制，193 个源码/配置输入与私有构建项目的 24 个原生文件一致，见[身份记录](evidence/implementation/microphone-selection/identity.json)。

本批没有启动原生窗口、运行语音模型、打开音频流或实测设备热拔插。macOS 权限、双路实际信号、系统默认变化、尾句和完整 G3/G4 仍待验收；不能据此关闭 C10 或整体目标。

私有 1438/1439 预览已停止，用户 1423/1430 保持原 PID，见[清理记录](evidence/implementation/microphone-selection/cleanup.json)。未提交、推送或发布。
