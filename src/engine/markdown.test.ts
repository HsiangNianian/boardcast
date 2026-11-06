import assert from 'node:assert/strict'
import { test } from 'node:test'
import { toMarkdown } from '../board/markdown.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { setLangChoice } from '../i18n/index.ts'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

// These tests check the Chinese wording
setLangChoice('zh')

test('markdown export: topics as numbered lists, flowchart as mermaid, transcript at the end', () => {
  let g = applyOps(
    emptyGraph(),
    parseOps(
      'T 「爬虫」\nL 「爬虫」 「监控任务」\nL 「监控任务」 「定时拉取」\nQ 「爬虫」 「要不要上 MQ」\nM 「监控任务」 星标\nE 「用户」 -> 「网关」 : 「请求」\nG 「入口」 n5\nF 「T_now − T_display」',
    ),
  )
  // 铅笔稿不导出
  g = applyOps(g, parseOps('N 「还没确认」'), 'draft')
  const md = toMarkdown(g, [{ speaker: 'A', text: '先把爬虫理一下' }], '会议板书', new Date(2026, 9, 29, 15, 4))
  assert.match(md, /^# 会议板书\n\n> 2026-10-29 15:04 · boardcast/)
  assert.match(md, /## 爬虫\n\n1\. 监控任务 ★\n   - a\. 定时拉取\n- ？要不要上 MQ/)
  assert.match(md, /```mermaid\nflowchart LR\n {2}subgraph g1\["入口"\]\n {4}n5\["用户"\]\n {2}end\n {2}n6\["网关"\]\n {2}n5 -->\|"请求"\| n6\n```/)
  assert.match(md, /## 待定\n\n- `T_now − T_display`/)
  assert.match(md, /## 会议记录\n\n- \*\*A\*\*：先把爬虫理一下/)
  assert.doesNotMatch(md, /还没确认/)
})

test('a meeting with several boards exports one section per board', () => {
  const a = applyOps(emptyGraph(), parseOps('T 「爬虫」\nL 「爬虫」 「监控任务」'))
  const b = applyOps(emptyGraph(), parseOps('E 「用户」 -> 「网关」'))
  const lines = [
    { speaker: 'A', text: '先说爬虫', page: 0 },
    { speaker: 'B', text: '再看网关', page: 1 },
  ]
  const md = toMarkdown(b, lines, '周会', new Date(2026, 9, 30, 9, 0), undefined, [a, b])
  assert.match(md, /## 第 1 张板\n\n### 爬虫\n\n1\. 监控任务/)
  assert.match(md, /## 第 2 张板\n\n### 流程\n\n```mermaid/)
  assert.match(md, /## 会议记录\n\n### 第 1 张板\n\n- \*\*A\*\*：先说爬虫\n\n### 第 2 张板\n\n- \*\*B\*\*：再看网关/)
  // 只有一张板时照旧，不分节
  assert.doesNotMatch(toMarkdown(a, lines.slice(0, 1)), /张板/)
})

const render = (value: string) => renderToStaticMarkup(createElement(Markdown, { children: value, remarkPlugins: [remarkGfm] }))

test('plain meeting fields retain punctuation instead of becoming Markdown formatting', () => {
  const g = applyOps(emptyGraph(), parseOps('T 「*重点* _topic_ [文档](file.md)」\nL n1 「- [x] literal」\nK n2 「<img src=x> &copy;」\nM n2 圈出'))
  const md = toMarkdown(g, [{ speaker: 'A_*B', text: '~~原话~~ `code` ![image](https://example.invalid/image.png)' }, { text: '1. literal numbered sentence.' }, { text: '1.' }], '# 周会 #')
  const html = render(md)
  assert(html.includes('<h1># 周会 #</h1>'))
  assert(html.includes('<h2>*重点* _topic_ [文档](file.md)</h2>'))
  assert(html.includes('<strong>- [x] literal</strong>'))
  assert(html.includes('<strong>A_*B</strong>：~~原话~~ `code` ![image]('))
  assert(html.includes('<a href="https://example.invalid/image.png">https://example.invalid/image.png</a>)'))
  assert(html.includes('<li>1. literal numbered sentence.</li>'))
  assert(html.includes('<li>1.</li>'))
  assert.doesNotMatch(html, /<(?:em|del|img|input)(?:\s|>)/)
  assert(md.includes('\\<img src=x\\> \\&copy;'))
})

test('multiline fields cannot add headings, lists or fences, and source spans and links stay literal', () => {
  const g = applyOps(emptyGraph(), parseOps('T 「Topic」\nT 「Other」\nE n1 -> n2 : 「*cross*」'))
  g.nodes[0].label = 'Topic\r\n# not a heading\n\n```'
  g.nodes.push({ id: 'n3', kind: 'region', label: 'source', text: '中文\n日本語\rEnglish', aliases: [], marks: ['高亮'], ink: 'ink', spans: [{ mark: '圈出', text: '*quoted*', boxes: [] }] })
  const md = toMarkdown(g, [{ text: 'first\n- second\n\n## third' }])
  assert(md.includes('Topic<br />\\# not a heading<br /><br />\\`\\`\\`'))
  assert(md.includes('<mark>中文<br />日本語<br />English</mark>'))
  assert(md.includes('圈出：\\*quoted\\*'))
  assert(md.includes('Other：\\*cross\\*'))
  assert(md.includes('- first<br />- second<br /><br />\\#\\# third'))
  assert.doesNotMatch(md, /^## third$|^# not a heading$|^```$/m)
})

test('formula code spans retain embedded backtick runs and significant surrounding spaces', () => {
  const g = emptyGraph()
  const labels = ['E = mc²', '`x` + ``y``', ' x + y ', 'x ` y', '   ']
  g.nodes = labels.map((label, i) => ({ id: `n${i + 1}`, kind: 'formula', label, aliases: [], marks: [], ink: 'ink' }))
  const html = render(toMarkdown(g))
  for (const label of labels) assert(html.includes(`<code>${label}</code>`), label)
  assert.equal((html.match(/<code>/g) ?? []).length, labels.length)
  g.nodes[0].label = 'a\n\nb'
  assert(toMarkdown(g).includes('`a`<br /><br />`b`'))
})

test('summary Markdown including repeated blank lines in code is preserved', () => {
  const summary = '### 结论\n\n**原有格式**\n\n```text\nfirst\n\n\nlast\n```'
  const md = toMarkdown(emptyGraph(), [], '周会', undefined, summary)
  assert(md.includes(summary))
  const html = render(md)
  assert(html.includes('<strong>原有格式</strong>'))
  assert(html.includes('first\n\n\nlast\n</code>'))
})

test('Mermaid labels cannot end the surrounding code block or introduce raw HTML', () => {
  const g = applyOps(emptyGraph(), parseOps('E 「入口」 -> 「出口」 : 「传递」\nG 「分组」 n1'))
  g.nodes[0].label = '中文 "quoted" #quot; <tag> &copy;\n```\n日本語'
  g.edges[0].label = '| edge " \n```'
  g.groups[0].label = 'Group <b> & #quot;'
  const md = toMarkdown(g)
  const html = render(md)
  assert.equal((html.match(/<code class="language-mermaid">/g) ?? []).length, 1)
  assert.equal((md.match(/^```$/gm) ?? []).length, 1)
  assert.doesNotMatch(md, /<tag>|<b>/)
  assert(md.includes('#35;quot;'))
  assert(md.includes('#60;tag#62;'))
})
