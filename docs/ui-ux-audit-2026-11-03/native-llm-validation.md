# C04/C17 Linux 本地板书模型与视觉组件

2026-11-05，HEAD `4f9ab26`。真实模型流程发现并修复三个问题：报错入口打开了通用设置；删除当前模型后，自动回退的模型没有显示为正在使用；保留的视觉组件在配套板书模型删除后从列表消失。本批 [34 组捕获证据核对](evidence/implementation/native-llm/evidence-check.json)通过，包含真实本地生成、4B/9B 切换、权限失败恢复、组件清理和冷启动。完整 C01–C68/G1–G4 仍未关闭。

## 修改范围

- `App.tsx`：本地模型缺失或报错时，点击状态提示直接进入模型管理。远程连接检查或远程模型可用时仍进入设置。
- `models.rs` / `llm.rs`：模型目录复用实际启动使用的 `pick_model`，保持既有选择与自动回退顺序；没有选择文件时也能标识所选模型。视觉组件有配套模型，或自身已经占用磁盘空间时，均保留管理条目。
- 扩展既有中英文生产页面检查和 Rust 模型测试。没有新增依赖、改模型文件名/哈希/公共下载源，或改变 SenseVoice、VAD、动态复核策略。

产品只改三个文件，测试两个文件；[本批补丁](evidence/implementation/native-llm/product-and-tests.patch)和[前后哈希](evidence/implementation/native-llm/changed-files.json)相对本批开始状态，不混入前面批次的改动。

## 真实运行结果

Linux Tauri/WebKit 在独立 XDG、D-Bus、AT-SPI、Xvfb/Openbox 与 Cua socket 中运行，使用真实原生 IPC、文件权限和 llama-server。模型使用本机已有、与目录登记哈希一致的文件副本，未使用模拟模型服务；测试会议及输入为合成内容。

| 场景 | 观察与结果 |
| --- | --- |
| 缺少内置服务的入口 | 旧版点击错误提示进入“通用与外观”；修改后直接显示“本地模型”和“重新启动”。恢复隔离安装目录中的引擎文件后，真实 4B 与配套视觉组件启动成功。见[入口截图](evidence/implementation/native-llm/fixed-error-models.png)与[引擎状态](evidence/implementation/native-llm/fixed-four-loaded-engine.json)。此故障注入不代表执行了真实安装包重装。 |
| 实际键入并生成 | 新建独立测试会议，键入三步流程并发送一次。实际本地推理产生 4 个节点、2 条连线；一条原话、模型操作和本地来源写入会议。单次记录耗时 3,653 ms，不作为性能或模型质量结论。见[输入与落盘核对](evidence/implementation/native-llm/generation-proof.json)、[会议 JSON](evidence/implementation/native-llm/generated-meeting.json)和[冷启动白板](evidence/implementation/native-llm/final-cold-restored-board.png)。 |
| 4B 切到 9B | 由“使用这个”实际切换；4B 进程退出，选择文件变为 9B，新服务加载真实 9B，健康接口返回 200。9B 未配置视觉组件，没有 `--mmproj`。见[9B 引擎记录](evidence/implementation/native-llm/switch-nine-loaded-engine.json)。 |
| 当前 9B 删除失败 | 将自有模型目录设为只读，触发真实 EACCES。文件、选择和确认框保留；后台重新加载 9B，恢复健康状态。见[错误截图](evidence/implementation/native-llm/nine-delete-error.png)与[恢复记录](evidence/implementation/native-llm/nine-delete-recovery-proof.json)。 |
| 原确认框重试与自动回退 | 恢复目录权限，在原确认框重试；9B 和选择文件被删除，剩余 4B 与视觉组件实际启动。旧目录清单却仍提供“使用这个”；修复后，无选择文件的冷启动会正确显示 4B 正在启动/正在使用。见[修复前](evidence/implementation/native-llm/fallback-four-wrong-selection.png)、[修复后](evidence/implementation/native-llm/selection-fixed-loaded.png)。 |
| 删除最后一个板书模型 | 确认框说明视觉组件会保留；删除后服务退出，672,423,616 字节的真实组件留在磁盘。旧版隐藏了该条目；最终构建显示组件、配套模型未安装提示和删除按钮，实际单独删除成功。见[旧目录证据](evidence/implementation/native-llm/orphan-vision-before-proof.json)、[最终目录状态](evidence/implementation/native-llm/orphan-fixed-manager.json)与[删除核对](evidence/implementation/native-llm/orphan-vision-delete-proof.json)。 |
| 使用中的视觉组件删除失败 | 重新准备独立 4B/视觉副本并启动最终构建。真实 EACCES 后保留确认框，4B 服务携带视觉组件重新就绪，两个文件的 SHA-256 不变。见[错误截图](evidence/implementation/native-llm/vision-delete-error-state.png)和[恢复记录](evidence/implementation/native-llm/vision-delete-recovery-proof.json)。 |
| 视觉组件删除重试 | 在原确认框直接重试；只移除视觉文件，4B 文件保留。新服务无 `--mmproj`，健康接口返回 200，目录显示组件未安装和下载入口。见[最终目录](evidence/implementation/native-llm/vision-removed-ready-manager.png)与[引擎/文件核对](evidence/implementation/native-llm/vision-delete-success-proof.json)。 |
| 最终冷启动 | 没有选择文件、只剩 4B，仍自动加载正确模型；缺失的视觉组件没有重新出现，测试会议恢复原话和白板。最终服务健康接口返回 200，见[冷启动核对](evidence/implementation/native-llm/final-cold-proof.json)。 |

原有五句合成会议、生成完成后的一句新会议及关闭远程服务的配置，在后续模型操作、退出和冷启动中逐字节不变；原会议在新会议生成期间也未改变。见[操作前](evidence/implementation/native-llm/persistent-before.json)、[新会议快照](evidence/implementation/native-llm/generation-proof.json)和[最终哈希](evidence/implementation/native-llm/persistent-after-final.json)。

## 模型、构建与检查边界

实际使用 Qwen3.5-4B-Q4_K_M、Qwen3.5-9B-Q4_K_M 和 4B 的 F16 mmproj；三个原件的大小和登记 SHA-256 匹配。本批通过 `cp --reflink=auto` 创建不同 inode 的私有副本，没有硬链接或修改用户模型。原件最终重新计算哈希并核对大小、mtime、inode，均不变，见[原始元数据](evidence/implementation/native-llm/source-models.json)及[结束核对](evidence/implementation/native-llm/source-models-after.json)。两场测试之间补回 4B/视觉副本属于[停机准备 fixture](evidence/implementation/native-llm/private-fixture-restored.json)，不计作 UI 下载。

引擎为仓库指定的 [b11954 Linux x64](evidence/implementation/native-llm/engine-version.json)，使用 RTX 3060 Laptop 6 GB 对应的 Vulkan1；保留默认自动显存适配。实际健康接口、进程参数、模型列表和日志共同作为启动证据，没有用配置内容代替运行结果。

| 构建 | 本批实际覆盖 |
| --- | --- |
| 既有 `99ee3601…` | 复用前批缓存，复现错误入口，并验证恢复引擎后的真实 4B 加载。 |
| [入口修复 `a7cefb6e…`](evidence/implementation/native-llm/fixed-build/identity.json) | 直接模型管理、实际生成、4B/9B 切换、9B 删除失败/重试及 4B 自动回退。 |
| [选择修复 `45531e3f…`](evidence/implementation/native-llm/validated-build/identity.json) | 4B 自动选择的冷启动、最后一个板书模型删除和隐藏组件复现。 |
| [最终构建 `d40f3932…`](evidence/implementation/native-llm/final-native/identity.json) | 保留组件的可见/删除、使用中视觉组件失败恢复/重试、无视觉组件的 4B 冷启动。没有在此二进制上再次跑完整 9B 流程。 |

最终原生 SHA-256 为 `d40f39324139ef324bb7f1129b7b4a4ed7c6a16821b0ae48d0292ddf5e8a2f3c`。三个新构建共用前端 `index-kZq6sIhJ.js`，SHA-256 为 `005d8a9b7c142caa7f5cdf440359b438a52e113c084c9294ef6cf324c4e145dd`；最终 197 个构建输入与结束时一致。

- [生产浏览器检查](evidence/implementation/native-llm/browser-final/check-model-language/check.json)：51 项、42 处文案比较、12 个直接入口全部通过；旧版 10 个模型错误入口失败已保留。远程 checking/ready 和缺少/报错本地模型的组合仍打开设置。这里使用受控原生状态回复，真实模型证据来自上表。
- [Rust 定向检查](evidence/implementation/native-llm/rust-model-tests.log)：10 项通过，包含显式选择、所选文件消失、无选择文件回退、仅剩视觉文件不作为板书模型；原有取消、续传和写入失败回归通过。真实 2.7 GB 下载用例仍 ignored，本批没有再次下载 LLM 权重。
- 类型检查、播放器/主应用构建、locked/offline Linux debug 构建和 diff/格式检查通过。Rust 检查后仅作了过滤条件的 rustfmt 排版，并重新构建最终二进制；没有重复无关 Node/Eval 全量检查。

六次应用均先通过原生菜单正常退出，确认应用及当前模型服务已经结束后才停止私有桌面。46 个登记进程（含 10 个模型服务）已结束，没有残留私有环境进程；[清理记录](evidence/implementation/native-llm/final-cleanup.json)保留每次应用 exit 0。缓存文件保留供复核，权重、二进制和浏览器资料目录不进入证据归档。

验证过程中有标题/按钮文案断言纠正、AX 未提供输入值后改用已有截图确认、后台滚动不可用后使用私有桌面前台输入，以及一份中间 AX JSON 被同名文件快照覆盖。对应操作均先检查已发生的结果，再执行未完成步骤，没有因观察失败重复提交；详见[运行说明](evidence/implementation/native-llm/harness-notes.json)。部分截图包含 Cua 光标覆盖层。

`python3 docs/ui-ux-audit-2026-11-03/evidence/implementation/native-llm/check-evidence.py` 只核对捕获记录和[归档哈希](evidence/implementation/native-llm/manifest.json)，不会启动应用、下载或删除模型。[1430 生产预览](http://127.0.0.1:1430)返回同一前端，1423 开发服务和两份 HTML 均可访问，原预览进程保留。没有安装应用、提交 Git、推送或发布。

本批未验证真实图片理解效果、最终二进制上的完整 9B 重跑、其他 GPU/CPU、Windows 文件占用、物理音频和签名安装更新。用户暂无测试 Mac，Mac/G3 与 G4 新用户任务试用仍保留；不以这批模型流程替代完整准入。
