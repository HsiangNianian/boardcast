# C42：真正取消原生下载、重新导入与新会议恢复

2026-11-05，基线 `4f9ab26` 的当前工作树。此前“取消导入”只隐藏面板、拒绝迟到结果，Rust 下载仍在运行。本批沿用既有请求登记与取消机制，使 arXiv 下载接收导入任务的 AbortSignal；远程板书与论文下载共用原有请求表，各自按 ID 取消。没有新增依赖、下载地址选项或第二套任务管理器。

后续的 [Linux 界面异常恢复](native-webprocess-validation.md)重现了下文的 WebView 退出：Cua 元素 token 输入触发 `Crashed`，窗口范围输入对照未触发。已补上原生恢复/退出及界面异常时的请求清理；底层崩溃原因仍未确定。下文保留本批执行时的原始边界。

进一步的[材料导入焦点续批](native-import-focus-validation.md)修复取消后的键盘返回，重验真实连接取消与Rime输入；元素定向路径仍失败，另取得文字送达前 `Component.GrabFocus` 断开的记录，不能仅归因为提交表单。

| 路径 | 实际结果 |
| --- | --- |
| 旧版对照 | 真实 Linux 取消面板后，连接仍保持约 13.84 秒，到请求的 15 秒连接期限才释放。见[取消后观察](evidence/implementation/native-arxiv-cancel/baseline-cancel-observation.json)与[代理事件](evidence/implementation/native-arxiv-cancel/events.json)。预先取消的单测也先失败。 |
| 连接等待中取消 | 新构建显示下载状态，点击取消后约 0.58 秒关闭真实连接，面板消失，原会议文件不变。见[结果](evidence/implementation/native-arxiv-cancel/fixed-connect-result.json)。 |
| 正文传输中取消 | 代理已转发 66,083 字节真实 TLS 数据后暂停；点击取消后约 0.57 秒关闭连接，原会议文件不变。见[结果](evidence/implementation/native-arxiv-cancel/fixed-body-result.json)。时间包含自动化输入送达开销，不是平台性能承诺。 |
| 重新导入 | 再次输入 `1706.03762v7`，完整经过真实 arxiv.org HTTPS 下载和 PDF 解析，预览 13 张白板、跳过 2 页参考文献。确认前原记录不变；选择“New meeting”后保存为独立的论文会议，原会议的一句测试原话仍在。见[保存界面](evidence/implementation/native-arxiv-cancel/fixed-import-saved.png)与[两场记录](evidence/implementation/native-arxiv-cancel/after-commit-records.json)。 |
| 冷启动 | 同一最终二进制恢复论文会议；从会议库重新打开原会议，原话仍在。两场会议、锁文件及 13 个底图文件共 17 个文件字节不变。见[冷启动](evidence/implementation/native-arxiv-cancel/cold-result.json)与[文件核对](evidence/implementation/native-arxiv-cancel/after-cold-files.json)。 |

[19 项记录核对](evidence/implementation/native-arxiv-cancel/evidence-check.json)通过。[5 项 Node](evidence/implementation/native-arxiv-cancel/node-final.log)覆盖 ID 解析、提前取消、登记竞态、迟到结果、重试、字节解码、远程错误本地化以及取消下载不影响并发板书；[10 项 Rust](evidence/implementation/native-arxiv-cancel/rust-tests.log)通过，既有外部模型服务测试仍跳过。生产 App 的新增 4 条 arXiv 协议检查及既有原生外壳流程、21 项导入、21 项摘要回归均通过，见[浏览器记录](evidence/implementation/native-arxiv-cancel/browser.log)。类型检查、前端与原生构建通过，没有重复无关全量测试。

测试代理只监听回环地址，保留到 arxiv.org 的真实 TLS 隧道；它在 CONNECT 回复前或转发部分加密数据后停住，没有替换证书、论文或产品下载地址。代理配置只进入测试进程环境。全部测试使用独立数据目录、测试原话及公开论文，没有使用用户会议、模型或物理音频。

最早一次自动化尝试出现 WebKit 网页进程退出、宿主仍存活的现象，原因尚未定位；该次没有产生 arXiv 请求，单独保留为[失败的环境观察](evidence/implementation/native-arxiv-cancel/baseline-frontend-failure.json)，不计为取消证据。随后基线、修复版和冷启动的三个应用均正常退出。早期退出时清理驱动比菜单响应更早，CLI 因连接关闭报错；正常退出结论取自实际子进程退出码。最后的冷启动退出返回正常响应。30 个登记的测试进程及代理全部停止，原 1423/1430 预览保留。

最终构建的 194 个记录输入与当前工作树一致；原生哈希、预览产物和配置边界见[身份记录](evidence/implementation/native-arxiv-cancel/identity.json)，原始脚本、源码差异和产物索引见[归档清单](evidence/implementation/native-arxiv-cancel/manifest.json)。`check-evidence.py` 只核对已有记录，不会重演桌面操作。

本批不建立 Mac、真实音频、任意网络/代理及 G4 用户试用结论；上述一次 WebView 退出仍待定位。前次 [arXiv 验收](native-arxiv-validation.md)的其他边界继续保留。整体目标未关闭，未提交、推送或发布。
