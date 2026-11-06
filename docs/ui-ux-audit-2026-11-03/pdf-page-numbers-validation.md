# C49 PDF 保留原板号

2026-11-06。整场 PDF 会跳过没有内容的白板，但此前按过滤后的列表重新编号：工作区第 2、4 张板变成 PDF 页眉的第 1、2 张，原话里的第 2、4 张引用没有跟着变化。若多页会议只有一张非空板，页眉还会省掉板号。

## 修复与范围

`meetingPdf` 在过滤时保留原索引，页眉使用会议中的板号；是否需要编号根据会议总板数判断。导出进度仍按实际输出的非空板计数。只改一处产品文件的 4 行，没有增加存储字段、依赖或翻译。唯一调用方是现有整场 PDF 导出入口；其余六种格式不改。见[本批补丁](evidence/implementation/pdf-page-numbers/product-and-tests.patch)。

## 浏览器与构建

| 范围 | 证据 |
| --- | --- |
| 修复前 | 同一生产预览完成五种会议的实际下载，[6 项断言失败](evidence/implementation/pdf-page-numbers/browser/baseline/check-pdf-page-numbers/report.json)。[原第四张板的旧页眉](evidence/implementation/pdf-page-numbers/browser/baseline/check-pdf-page-numbers/gaps-en-header-2.png)确实写成 `Board 2`。 |
| 修复后 | [25 项通过](evidence/implementation/pdf-page-numbers/browser/final/check-pdf-page-numbers/report.json)：首尾及中间空板、只有第三张非空、单板会议、中文页眉、纯原话会议；实际 PDF 页数、记录不变和脚本错误均检查。 |
| 文件内容 | 英文页眉用已安装的 `pdftoppm` 和 Tesseract 从实际 PDF 栅格读取；中文检查记录真实 Canvas `fillText` 参数并继续原样绘制，同时人工查看[中文第四张板页眉](evidence/implementation/pdf-page-numbers/browser/final/check-pdf-page-numbers/gaps-zh-header-2.png)。没有把 PDF 文本提取用于这些本来不可选择文字的图片页。 |
| 相关回归 | [七格式 92 项通过](evidence/implementation/pdf-page-numbers/browser/final/check-exports/export-check.json)，包括底图、自由对象、PNG/SVG、可编辑绘图、PDF 与离线回放。 |
| 构建 | TypeScript、生产前端、Linux 原生离线锁定构建通过。193 个产品/配置文件与构建快照一致，较本批开始仅 `meetingPdf.ts` 改变。独立播放器源码没有变化，复用上批已构建的播放器。见[构建身份](evidence/implementation/pdf-page-numbers/identity.json)。 |
| 预览 | 1430 更新为 `index-Dtt0h317.js`；11,575 个文件和 10 个入口 HTTP 资源核对一致，保留旧哈希资源供已打开页面使用。[预览再次运行 25 项通过](evidence/implementation/pdf-page-numbers/browser/user-preview/check-pdf-page-numbers/report.json)。 |

复跑需要已有 Playwright、Chromium、`pdfinfo`、`pdftoppm` 和带英文数据的 `tesseract`：

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright \
node scripts/ui/run-checks.cjs http://127.0.0.1:1430 \
  check-pdf-page-numbers.cjs check-exports.cjs
```

第一次基线运行把中文菜单文案写错；按现有文案修正测试后，完整基线仍有上述六项真实失败。初次测试和完整前后对照均保留，不把菜单定位错误计为产品缺陷。

## Linux 实际保存与恢复

最终二进制在独立标识、XDG、D-Bus、Xvfb 下运行。使用 Cua Driver 从真实会议库打开合成五板会议，当前第五张为空；经真实系统保存框导出 [native-pages.pdf](evidence/implementation/pdf-page-numbers/native/native-pages.pdf)。在点击 Save 前确认目标文件不存在，之后界面显示实际路径。

文件共三页：带底图的第 2 张板、仅自由图形的第 4 张板、两条原话。实际 PDF OCR 得到 `Board 2`、`Board 4`，原话页引用一致；[原生第四张板输出](evidence/implementation/pdf-page-numbers/native/pdf-page-2.png)、[保存反馈](evidence/implementation/pdf-page-numbers/native/explicit-save.png)及[文件核对](evidence/implementation/pdf-page-numbers/native/export-proof.json)可复查。

保存后，原合成记录当时字节不变。首次正常退出时，应用将内嵌 PNG 转为自有素材，并补齐由原话恢复的 history、布局、缩略图和自由场景预览，revision/时间随之更新；因此没有把首次退出算作文件字节不变。核对确认五页 graph、两条原话、自由对象以及 PNG 字节保持。随后复制实际写出的完整配置与数据冷启动，[会议恢复](evidence/implementation/pdf-page-numbers/native-cold/restored.png)，再次正常退出后文件 SHA-256 不变，见[冷启动核对](evidence/implementation/pdf-page-numbers/native-cold-proof.json)。

首次 GTK `set_value` 读回与可见文件名不一致，随后生成了默认名文件；这次操作不作为正确命名或显式点击 Save 的证据。后续改用可见字段的键盘输入与 Save 像素点击完成上面的独立保存。冷启动退出时 Cua 连接先被启动器清理，工具没有收到响应；启动器记录应用退出码 0，并确认全部私有进程结束。[操作说明](evidence/implementation/pdf-page-numbers/investigation-notes.json)保留这些边界。

两次运行登记的 40 个测试进程均结束，临时 1443 预览已关闭，用户 1423/1430 保留。本批未采集音频或运行模型，也未重复全量 Node/Eval 或视觉矩阵。Windows ARM/Adreno、macOS 实机和完整 G3/G4 继续保留；未提交、推送、发布或生成 Windows 安装包。
