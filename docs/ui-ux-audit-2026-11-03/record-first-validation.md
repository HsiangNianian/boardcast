# C02/C24：先记原话，再恢复本地板书

2026-11-05，接续[原生会前检查](native-preparation-validation.md)和[原话状态](transcript-state-validation.md)。补齐真实离线转写、缺少板书模型时保存、配置后重试同一句及冷启动恢复。

## 改动与回归

四行产品修改均在 `src/App.tsx`：原话的「配置板书模型」、网页示例的连接入口和准备页的管理入口明确打开「板书与模型」；设置初始栏目直接采用入口参数，移除对准备页是否打开的隐式依赖。继续使用现有设置、保存和逐句重试，不增加状态、依赖或模型配置。见[本批补丁](evidence/implementation/record-first/product-and-tests.patch)和[文件哈希](evidence/implementation/record-first/changed-files.json)。

复用两份现有生产页面检查：修复前，中文原话恢复入口和网页示例入口均未直达对应栏目；修复后分别通过 62 项检查及 40 条语言对照、38 项示例检查。新增旅程覆盖中英文下先记录、保存不可用状态、恢复后重试同一 ID，以及转写/板书均缺失时禁止录音但保留手动输入。浏览器旅程模拟原生协议，不代替下面的真实识别。另通过既有 10 项本地通道、准备页和输入预览检查。见[页面结果](evidence/implementation/record-first/browser.log)、[组件结果](evidence/implementation/record-first/components.log)及归档的两组修复前失败。

TypeScript、前端生产构建、Linux 原生离线锁定构建及 `git diff --check` 通过。本批没有改变语音、推理或保存算法，没有重复此前完整 Node/Rust 套件。

## 真实 Linux 离线链路

使用独立 XDG、D-Bus、Xvfb、应用标识和网络命名空间；两个运行环境均只有 loopback、没有外部路由，外连探测返回 `ENETUNREACH`。保留用户原系统网络。二进制 SHA-256 为 `a137c490c4c8c3898a63be9848a8ce9074abffd6791bb5efa48cc1eeafb26495`，前端入口为 `index-CMcenoVY.js`，见[构建身份](evidence/implementation/record-first/identity.json)。

音频来自已有 Eval 的 `en-ami-00.wav`，33.72 秒、16 kHz 单声道；只通过 `paplay` 播放到专用虚拟 sink，再映射为界面明确选择的 `BoardcastRecordFirstQA` 麦克风。采集流始终匹配应用 PID 和该虚拟源，未录制实体麦克风。SenseVoice、tokens 和 Silero VAD 使用已有文件的独立副本，未安装可选说话人嵌入模型；SenseVoice、动态复核和 VAD 产品代码保持不变。见[音频样本信息](evidence/implementation/record-first/audio-sample.json)和[语音资源哈希](evidence/implementation/record-first/voice-assets.json)。

| 步骤 | 实际结果 |
| --- | --- |
| 转写和板书均缺失、离线 | 准备页录音不可用，「先打字」可用；键入的原话保存到真实会议文件，状态为模型不可用。启动、准备页和打字期间无采集流。见[缺件窗口](evidence/implementation/record-first/missing-ready.png)和[保存文件](evidence/implementation/record-first/offline-typed-meeting.json)。 |
| 转写就绪、板书缺失 | 明确选择虚拟麦克风后点击「先记录原话」，产生一个实际采集流。既有音频经生产语音链路形成 10 条独立原话，全部保存，并提示板书模型未配置。停止后采集流消失。见[录音窗口](evidence/implementation/record-first/audio-playing.png)和[记录证据](evidence/implementation/record-first/recorded-proof.json)。 |
| 配置入口与模型恢复 | 从原话进入对应设置，再打开模型管理。目录仍显示缺失时，将已有 Qwen3.5-4B GGUF 的独立副本放入测试目录，再点击该行下载按钮；原生路径校验已有缓存并启动模型，没有从外网下载新字节。NVIDIA RTX 3060 Laptop GPU 上实际完成预热推理。见[恢复入口](evidence/implementation/record-first/recovery-settings.png)、[模型文件](evidence/implementation/record-first/board-model-asset.json)和[实际推理日志](evidence/implementation/record-first/first-run/llama-server.log)。 |
| 重试已保存的原话 | 点击第一条原话的重试，真实本地模型生成 PhD students、Professors 两个主题。10 条原话的 ID、文字和音频元数据全部保持；另外 9 条待处理原话及此前手动输入的独立会议不变。见[白板窗口](evidence/implementation/record-first/recovered-board.png)和[保存核对](evidence/implementation/record-first/recovered-proof.json)。 |
| 正常退出与冷启动 | 同一二进制恢复白板、10 条原话和所选输入，完成第二次实际预热；未自动采集。启动和再次退出后两个会议文件哈希一致。见[冷启动](evidence/implementation/record-first/cold-ready.png)、[所选输入](evidence/implementation/record-first/cold-input-settings.png)及[文件核对](evidence/implementation/record-first/cold-proof.json)。 |

[31 项归档证据核对通过](evidence/implementation/record-first/proof.json)，可在证据目录运行 `python verify-evidence.py` 重新核对。测试辅助断言对持久化来源字段、原生选择框表示和命名空间编号复用的修正，单独记在[运行说明](evidence/implementation/record-first/run-notes.json)，未因此修改产品。截图保留原始 Cua 指针覆盖层。

两个应用均正常退出，20 个登记的测试进程已结束，最终隔离网络空间中无残留进程；两个专用音频模块已移除，系统默认输入/输出保持。见[进程清理](evidence/implementation/record-first/process-proof.json)和[音频清理](evidence/implementation/record-first/audio-cleanup.json)。原有 1423/1430 预览进程保留，1430 实际服务的 JS 与本批原生内嵌前端相同，见[预览核对](evidence/implementation/record-first/preview-proof.json)。

## 剩余边界

本批证明 Linux 虚拟麦克风的真实转写到本地板书恢复，不是实体设备、系统声音/双路、说话人识别或 ASR 准确率验收。仅缺转写但板书就绪、已有远程配置等其余组合以及外网首次下载仍保留。Windows ARM/Mac 实机、全方案 G3/G4 未完成；没有提交、打包 Windows 安装器或发布。[Windows ARM 本地请求修复](windows-arm-local-model-validation.md)仍需在原 Adreno 机器使用新构建验证。
