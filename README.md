# boardcast

**说话，板书自己长出来。** 会议里大家只管讨论，白板上的手绘示意图跟着对话实时生长。全部在本机运行，不上传音频，不调用云端模型。

> Talk, and the whiteboard draws itself. Local-first live meeting diagrams: speech → a small local LLM emitting graph edits → deterministic layout → hand-drawn Excalidraw board. Runs on Linux, Windows (x64 / ARM64), and later macOS.

现在是早期原型：文字输入和示例会议回放已经能用，语音输入还在做。

板书有两种风格，左下角切换：

- **手写**：像有人拿马克笔在白板上写（深色主题下是黑板粉笔）。汉字按笔顺一笔一笔写出来，每个字有一点随手的歪斜和大小起伏。
- **铅笔 · 可编辑**：Excalidraw 的手绘风格，可以直接在板上拖动、修改、补画。

![手写 · 白板](docs/screenshot-ink.png)

![手写 · 黑板](docs/screenshot-chalk.png)

![铅笔 · 可编辑](docs/screenshot-board.png)

## 它怎么工作

```
麦克风 / 系统声音 ──► 流式转写 ──► 一句话说完
                                    │
              ┌─────────────────────┴─────────────────────┐
              ▼                                           ▼
        快车道（规则，<10ms）                       慢车道（本地小模型）
   提到已有节点就高亮；                          读「实体表 + 前两句 + 新的一句」，
   「画一下 A 到 B」直接出铅笔稿                  只输出几行图操作（语法约束）
              │                                           │
              └──────────────► 图状态 ◄────────────────────┘
                     （铅笔稿被模型确认就描成墨线，否则擦掉）
                                    │
        要点按区域摆放 + 流程图用 ELK 排版 → 手写笔迹 / Excalidraw 渲染
```

关键设计：

- **模型不画图，只发操作。** 排版和画风全部是确定性代码。操作分两类：
  - 流程图：`E 甲 -> 乙 : 「标签」`（连线）、`X`（删线）、`G`（分组）；
  - 要点板：`T`（话题）、`L`（条目，可以多层，也用来把条目挪到别处）、`Q`（待定问题）、`F`（公式）、`K`（备注）、`M`（划掉、圈出、下划线、星标）、`I`（从图标库里挑一个小图标）；
  - 通用：`R`（改名）、`A`（别名）、`D`（删除）。
  一块板上两类可以并存、互相连线。方案被否掉时默认划掉而不是擦掉，更像真实板书。
- **实体表管记忆。** 「它」「那边」「那个服务」靠程序维护的节点表和别名对上，模型只需要在列表里挑编号。
- **语法约束解码。** 每次调用按当前白板生成 GBNF，编号只能从已有节点里选，小模型写不出不存在的东西。
- **前缀缓存。** 系统提示词和示例对话固定在最前面，llama-server 复用它们的 KV，每句话只需重新读入一百个左右的 token。
- **手写是真的「写」出来的。** 不是套手写字体：汉字用 [hanzi-writer-data](https://github.com/chanind/hanzi-writer-data) 里按笔顺排好的骨架线，英文数字用 Hershey 单线字体，再交给 [perfect-freehand](https://github.com/steveruizok/perfect-freehand) 模拟马克笔的压感和收笔。新内容用骨架线做遮罩逐笔描出来；一轮新内容最多写 2.4 秒，写不完就整体加速，板书不会落后于讨论。
- **忙时合并。** 模型同一时间只跑一个请求，排队的句子下一轮合并处理，板书不会越拖越晚。

## 目前的评测

`eval/cases.json` 里有 17 段合成会议：10 段流程图向（纠正、指代、闲聊噪声、分组、改名、删方案、中英混说、口令），7 段头脑风暴向（拆模块、分工调整、方案对比、复盘、定价公式、流程图混待办、改条目）。在 RTX 3060 Laptop 上用一个 9B（Qwen 系微调）Q4 模型：

| 指标 | 值 |
|---|---|
| 节点 F1 | 0.81 |
| 连线 F1 | 0.67 |
| 话题召回 | 1.00 |
| 要点 F1（条目挂在对的话题下） | 0.73 |
| 标记命中（划掉、强调、问题、公式） | 0.69 |
| 单句延迟 p50 / p95 | 776 / 1784 ms |

已知问题：
- 这 17 段用例同时被用来调提示词，分数偏乐观；
- 「大量闲聊里夹着两句有用的」那段，模型把有用的两句也当成闲聊跳过了；
- 模型偶尔把标记打在错的编号上（比如星标打在话题上而不是条目上）；
- 2B 模型在新版用例上的分数以 CI 的评测结果为准，只用来看回归。

下一步：补一套不参与调参的测试集，再用合成数据给 2B–4B 的模型做 LoRA 微调，目标是让小模型接近 9B 的准确率。

## 本地运行

需要 Node 24+、pnpm、Rust（打包桌面版时）和 [llama.cpp](https://github.com/ggml-org/llama.cpp)。

```sh
pnpm install

# 1. 起一个本地模型（任意 GGUF；有显卡就加 -ngl 99）
llama-server -m path/to/model.gguf --port 8080 -c 4096 --jinja -np 1

# 2. 网页版
pnpm dev            # http://localhost:1420

# 3. 或者桌面版
pnpm tauri dev
```

跑评测：

```sh
pnpm eval -- --label my-model          # 报告写到 eval/reports/
node eval/run.ts --only login-flow -v  # 看某一段的逐句操作
pnpm test                              # 引擎单元测试，不需要模型
```

## CI

- `CI`：类型检查、单元测试、网页构建，然后在 Linux x64、Windows x64、Windows ARM64 上打桌面安装包。
- `Board eval`：改了 `src/engine` 或 `eval` 时，在 CPU 机器上用 llama.cpp 跑一遍评测，分数写进 job summary，用来看回归。
- `Release`：推 `v*` tag 时三平台打包，生成草稿 Release。

## 路线

- [x] 文字 → 图操作 → 手绘板书，评测和延迟统计
- [x] 要点板：话题、多层条目、问题、公式、备注、划掉 / 圈出 / 星标、小图标
- [x] 手写模式：按笔顺逐笔书写，白板马克笔 / 黑板粉笔
- [ ] 手写模式里的小图标、导出 PNG / SVG
- [ ] sherpa-onnx 流式转写（麦克风 + 系统声音）
- [ ] Tauri 外壳拉起 llama-server，首次启动下载模型
- [ ] 不参与调参的测试集；合成数据 + LoRA 微调小模型
- [ ] 增量排版，新节点出现时旧节点不跳
- [ ] 导出 Markdown / PNG / `.excalidraw`
- [ ] macOS 构建

## 许可

代码 MIT。

手写模式用到的汉字笔画数据来自 hanzi-writer-data（源自 Make Me a Hanzi 与 Arphic 字体），使用 Arphic Public License，许可文本会随数据一起放进 `public/hanzi/ARPHICPL.TXT`。Hershey 字体数据来自 hersheytext（MIT）。
