# C32 原文件页码与紧凑选择器

2026-11-06。本批补齐方案 §5.3 的原文件页码：导入三页 PDF、跳过第二页参考文献后，第二块板书应标明「原文件第 3 页」。此前导入器和会议文件已经保留正确页码，界面只显示板书顺序。

## 改动

- 页栏缩略图、当前页位置、紧凑页选择、内容列表和离线 HTML 回放复用已有 `graph.underlay.no`。中英标签区分板书序号与原文件页码；没有有效原页码时省略，不根据板书顺序推算。
- 页栏与回放导航保留原有按钮名称，以 `aria-description` 补充来源；缩略图页码至少 12px。
- 真实 Linux WebKit 的 720×480 窗口暴露出选择器被矮窗口规则压到 28px、文字下半部裁切的问题。移除这一处高度覆盖，沿用已有的 34px；新增页按钮和粗指针规则保持原行为。
- 产品涉及 8 个文件，没有存储格式、导入规则、模型、ASR 或依赖变化。见[本批补丁](evidence/implementation/page-sources/product-and-tests.patch)和[构建身份](evidence/implementation/page-sources/identity.json)。

## 浏览器与构建

| 范围 | 结果 |
| --- | --- |
| 修复前 | [旧构建检查](evidence/implementation/page-sources/browser/baseline-final/check-page-sources/report.json)实际导入得到原页码 1、3，随后因页栏缺少来源说明而失败。 |
| 最终新增流程 | [18 项通过](evidence/implementation/page-sources/browser/browser-complete/check-page-sources/report.json)：真实 PDF.js 导入、IndexedDB 保存、缩略图/位置/两种选择器、紧凑高度和键盘、实际离线 HTML 下载、冷重载、空白新页及实时切换中文。不模拟导入器或存储。 |
| 最终相关回归 | 内容键盘导航 47 项、圈选反馈 34 项通过。圈选检查新增三条请求排队时离开并返回原页，仍要求各句保留原目标。见[检查汇总](evidence/implementation/page-sources/test-summary.json)。 |
| 最终布局 | [16 组通过](evidence/implementation/page-sources/layout-final/report.json)：中英 × 明暗 × 1280×800、900×600、720×480、真实 200% 缩放。缩放使用 `chrome.tabs.setZoom`，1280×800 窗口对应 640×400 CSS 视口，DPR 为 2；来源可见、字体至少 12px、选择器至少 34px，无横向溢出。 |
| 初版相关回归 | 30 项针对性 Node 检查、92 项七格式导出、69 项语言、55 项长原话阅读、17 项生成翻页、8 项底图翻页及 4 项内容列表通过。随后仅修正选择器高度并调整测试；没有把这些结果计为最终构建全部重跑。 |
| 构建 | TypeScript、播放器与主前端、Linux 原生构建通过；193 个产品/配置文件与最终构建快照一致。最终播放器和主前端均包含高度修正，见[源码快照](evidence/implementation/page-sources/source-final.json)和[原生构建日志](evidence/implementation/page-sources/native-build-complete.log)。 |
| 用户预览 | 1430 更新为 `index-DwvWwQOs.js`；11,575 个构建文件逐字节核对，10 个入口资源 HTTP 字节一致。新浏览器上下文再次通过全部 18 项新增检查。见[产物核对](evidence/implementation/page-sources/preview-publication.json)和[预览检查](evidence/implementation/page-sources/browser/user-preview/check-page-sources/report.json)。 |

复跑生产页面检查前应同时构建播放器和主前端：

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright \
node scripts/ui/run-checks.cjs http://127.0.0.1:1430 \
  check-page-sources.cjs check-content-navigation.cjs check-circle-feedback.cjs
```

初次新测试的 IndexedDB 初始化时序、语言控件定位和 `line-height: normal` 测量错误均已修正。原内容导航测试还曾早于动画帧检查 Escape 返回焦点；原代码原样重跑通过，测试改为等待编辑按钮收到焦点后再断言正确目标。失败日志与[调查说明](evidence/implementation/page-sources/investigation-notes.json)保留，没有用这些测试错误替代实际缺陷的证据。

## Linux 原生窗口

使用独立应用标识、XDG、D-Bus、Xvfb 和 Cua Driver，通过真实 GTK 文件选择器导入合成 PDF，没有模型请求或音频采集。

1. [实际导入预览](evidence/implementation/page-sources/native/preview-ready.png)显示三页文件、两块板书及一页跳过的参考文献；保存文件的来源页码为 1、3。
2. 初版 [720×480 截图](evidence/implementation/page-sources/native/compact-stable.png)及无障碍边界记录确认 28px 控件裁切文字。中间构建修正至 34px，并通过实际 Home/End、Enter 选择两页，见[第一页](evidence/implementation/page-sources/native-final/first-selected.png)和[第二页](evidence/implementation/page-sources/native-final/second-selected.png)。
3. 完整重建播放器与主前端后，最终原生二进制冷启动恢复第二块板书、原文件第 3 页；[宽窗口](evidence/implementation/page-sources/native-complete/restored.png)和[紧凑窗口](evidence/implementation/page-sources/native-complete/compact.png)均已检查，紧凑标签完整可读。内容页选择的原生无障碍树包含来源标签，但控件当时位于滚动区域外；其可见操作流程由浏览器检查覆盖，不算作原生可见性证明。
4. 三次运行的实际会议记录 SHA-256 均为 `8adefb3710f516862b0b73f6654440ed414d762d8040a224c66e032f541f9499`；最终应用正常退出，会议数据保持。见[最终记录](evidence/implementation/page-sources/native-complete/restored-record.json)和[清理核对](evidence/implementation/page-sources/cleanup-proof.json)。

中间启动器曾以 143 退出，未写出正常清理记录；原因未确定，应用当时已结束。随后仅终止匹配该私有运行目录的 11 个残留进程，见[人工收尾记录](evidence/implementation/page-sources/native-final/manual-cleanup.json)。初次和最终启动器均记录应用正常退出；全部私有测试进程与 1443/1444/1445 预览已结束，用户 1423/1430 进程保留。

## 剩余边界

Windows ARM [本地请求与预热修复](windows-arm-local-model-validation.md)仍在当前源码中，尚未生成新 Windows 安装包或在用户的 Adreno 机器复测。用户暂无测试 Mac。其他平台、VoiceOver、物理触屏和完整 G3/G4 继续保留；本批未重复全量 Node/Eval，也未提交、推送或发布版本。
