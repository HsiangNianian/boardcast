# C60 外观与笔迹：生产浏览器验收

2026-11-04，工作树基线 `4f9ab26`。本批补齐 C60 的下述 G1 路径，并修复三个实际复现的问题。C60 的真实系统主题与 VoiceOver 等 G3 验收仍未完成，完整重构继续按 C01–C68 推进。

2026-11-05 更新：[Linux 原生系统主题续批](native-theme-validation.md)已验证真实 GSettings/GTK portal 通知、两种白板、手动覆盖和三种偏好冷启动。下文保留本批当时的 G1 证据边界；原生 Wayland、Mac/VoiceOver 仍未验。

## 修复及复现

| 问题 | 修复前证据 | 最终行为 |
| --- | --- | --- |
| 保存的小数偏好与滑杆步长不一致 | `12.3 / 63.7 / 149.2` 的标签保留小数，原生滑杆实际显示 `12 / 65 / 150`；[原始结果](evidence/implementation/appearance/baseline-result.json)、[截图](evidence/implementation/appearance/baseline-fractional-sliders.png) | 启动时先限范围，再按对应滑杆的步长归一化；显示、持久值和渲染参数统一为 `12 / 65 / 150`，见 [最终截图](evidence/implementation/appearance/appearance/fractional-normalized.png) |
| 当前板导出丢失续会底板的淡化 | 实际画面中的旧框与文字为 `opacity=0.38`，下载的 [SVG](evidence/implementation/appearance/baseline-export.svg) 六组笔画全部为 `1` | App 导出快照包含当前页 `base`，导出器冻结该字段并传给 SVG/PNG/动画的合成过程；旧节点为 `0.38`，新节点与新连线为 `1` |
| 窄窗调节滑杆后滚回顶部 | 在 640×400 中文页面用 Tab 到达滑杆，立即按 Home；三个焦点控件的底部分别落到 504/558/612 px，超出可见区域，见 [实际键盘记录](evidence/implementation/appearance/narrow-tab-home.json) | 设置面板的滚动恢复回调只随分类改变，输入重绘不会重置刚发生的焦点滚动；最终三个控件底部分别为 267/321/375 px，见 [截图](evidence/implementation/appearance/appearance/chinese-narrow.png) |

产品改动只涉及 `App.tsx`、`exporter.ts`、`SettingsPanel.tsx`，本批差异见 [product.patch](evidence/implementation/appearance/product.patch)。已有的分类返回位置保留、导出快照冻结和错误处理继续有效。

## 实际验证

新增 [check-appearance.cjs](../../scripts/ui/check-appearance.cjs) 使用生产 App 和独立的真实 IndexedDB。输入为两个中英日混合节点、一条新连线及一个继承节点的合成会议，没有调用模型或修改用户会议。

| 检查 | 实际范围与结果 |
| --- | --- |
| 外观流程 | [82 项通过](evidence/implementation/appearance/appearance/appearance-check.json)：非法值、越界、小数偏好；系统 light→dark→light 实时变化；手动主题覆盖系统；切回系统；三种选择重开保持 |
| 笔迹组合 | 潦草度 `0/100` × 中日文和线条 `50/200` × 英文 `50/200` 的八种组合，另加默认 `0/65/100`。均通过实际键盘 Home/End/方向键设置，九种轮廓哈希不同 |
| SVG、PNG、动画 | 九种组合均实际下载。SVG 和动画的全部可见轮廓及组透明度与屏幕一致；移除动画遮罩后比较轮廓。PNG 与同次下载 SVG 在 PNG 尺寸下栅格化的像素逐通道一致，九组差异均为零 |
| PDF 与离线 HTML | 只抽取全低 `0/50/50` 和全高 `100/200/200` 两组。实际 HTML 在独立禁网 context 内保留参数、底板、全部轮廓和透明度，零 HTTP 请求和脚本错误。`pdfimages` 提取实际 PDF 第一张板的 2400×1696 JPEG；按同次 SVG 的几何、纸面与缩放生成参考 JPEG，排除页眉前 128 行后逐通道差异均为零 |
| 数据与窄窗 | 外观调整和导出后，保存的 graph、原话与 base 均保持不变。中文 640×400 下三个滑杆通过真实 Tab→Home 操作，数值正确且焦点控件保持可见，无横向溢出 |
| 导出回归 | 最终生产构建的 [七格式 68 项](evidence/implementation/appearance/exports/export-check.json) 和 [交接 64 项](evidence/implementation/appearance/export-handoff/export-handoff-check.json) 全通过；后者使用受控原生 IPC 协议，不代表真实 Tauri 系统保存 |
| 其他检查 | [18 组 CSS 颜色对](evidence/implementation/appearance/theme-contrast/rendered-theme-contrast.json)、[设置 Dialog 回归](evidence/implementation/appearance/dialogs/runner-result.json)、[9 项定向 Node](evidence/implementation/appearance/node-tests.log)、[类型检查](evidence/implementation/appearance/typecheck-result.json) 和 [生产构建](evidence/implementation/appearance/build-result.json) 均通过 |

深色界面仍导出浅色纸面，沿用既有产品规则。屏幕上的粉笔/记号笔滤镜没有进入逐轮廓比较；这里不宣称深色屏幕截图与白纸产物像素相同。PDF 比较覆盖抽样会议的板面，未比较页眉或所有参数组合下的所有 PDF 页面。

旧对比度检查在本批的 reduced-motion context 中读出了错误的 `1:1`：同一测量 span 切换 CSS 颜色变量时，`0.01ms` 过渡使即时读取仍返回上一个颜色。检查现已等待 App 主题及样式就绪，并仅对测量探针关闭 transition，阈值未改变。早期外观脚本也曾等待窄窗中被原话栏遮住的板面“可见”，现改为等待节点挂载。这些脚本修正与随后实际复现的滑杆滚动缺陷分别记录在 [早期检查说明](evidence/implementation/appearance/initial-harness-failures/notes.json)。

## 构建与边界

最终入口为 `index-CquHqz0r.js`，SHA-256 为 `3164a61aecbb830a8b9cf82f0c9a639177eebd81288d9514490987d39af869a1`。四项生产浏览器检查均运行于该构建；Dialog 使用 1423 的真实组件 fixture。完整命令、源文件和播放器哈希见 [identity.json](evidence/implementation/appearance/identity.json)，源码快照、产物、早期失败和最终结果见 [证据目录](evidence/implementation/appearance/)。

本批执行 `tsc --noEmit` 和 Vite 主应用构建，复用已有播放器 bundle，未声称运行完整 `npm run build`。未执行 Rust/原生构建、真实 OS 深浅色切换、Mac/VoiceOver、真实音频或 G4 用户试用；自由编辑样式与全量续会 C38 变体未因本批的手绘继承 fixture 而扩大验收结论。

未提交、推送或发布。私有预览和浏览器已清理，用户已有的 1423/1430 服务保留，见 [清理记录](evidence/implementation/appearance/cleanup.json)。
