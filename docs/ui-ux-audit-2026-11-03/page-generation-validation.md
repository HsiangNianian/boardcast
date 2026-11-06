# C24/C32 生成中翻页与结果归属

2026-11-05。本批补齐方案中「生成不阻止浏览已完成内容」。此前任何原话处于等待状态，页栏、内容列表和翻页快捷键都会被锁住。直接解除按钮限制也不安全：正常生成使用的是当前可见页的 Session。

## 改动

- 复用已有的后台重试 Session，按原话的页码处理所有生成；同页保留顺序队列，其他页可以继续浏览、编辑或生成。完成结果更新原页和缩略图，不切换正在看的页。
- 应用模型结果前重新读取目标页，保留手动编辑、撤销、回放步骤和原话身份；切会取消全部活动页，旧结果不能进入新会。保存切换与回放期间的原有保护继续生效。
- 旧原话缺少页码时归属第一页。回归发现旧重试逻辑可能使用当前页，已在共同入口修正。
- 产品只改 `src/App.tsx`；没有新依赖、存储格式或 ASR 改动。[本批补丁](evidence/implementation/page-generation/product-and-tests.patch)包含新增页面检查、旧记录断言，以及为公共测试 runner 分配独立临时证据目录的调整。后者让要求第三参数的检查可直接复跑，也避免覆盖历史证据。

## 浏览器与构建

| 范围 | 结果 |
| --- | --- |
| 修复前 | [旧构建检查](evidence/implementation/page-generation/baseline/report.json)因生成中页栏被禁用而失败。 |
| 最终多页流程 | [17 项通过](evidence/implementation/page-generation/regression/page-generation/report.json)：迟到结果、同页排队上下文、后台失败重试、手改和撤销/重做、内容页选择、PageUp、翻页指令、新页、冷重载、两页取消及跨会迟到结果。使用生产 App、真实 IndexedDB 和可控制返回时机的合成模型回复；翻页指令由文字输入触发，不是实际 ASR 证明。 |
| 最终相关回归 | 原话状态 35 项（包括无页码旧记录）、七格式导出 92 项、回放浏览 19 项通过，见[汇总](evidence/implementation/page-generation/regression-summary.json)。 |
| 初版相关回归 | 重试、生命周期、绘图并发 18 项、文字编辑 30 项、纠错样本 21 项、发言人 4 项通过；Node/Eval 328 项通过。随后只修正缺失页码的默认值，再完成上面的最终检查；不把初版检查算作全部重新执行。 |
| 构建 | 最终 TypeScript、生产前端和 Linux 原生构建通过。195 个产品/配置文件与最终构建时的哈希一致，见[构建身份](evidence/implementation/page-generation/identity.json)。 |
| 用户预览 | 1430 更新为 `index-TO3REVfS.js`；11,575 个构建文件逐字节核对，10 个入口资源 HTTP 字节相同。新上下文再次通过多页 17 项、原话 35 项、本地原生请求 10 项，以及 720×480 鼠标/触屏入口与设置返回检查。见[发布核对](evidence/implementation/page-generation/preview-publication.json)和[实际预览检查日志](evidence/implementation/page-generation/user-preview-checks.log)。 |

复跑页面检查：

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright \
node scripts/ui/run-checks.cjs http://127.0.0.1:1430 \
  check-page-generation.cjs check-transcript-state.cjs check-local-transport.cjs
```

每个检查打印自己的临时证据目录。初次测试的按钮名称和手动原话计数错误、原生检查对浏览页持久化的错误预期，以及真正发现的缺失页码问题，均在[调查说明](evidence/implementation/page-generation/investigation/notes.json)中区分保存。

## Linux 真实模型

最终二进制在独立应用标识、XDG、D-Bus、Xvfb 中运行，使用已有 `llama.cpp b11954` 和 Qwen3.5-4B-Q4_K_M。没有打开物理音频。引擎和模型使用已有只读文件的硬链接，界面输入为合成文字；请求经过真实 Tauri IPC 和本地推理。

1. 首次真实预热和第一页 ALPHA 生成完成。在第二页 BRAVO 生成等待时返回第一页，原有板书可以继续阅读。
2. 为观察等待状态，短暂停止私有模型进程，并在 `finally` 中恢复；受控一轮耗时 4.565 秒。重试 BRAVO，同时在第一页追加 ALPHA，再翻到第二页，[实际窗口显示两条原话正在生成](evidence/implementation/page-generation/native/pending-read-bravo.png)。
3. 两条请求真实完成后仍停留第二页；BRAVO 只写第二页，ALPHA 增量只写第一页。见[第二页](evidence/implementation/page-generation/native/real-result-bravo.png)、[第一页](evidence/implementation/page-generation/native/stable-alpha.png)和[实际会议文件](evidence/implementation/page-generation/native/completed-record.json)。三条原话保持唯一身份，回放分别保存两步和一步。
4. 正常退出后冷启动，原话与两页板书恢复；[冷启动窗口](evidence/implementation/page-generation/native-cold/restored.png)和两次退出后的会议 SHA-256 一致。仅浏览不同页不修改会议内容，冷启动恢复最后一次内容保存的页；本轮没有声称第二次模型预热完成。

最早一次人为暂停跨过了上下文续接，实际持续约 376 秒，触发请求超时；界面保留原话和未提交草稿。恢复模型后，以上有时限的重试成功。这是故障注入过程，不作为正常模型耗时或 Windows ARM 故障的证据。[两次暂停记录及说明](evidence/implementation/page-generation/investigation/notes.json)均保留。

[17 项原生证据核对](evidence/implementation/page-generation/native-checks.json)通过；两个应用正常退出，37 个登记测试进程均结束。两个临时预览也已退出，原 1423/1430 进程保留。可对归档执行 `python check-native-proof.py <归档目录>` 重新核对数据；该脚本不重放 GUI。

## 剩余边界

Windows ARM [原生通道与预热修复](windows-arm-local-model-validation.md)在本批仍通过页面回归；没有生成新的 Windows 安装包，也没有在用户的 Adreno 机器上验证。用户暂无测试 Mac。真实语音翻页、其他平台/输入法/读屏组合、长会及完整 G3/G4 保留；本批未提交、推送或发布版本。
