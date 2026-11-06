# C35 会议库搜索与定位

2026-11-06，基线 `4f9ab26` 的当前工作区。补查中英文多关键词、快速输入、原话/板书/摘要定位及窄窗口键盘操作，修复五处查询和跳转问题。

## 修复

- `CACHE cache`、`缓存 缓存` 原来无法返回任何结果。搜索词在原有大小写处理后去重，仍要求每个不同关键词在会议中出现。
- `cache ca he` 的重叠高亮把结果文字显示成 `Cachehe`。合并重叠区间后，高亮保留全部命中字符，结果文字不重复。[修复前](evidence/implementation/library-search/baseline/overlap.png)、[修复后](evidence/implementation/library-search/browser/overlap.png)。
- 缺页码的旧原话搜索结果没有携带默认页号，打开存于第 2 板的会议后仍停在那里。现在沿用原话展示和重试的既有规则，定位到第 1 板。[修复前](evidence/implementation/library-search/baseline/legacy-hit.png)、[修复后](evidence/implementation/library-search/browser/legacy-hit.png)。
- 窄窗口用键盘打开板书结果，焦点可能被尚未关闭的会议库拿回。复用现有会议库动作延后执行的方式，待对话框关闭及目标会议渲染后再定位；普通关闭仍返回会议库按钮。
- 高亮尚未消失时，滚开原话再打开同一结果不会再次滚动。定位请求保留行号并使用新的请求对象，让每次搜索或来源定位都能重新滚动。

产品仅改 `engine/search.ts` 和 `App.tsx`。已核对持久会议搜索、当前未保存副本搜索、结果渲染、会议打开/回放/导出和两处原话定位入口；未增加依赖、存储格式、翻译键或搜索设置。[本批差异](evidence/implementation/library-search/batch.diff)、[源文件哈希](evidence/implementation/library-search/source-hashes.json)。

## 浏览器与逻辑验证

新增 [`check-library-search.cjs`](../../scripts/ui/check-library-search.cjs) 使用生产 App、真实 IndexedDB、三份合成会议和 60 条原话。最终 [1430 预览的 20 项检查](evidence/implementation/library-search/browser/report.json)通过：重复/重叠词、跨字段全部匹配、无结果、排除手工操作原话、跨会/跨页/摘要、再次定位同一句、普通关闭的返回焦点、720×480 键盘流程、查询冷恢复，以及所有已存会议不变。

有效原构建对照为 [19 项中 6 项失败](evidence/implementation/library-search/baseline/report.json)。第一轮修复后追加同一句再次定位检查，[20 项中该项失败](evidence/implementation/library-search/repeated-hit-baseline/report.json)；最终通过。新增三个单元测试也先失败，最终 [6 项搜索单元测试](evidence/implementation/library-search/unit-tests.log)通过。

相关生产回归通过会议库 21 项、内容列表 4 组、长会阅读 55 项（含 40 板/2,000 条原话、来源定位、缩放与离线输出）。会议库组件另以受控异步响应验证迟到的失败和成功都不能覆盖较新查询，以及既有错误恢复。见 [生产日志](evidence/implementation/library-search/related/browser.log)、[组件日志](evidence/implementation/library-search/related/meetings-component.log)。TypeScript、主应用和 Linux 原生构建通过；播放器未改动，复用产物字节一致。

```sh
node --test src/engine/search.test.ts
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-library-search.cjs
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1423 check-meetings.cjs
```

早期检查曾使用错误的无结果文案、在异步定位完成前读取可见性，以及在 Home 滚动完成前断言；这些校准失败未计为产品缺陷。最终等待实际焦点及视图，重复定位对照也先确认原话确已滚出可见范围。见 [notes.json](evidence/implementation/library-search/notes.json)。

## Linux 原生验证

最终二进制运行于独立应用标识、XDG、D-Bus 和 Xvfb。测试前写入三份合成会议文件，再通过实际原生文件读取和窗口操作搜索；未访问用户会议、模型或物理音频。

- 实际键盘输入 `CACHE cache` 返回两份会议，`cache ca he` 的高亮不重复原文。[重复词结果](evidence/implementation/library-search/native/repeated-results.png)、[重叠词结果](evidence/implementation/library-search/native/overlap-query.png)。
- Tab → Tab → Enter 打开旧原话，真实窗口显示第 1 板及目标原话。[键盘结果](evidence/implementation/library-search/native/legacy-open.png)。
- 真实窗口调整为 720×480 后，通过同样的键盘路径打开第 2 板，目标被选中且画布获得焦点；摘要结果也打开对应内容。[窄窗口板书](evidence/implementation/library-search/native/compact-board-open.png)、[摘要](evidence/implementation/library-search/native/summary-open.png)。
- 正常退出后使用同一二进制冷启动，搜索词 `SummaryNeedle` 保留，结果仍能打开。三份原会议文件在搜索、退出、冷启动和再次退出期间逐字节一致。[冷启动查询](evidence/implementation/library-search/native-cold/library-restored.png)、[文件核对](evidence/implementation/library-search/native-proof.json)。

两次原生运行正常退出，35 个登记进程及私有预览已结束，两个 Cua 会话显式结束。1430 更新至 `index-d23p88qr.js`，原有 1,410 个资源文件保持，入口最后替换，HTTP 哈希匹配；1423/1430 原进程继续提供预览。[构建身份](evidence/implementation/library-search/native/identity.json)、[清理](evidence/implementation/library-search/cleanup.json)、[同步](evidence/implementation/library-search/publication.json)。

## 保留边界

本批不代表任意 Unicode 大小写组合、海量会议库性能或读屏体验全部验收。快速响应竞争采用组件控制，原生重复定位同一句的时序未另测。未重复全量 Node/Rust 测试，未制作或发布安装包；Mac/VoiceOver、Windows ARM 新包与原 Adreno 机器、完整 G3/G4 仍保留。
