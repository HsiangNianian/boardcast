// Records what the TypeScript engine does with real recorded model output, as the Rust port's answer key.
// Usage: node fixtures/make-goldens.mjs <boardcast-root> <output.json>
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const root = resolve(process.argv[2])
const load = (p) => import(pathToFileURL(join(root, p)).href)
const { applyOps, parseOps } = await load('src/engine/ops.ts')
const { describe, emptyGraph } = await load('src/engine/graph.ts')
const { boardLang, grammar, messages, userMessage } = await load('src/engine/prompt.ts')
const { StableLayout } = await load('src/board/layout.ts')
const { diffGraph, inkOnly } = await load('src/engine/steps.ts')

const rect = (b) => ({ id: b.id, x: b.x, y: b.y, w: b.w, h: b.h })
const shot = (l) => ({
  regions: l.regions.map((r) => ({ ...rect(r), title: r.title ?? null, rows: r.rows.map((x) => ({ id: x.id, prefix: x.prefix, text: x.text, x: x.x, y: x.y, w: x.w, size: x.size })), links: (r.links ?? []).map((k) => ({ from: k.from, to: k.to, points: k.points })) })),
  diagram: l.diagram ? { box: rect(l.diagram.box), nodes: l.diagram.nodes.map(rect), groups: l.diagram.groups.map((b) => ({ ...rect(b), label: b.label })), edges: l.diagram.edges.map((e) => ({ from: e.from, to: e.to, points: e.points })) } : null,
  links: l.links.map((k) => ({ from: k.from, to: k.to, points: k.points ?? null })),
  bounds: l.bounds,
})

const shape = (g) => ({
  nodes: g.nodes.map((n) => ({ id: n.id, label: n.label, aliases: n.aliases, kind: n.kind, parent: n.parent ?? null, group: n.group ?? null, marks: n.marks, icon: n.icon ?? null })),
  edges: g.edges.map((e) => ({ from: e.from, to: e.to, label: e.label ?? null })),
  groups: g.groups.map((x) => ({ id: x.id, label: x.label })),
})

const steps = []
const dir = join(root, 'public/demos')
for (const file of readdirSync(dir).filter((f) => f.endsWith('.json')).sort()) {
  const demo = JSON.parse(readFileSync(join(dir, file), 'utf8'))
  let g = emptyGraph()
  let recent = []
  let previous = emptyGraph()
  const board = new StableLayout()
  for (const line of demo.lines ?? []) {
    if (line.manual || typeof line.ops !== 'string') continue
    const next = { text: line.text, ...(line.speaker ? { speaker: line.speaker } : {}) }
    const context = recent.slice(-2)
    const before = { describe: describe(g), user: userMessage(g, context, next), lang: boardLang(context, next), grammar: grammar(g, 8, boardLang(context, next)) }
    g = applyOps(g, parseOps(line.ops), 'ink')
    steps.push({ file, text: line.text, speaker: line.speaker ?? null, ops: line.ops, ...before, after: shape(g), layout: shot(await board.layout(g)), memory: board.snapshot(), diff: diffGraph(previous, inkOnly(g)) })
    previous = g
    recent.push(next)
  }
}
// Whole boards laid out in one go, the case incremental meetings barely exercise.
function random(seed) {
  let a = seed >>> 0
  return (n) => {
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return (((t ^ (t >>> 14)) >>> 0) / 4294967296 * n) | 0
  }
}
const stress = []
for (let seed = 1; seed <= 80; seed++) {
  const r = random(seed)
  const size = 4 + r(23)
  const names = Array.from({ length: size }, (_, i) => `${['服务', '网关', 'Queue', '存储', '任务'][i % 5]}${i}`)
  const lines = []
  for (let i = 1; i < size; i++) lines.push(`E 「${names[Math.max(0, i - 1 - r(4))]}」 -> 「${names[i]}」${r(3) ? '' : ' : 「调用」'}`)
  for (let i = r(size); i > 0; i--) { const x = r(size), y = r(size); if (x !== y) lines.push(`E 「${names[x]}」 -> 「${names[y]}」`) }
  if (seed % 2) { const from = r(size); lines.push(`G 「分组」 ${[0, 1, 2].map((k) => `「${names[(from + k) % size]}」`).join(' ')}`) }
  const ops = lines.join('\n')
  const g = applyOps(emptyGraph(), parseOps(ops), 'ink')
  stress.push({ seed, ops, layout: shot(await new StableLayout().layout(g)) })
}
// The summary's prompts and its fingerprint, which meetings store.
const { boardsText, chunks, plainText, summaryFingerprint, summaryMessages, tidySummary, transcript } = await load('src/engine/summary.ts')
const { spokenLang } = await load('src/engine/lang.ts')
const summaries = []
for (const file of readdirSync(dir).filter((f) => f.endsWith('.json')).sort()) {
  const demo = JSON.parse(readFileSync(join(dir, file), 'utf8'))
  let g = emptyGraph()
  for (const line of demo.lines ?? []) if (typeof line.ops === 'string') g = applyOps(g, parseOps(line.ops), 'ink')
  // Two boards in some, so the per-board wording is covered too.
  const boards = summaries.length % 3 === 2 ? [g, applyOps(emptyGraph(), parseOps('T 「第二张」\nL 「第二张」 「条目」'), 'ink')] : [g]
  const lines = (demo.lines ?? []).map((l, i) => (boards.length > 1 ? { ...l, page: i % 2 } : l))
  const spoken = lines.filter((l) => !l.manual)
  const lang = spokenLang(spoken.map((l) => l.text))
  const said = transcript(spoken, lang)
  summaries.push({
    file, boards, lines: spoken.map(({ speaker, text, page }) => ({ speaker, text, page })), lang,
    fingerprint: summaryFingerprint(boards, spoken), transcript: said, boardsText: boardsText(boards, undefined, lang),
    messages: summaryMessages(boards, `${lang === 'en' ? 'Transcript:' : '会议记录：'}\n${said}`, false, lang),
    partial: summaryMessages(boards, said, true, lang),
    chunks: chunks(spoken, 40).map((p) => p.length),
  })
}
const tidyInputs = [
  ['好的，下面是摘要：\n```markdown\n### 结论\n- 先做登录\n### 待办\n- [ ] 写接口（负责人：小王）\n```', 'zh'],
  ['**结论**\n- 用 Go\n## 决定：\n- 上 Redis\n【待办事项】\n- [ ] 写爬虫（负责人：A）\n待讨论：\n- 热度公式', 'zh'],
  ['这场会主要聊了爬虫。', 'zh'],
  ['', 'zh'],
  ['Here is the summary:\n## Conclusions\n- Use Go\n**Action items:**\n- [ ] Ship it (owner: Sam)\nNext steps\n- Later\nOpen issues:\n- Budget', 'en'],
  ['Decisions made\n- Yes', 'en'],
  ['Nothing recognizable here.', 'en'],
]
const tidy = tidyInputs.map(([input, lang]) => ({ input, lang, output: tidySummary(input, lang) }))
const plain = ['### 结论\n- **用 Go**\n\n\n### 待办\n- [ ] 写爬虫\n- [x] 定方案', '### 结论\n- 好\n### 决定\n- 无', '说明 `代码` ~~划掉~~\n  * 缩进\n+ [X] 大写'].map((input) => ({ input, output: plainText(input) }))
// Markdown, in both languages, on recorded meetings and on labels written to break it.
const { toMarkdown } = await load('src/board/markdown.ts')
const { setLangChoice } = await load('src/i18n/index.ts')
const exportedAt = new Date(2026, 0, 2, 3, 4)
const hostile = {
  nodes: [
    { id: 'n1', label: '# 标题 *强调* [链接](x) <b>&', kind: 'topic', aliases: [], marks: ['圈出', '星标'], ink: 'ink' },
    { id: 'n2', label: '- 不是列表', kind: 'item', parent: 'n1', aliases: [], marks: ['划掉', '下划线', '高亮'], ink: 'ink' },
    { id: 'n3', label: '12. 不是编号', kind: 'item', parent: 'n1', aliases: [], marks: [], ink: 'ink' },
    { id: 'n4', label: '  + 换行\n第二行\r\n第三行', kind: 'item', parent: 'n3', aliases: [], marks: [], ink: 'ink' },
    { id: 'n5', label: 'x`y``z', kind: 'formula', parent: 'n3', aliases: [], marks: [], ink: 'ink' },
    { id: 'n6', label: '`a\n\n b ', kind: 'formula', aliases: [], marks: [], ink: 'ink' },
    { id: 'n7', label: '为什么？', kind: 'question', parent: 'n1', aliases: [], marks: [], ink: 'ink' },
    { id: 'n8', label: '备注一', kind: 'note', parent: 'gone', aliases: [], marks: [], ink: 'ink' },
    { id: 'n9', label: '草稿', kind: 'item', parent: 'n1', aliases: [], marks: [], ink: 'draft' },
    { id: 'n10', label: '网关"<a>', kind: 'node', group: 'g1', aliases: [], marks: ['高亮'], ink: 'ink' },
    { id: 'n11', label: '服务#1', kind: 'node', aliases: [], marks: [], ink: 'ink' },
    { id: 'n12', label: '挂件', kind: 'question', parent: 'n11', aliases: [], marks: [], ink: 'ink' },
    { id: 'n13', label: '原文', text: '原文全文 *x*', kind: 'region', aliases: [], marks: ['圈出'], spans: [{ mark: '高亮', text: '全文' }], ink: 'ink' },
    { id: 'n14', label: '批注', kind: 'note', parent: 'n13', aliases: [], marks: [], ink: 'ink' },
    { id: 'n15', label: '三层', kind: 'item', parent: 'n4', aliases: [], marks: [], ink: 'ink' },
    { id: 'n16', label: '四层', kind: 'item', parent: 'n15', aliases: [], marks: [], ink: 'ink' },
    { id: 'n17', label: '12.3 小数', kind: 'item', parent: 'n1', aliases: [], marks: [], ink: 'ink' },
  ],
  edges: [
    { from: 'n10', to: 'n11', label: '调用"', ink: 'ink' },
    { from: 'n11', to: 'n10', ink: 'draft' },
    { from: 'n1', to: 'n11', label: '关联', ink: 'ink' },
    { from: 'n2', to: 'missing', ink: 'ink' },
  ],
  groups: [{ id: 'g1', label: '后端&"网关"' }, { id: 'g2', label: '空组' }],
  nextNode: 18, nextGroup: 3,
}
const markdownCases = []
for (const lang of ['zh', 'en']) {
  setLangChoice(lang, false)
  const take = (name, boards, lines, title, summary) => markdownCases.push({ name: `${name} ${lang}`, lang, boards, lines, title, summary, output: toMarkdown(boards[0], lines, title, exportedAt, summary, boards.length > 1 ? boards : undefined) })
  for (const s of summaries) take(s.file, s.boards, s.lines, s.file, summaries.indexOf(s) % 2 ? '### 结论\n- 好\n' : undefined)
  const said = [{ text: '- 第一句 *a*' }, { speaker: 'A_1', text: '12. 第二句', page: 0 }, { text: '换行\n了', page: 1, manual: true }]
  take('hostile', [hostile], said, '# 题目 [x]', '  ### 待办\n- [ ] 写\n  ')
  take('hostile pages', [hostile, emptyGraph(), hostile], said, '题目', undefined)
  take('empty', [emptyGraph()], [], '空', undefined)
}
setLangChoice('zh', false)
// When ink appears in the animated SVG.
const { ranks, timeline } = await load('src/ink/animate.ts')
const rankCases = []
for (const [fileIndex, file] of readdirSync(dir).filter((f) => f.endsWith('.json')).sort().entries()) {
  if (fileIndex % 3) continue
  const demo = JSON.parse(readFileSync(join(dir, file), 'utf8'))
  let g = emptyGraph()
  const recorded = []
  for (const line of demo.lines ?? []) {
    if (typeof line.ops !== 'string') continue
    const before = inkOnly(g)
    g = applyOps(g, parseOps(line.ops), 'ink')
    // Some things are left out of the record, as in a meeting recorded part way through.
    const d = diffGraph(before, inkOnly(g))
    if (d && recorded.length < (demo.lines.length * 3) / 4) recorded.push({ t: recorded.length, line: recorded.length, d })
  }
  const layout = await new StableLayout().layout(g)
  const regions = layout.regions.map((r) => [r.id, r.rows.map((row) => row.node.id)])
  const keys = [
    ...g.nodes.flatMap((n) => [`${n.id}|box`, `${n.id}|${n.label}|x`, `${n.id}|高亮`, `${n.id}|${n.label}|h`, `${n.id}|pre|1.`]),
    ...g.edges.flatMap((e) => [`${e.from}>${e.to}`, `${e.from}>${e.to}l|bg`, `${e.from}~${e.to}`, `${e.from}~${e.to}l`]),
    ...g.groups.flatMap((x) => [`${x.id}|${x.label}`, `${x.id}t|${x.label}`]),
    ...regions.map(([id]) => `${id}|标题|ul`),
    'nowhere|x',
  ]
  rankCases.push({ file, nodes: g.nodes, regions, steps: recorded, keys, ranks: ranks(g, { regions: layout.regions }, recorded, keys) })
}
const timelineCases = [
  [[{ lengths: [10, 900] }, { under: true, lengths: [50] }, { lengths: [5] }], [0, 1, 1], 12000],
  [[{ lengths: [9000, 9000] }, { lengths: [9000] }, { under: true, lengths: [1] }], [2, 0, 1], 12000],
  [[{ lengths: [100] }, { lengths: [100] }], [0, 0], 100],
  [[], [], 12000],
].map(([pieces, rank, total]) => {
  const { timed, end } = timeline(pieces.map((p) => ({ under: p.under, strokes: p.lengths.map((length) => ({ length })) })), rank, total)
  return { pieces, rank, total, end, timed: timed.map(({ delays, durs }) => ({ delays, durs })) }
})
// Imported pages: finding words in their text, the model's operations on them, and how they lay out.
const { locate, spanAt } = await load('src/engine/spans.ts')
const { pageGraph } = await load('src/engine/regions.ts')
const seg = (t, x, w) => ({ t, x, w })
const english = [
  { y: 10, h: 12, segs: [seg('Deep networks are hard to train. We present a resid-', 10, 400)] },
  { y: 26, h: 12, segs: [seg('ual framework (see Fig. 3, et al.) that eases', 10, 300), seg('training.', 330, 60)] },
  { y: 42, h: 12, segs: [seg('Results improve! Is it "stable?" Yes.', 10, 280)] },
]
const chinese = [
  { y: 100, h: 16, segs: [seg('深度网络很难训练。我们提出一个残差框架，', 10, 340)] },
  { y: 120, h: 16, segs: [seg('它让训练更容易。Ｆｕｌｌ ｗｉｄｔｈ 文字也能找到！', 10, 340)] },
]
const turned = [{ y: 5, h: 10, angle: 0.5, segs: [seg('Turned text here. Second one', 20, 200)] }, { y: 18, h: 10, angle: 0.5, segs: [seg('goes on.', 20, 60)] }]
const spanCases = []
const quoted = (name, rows, quote, unit) => spanCases.push({ name, rows, quote, unit, mark: '圈出', span: locate(rows, quote, '圈出', unit) })
const pointed = (name, rows, x, y, unit) => spanCases.push({ name, rows, x, y, unit, mark: '高亮', span: spanAt(rows, x, y, '高亮', unit) })
for (const unit of ['word', 'sentence']) {
  for (const q of ['Deep', 'residual', 'RESIDUAL  framework', 'Fig', 'eases training', 'stable', 'not there', '', 'Yes']) quoted(`en ${q} ${unit}`, english, q, unit)
  for (const q of ['残差', '框架它让', 'full width', '文字', '难训练']) quoted(`zh ${q} ${unit}`, chinese, q, unit)
  quoted(`turned ${unit}`, turned, 'Second', unit)
  for (const [x, y] of [[40, 15], [380, 15], [20, 30], [340, 32], [80, 48], [5, 5], [999, 999]]) pointed(`en point ${x},${y} ${unit}`, english, x, y, unit)
  for (const [x, y] of [[30, 108], [120, 108], [200, 128], [300, 128]]) pointed(`zh point ${x},${y} ${unit}`, chinese, x, y, unit)
  pointed(`turned point ${unit}`, turned, 60, 45, unit)
}
const pageAreas = [
  { text: 'Deep networks are hard to train. We present a residual framework.', x: 40, y: 30, w: 400, h: 44, rows: english },
  { text: '深度网络很难训练。我们提出一个残差框架，它让训练更容易。Full width 文字也能找到！', x: 40, y: 200, w: 400, h: 44, rows: chinese },
  { text: 'A heading with no text layer, long enough that its label has to be cut short here', x: 40, y: 300, w: 300, h: 20 },
  { text: '「 same 」', name: 'Same', x: 40, y: 340, w: 60, h: 14 },
  { text: 'same again', name: 'Same', x: 40, y: 360, w: 60, h: 14 },
]
const pageOps = [
  [],
  ['M n1 圈出 「residual」', 'M n1 下划线 句 「Fig」', 'M n1 高亮', 'M n3 划掉', 'M n4 星标'],
  ['M n1 圈出 「残差」', 'M n3 高亮 「not there at all」', 'M n1 星标 句 「残差」', 'M n1 圈出 「residual」', 'M n1 圈出 「residual」'],
  ['K n1 这段重要', 'Q n2 为什么', 'K n3 很长的一条批注写在边栏里', 'F n1 x+y', 'L n1 不该是条目', 'T 「讨论」', 'L 「讨论」 「要点」', 'E n1 n2 对比', 'E 「讨论」 n3'],
  ['R n1 改名', 'A n2 别名', 'D n3', 'G 分组 n4 n5', 'L 「讨论」 n1', 'M n1 划掉'],
  ['E 「甲」 -> 「乙」', 'K 「甲」 流程上的备注'],
]
const pageCases = []
for (const size of [{ w: 900, h: 1200, no: 3 }, { w: 500, h: 300 }]) {
  for (const ops of pageOps) {
    const before = pageGraph(size, pageAreas)
    const g = applyOps(before, parseOps(ops.join('\n')), 'ink')
    const memory = new StableLayout()
    const l = await memory.layout(g)
    pageCases.push({
      name: `${size.w} ${ops.join(' / ')}`, before, ops: ops.join('\n'), graph: g, described: describe(g), step: diffGraph(before, g),
      regions: l.regions.map((r) => ({ id: r.id, x: r.x, y: r.y, w: r.w, h: r.h, rows: r.rows.map((x) => [x.id, x.prefix, x.text, x.x, x.y, x.w, x.size]) })),
      anchors: Object.fromEntries([...l.anchors].map(([id, b]) => [id, [b.x, b.y, b.w, b.h]])),
      bounds: l.bounds, links: l.links.map((e) => [e.from, e.to]), memory: memory.snapshot(),
    })
  }
}
// Reading a PDF page's layout, on pages written to reach each of its rules.
const { analyzePage } = await load('src/board/paper.ts')
const { parseArxiv } = await load('src/board/arxiv.ts')
const arxivCases = ['2401.12345', 'arXiv:2401.12345v2', 'ARXIV: 1512.03385', 'https://arxiv.org/abs/2401.12345', 'http://www.arxiv.org/pdf/2401.12345v3.pdf', 'arxiv.org/html/2401.12345/', 'https://export.arxiv.org/format/2401.12345?x=1#y', 'hep-th/9901001', 'math.GT/0309136v1', 'https://evil.example/abs/2401.12345', '24.12345', 'not an id', '', '  2401.12345  ', 'https://arxiv.org/abs/2401.12345/extra'].map((input) => ({ input, id: parseArxiv(input) }))
const P = 900, Q = 1165, L = 74, R = 474, C = 352
const put = (x, y, text, w = C, h = 13, angle) => ({ text, x, y, w, h, ...(angle ? { angle } : {}) })
const prose = 'We present a method that eases the training of networks which are much deeper than those used before, and we show results'
const body = (x, y, n, text = prose, w = C) => Array.from({ length: n }, (_, i) => put(x, y + i * 16, `${text} ${i}`.slice(0, 62), w))
const twoColumn = (y = 100, n = 12) => [...body(L, y, n), ...body(R, y, n)]
const paperDocuments = {
  'one column': [[put(100, 60, 'A Plain Document', 300, 24), ...body(100, 100, 6, prose, 640), put(100, 220, 'A new paragraph that ends early.', 200), ...body(120, 240, 3, prose, 620), put(440, 1120, '12', 20, 12)]],
  'two columns': [[put(200, 40, 'Deep Residual Learning', 500, 24), put(300, 70, 'Someone Else', 300, 13), ...twoColumn()]],
  headings: [[put(L, 60, 'Abstract', 80), ...body(L, 80, 4), put(L, 160, '1. Introduction', 120), ...body(L, 180, 4), put(L, 260, '3.1 Our Method', 110), ...body(L, 280, 4), put(L, 360, 'IV. Results', 90), ...body(L, 380, 4), put(L, 460, '一、背景', 70), ...body(L, 480, 4), put(L, 560, '第二章 方法', 90), ...body(L, 580, 4), put(L, 660, '摘要', 40), put(L, 680, '10 The responses are good', 200), put(L, 700, '3 x3 conv', 70), put(L, 720, 'K. Anderson', 80), put(L, 740, '4 For each of them', 130), put(300, 760, '7 Introductions', 110), put(500, 760, 'beside', 60), put(L, 780, 'A very long numbered heading written to fill all of the column that it sits in. And more', 340), put(L, 800, '2. Big Heading', 300, 20), put(L, 824, 'continued on a second line', 300, 20)]],
  captions: [
    [put(L, 300, 'Figure 1. A picture of things.', 300), put(L, 100, 'axis', 40), put(L + 100, 200, 'legend', 60), ...body(L, 340, 5), put(L, 440, 'Table 2: Numbers here', 200), put(L, 460, '1.0 2.0', 60), put(L + 100, 480, '3.0 4.0', 60), put(L, 700, '图 3：系统结构', 120), put(L, 620, '组件', 40), put(L, 900, 'Fig. 3) shows it', 120), put(L, 920, 'Figure 1. A picture again here', 200)],
    [put(L, 60, 'Table 2 (continued): More numbers', 260), put(L, 80, '5.0 6.0', 60), put(L, 100, 'Table S3. Without cells', 200), ...body(L, 300, 4), put(L, 500, 'Figure 9. No text around it', 220)],
  ],
  equations: [[...body(L, 80, 3), put(L, 140, 'y = f(x) + x (1)', 200), put(200, 170, 'a = b + c', 100), put(400, 170, '(2)', 24), put(L, 200, '(3)', 24), ...body(L, 230, 3), put(L, 300, 'a value that has no sign in it (4)', 300)]],
  references: [
    [...body(L, 80, 3), put(L, 200, 'References', 90), put(L, 220, '[1] A. Author. A title. 2015.', 300), put(L, 236, 'more of that entry here', 200), put(L, 252, '[2] B. Author. Another. 2016.', 300), put(L, 268, '(3) C. Author. Third. 2017.', 300), put(L, 284, '4. D. Author. Fourth. 2018.', 300)],
    [put(L, 60, '[5] E. Author. Fifth. 2019.', 300), put(L, 80, 'Appendix A', 90), ...body(L, 100, 3)],
    [put(L, 60, '参考文献', 80), put(L, 80, '[1] 作者. 题目. 2020.', 300), put(L, 100, '2. Big Numbered Part', 300, 20)],
  ],
  scanned: [[], [put(440, 500, '7', 20, 12)], [put(800, 40, 'S-6', 30, 12), put(800, 60, 'A figure with a page mark', 200)], [put(10, 1140, 'S-', 12, 12), put(23, 1140, '6', 8, 12)]],
  superscripts: [[put(L, 100, 'The value of', 90), put(L + 92, 96, '10', 12, 8), put(L + 106, 100, 'and then', 60), put(L, 120, 'Some ', 40), put(L + 40, 117, 'italic', 40, 13), put(L + 84, 120, 'words', 40), put(L, 140, 'broken automa-', 340), put(L, 156, 'tion across a line of text in this paragraph here which goes on', 340), put(L, 172, 'double spaced', 340), put(L, 210, 'text keeps going in this paragraph across a large gap in lines', 340), put(L, 248, 'and ends here.', 100)]],
  vertical: [[...[...'竖排的文字'].map((t, i) => put(600, 100 + i * 20, t, 20, 20)), ...[...'第二列文字在这'].map((t, i) => put(570, 100 + i * 20, t, 20, 20)), ...[...'另外一块独立'].map((t, i) => put(200, 300 + i * 20, t, 20, 20)), ...[...'ab'].map((t, i) => put(400, 100 + i * 20, t, 20, 20)), put(100, 80, '正常横排一行字', 140, 20)]],
  rotated: [[put(20, -700, 'arXiv:1512.03385v1  [cs.CV]', 300, 14, 1.5707963), ...twoColumn(100, 4), put(300, 300, 'Sideways heading text', 200, 14, 0.5), ...body(300, 320, 3, prose, 300).map((p) => ({ ...p, angle: 0.5 }))]],
  table: [[put(L, 100, '9', 12), put(L + 30, 100, 'Agent tool call', 120), put(L + 200, 100, '1,835', 40), put(L, 120, '10', 12), put(L + 30, 120, 'Another call', 120), put(L + 200, 120, '2,001', 40), put(L, 140, 'https://example.com/a', 200), put(L, 160, 'A short line.', 100)]],
  'two column figures': [[put(200, 40, 'Title Here', 300, 24), ...twoColumn(80, 5), put(L, 300, 'Figure 2. On the left.', 300), put(L + 20, 200, 'x', 20), put(R, 300, 'Table 1. On the right.', 300), put(R + 20, 320, '1', 12), put(R + 20, 340, '2', 12), ...body(L, 400, 8), ...body(R, 400, 8), put(150, 600, 'Figure 3. Across both columns of the whole page, a wide one.', 600), put(200, 560, 'wide', 40)]],
}
const paperCases = Object.entries(paperDocuments).map(([name, pages]) => {
  const state = { inRefs: false }
  return { name, pages: pages.map((pieces, i) => {
    const { areas, headings, columns } = analyzePage(pieces.map((p) => ({ ...p })), P, Q, state)
    return { w: P, h: Q, pieces, areas, headings, columns, inRefs: state.inRefs, graph: pageGraph({ w: P, h: Q, no: i + 1 }, areas) }
  }) }
})
// Finding blocks in an image by its pixels, and tidying what a vision model says about them.
const { segment } = await load('src/board/segment.ts')
const imageSource = readFileSync(join(root, 'src/board/importImage.ts'), 'utf8')
if (!imageSource.includes('export function cleanRead')) throw new Error('importImage.ts moved')
// importImage.ts reaches for the desktop shell, so its two pure functions are taken on their own here.
const pureFunctions = imageSource.slice(imageSource.indexOf('export function cleanRead'), imageSource.indexOf('export interface ImageImport'))
const { cleanRead, meaningful } = await import(`data:text/javascript,${encodeURIComponent(pureFunctions.replace(/: string/g, '').replace(/: boolean/g, ''))}`)
const glyphRow = (x, y, n, size = 14, step = 18) => Array.from({ length: n }, (_, i) => [x + i * step, y, size, size])
const segmentCases = [
  { name: 'text, a box and an arrow', w: 600, h: 300, paper: 255, ink: 20, rects: [...glyphRow(20, 20, 16), ...glyphRow(20, 42, 16), [380, 120, 160, 2], [380, 200, 160, 2], [380, 120, 2, 82], [538, 120, 2, 82], ...glyphRow(440, 154, 2), [200, 160, 180, 2], ...glyphRow(20, 260, 6)] },
  { name: 'a blackboard', w: 400, h: 200, paper: 30, ink: 230, rects: [...glyphRow(20, 30, 8), ...glyphRow(20, 120, 5, 20, 26)] },
  { name: 'nothing', w: 100, h: 100, paper: 255, ink: 0, rects: [] },
  { name: 'specks', w: 200, h: 200, paper: 255, ink: 0, rects: [[10, 10, 2, 2], [50, 50, 1, 8], [90, 90, 9, 1]] },
  { name: 'a table', w: 500, h: 300, paper: 250, ink: 10, rects: [[20, 20, 460, 2], [20, 100, 460, 2], [20, 180, 460, 2], [20, 20, 2, 162], [240, 20, 2, 162], [478, 20, 2, 162], ...glyphRow(40, 50, 5), ...glyphRow(280, 50, 5), ...glyphRow(40, 130, 5), ...glyphRow(280, 130, 5)] },
  { name: 'sizes that stay apart', w: 500, h: 300, paper: 255, ink: 0, rects: [...glyphRow(20, 20, 6, 30, 36), ...glyphRow(20, 60, 12, 10, 13), ...glyphRow(20, 74, 12, 10, 13)] },
  { name: 'a thick line kept as ink', w: 400, h: 200, paper: 255, ink: 0, rects: [[20, 50, 300, 16], ...glyphRow(20, 120, 4)] },
  { name: 'transparent ground', w: 300, h: 120, paper: 255, ink: 0, clear: true, rects: [...glyphRow(20, 30, 6)] },
  { name: 'a stroke inside a block', w: 300, h: 200, paper: 255, ink: 0, rects: [[20, 20, 200, 60], [30, 100, 4, 4], [20, 90, 200, 20]] },
].map((c) => {
  const data = new Uint8ClampedArray(c.w * c.h * 4).fill(255)
  for (let i = 0; i < c.w * c.h; i++) data[i * 4] = data[i * 4 + 1] = data[i * 4 + 2] = c.paper
  for (const [x0, y0, w, h] of c.rects) for (let y = y0; y < y0 + h; y++) for (let x = x0; x < x0 + w; x++) { const i = (y * c.w + x) * 4; data[i] = data[i + 1] = data[i + 2] = c.ink }
  if (c.clear) for (let i = 0; i < c.w * c.h; i++) if (data[i * 4] === 255) data[i * 4 + 3] = 0
  return { ...c, blocks: segment({ width: c.w, height: c.h, data }).map((b) => [b.x, b.y, b.w, b.h]) }
})
const readCases = ['「订单服务」', '```\nAPI 网关\n```', '<think>嗯</think>MySQL', '<think>没完', '无', '无。', '没有文字', 'None', 'n/a.', '（无）', '(无)', '  a   b\n c ', '1>1', '->', '', '写库', 'HTTPS', '2026', '12', '"quoted\'', '无论如何'].map((input) => ({ input, clean: cleanRead(input), meaningful: meaningful(input) }))
// Telling voices apart, and remembering who raised what.
const { identify, nextName, renameVoice } = await load('src/engine/voices.ts')
const { attribute, renameBy } = await load('src/engine/attribute.ts')
const { initial } = await load('src/ui/speakers.ts')
const voicePerson = (seed) => Array.from({ length: 16 }, (_, i) => Math.sin(seed * 12.9898 + i * 78.233) * 0.5)
const voiceSay = (person, noise, k) => {
  const v = person.map((x, i) => x + noise * Math.sin(k * 3.17 + i * 1.91))
  const len = Math.hypot(...v)
  return v.map((x) => x / len)
}
const voiceCases = [
  { name: 'two people taking turns', who: [1, 1, 2, 1, 2, 2], noise: 0.1, taken: [], renames: [['A', '李工']] },
  { name: 'three people, one of them new late on', who: [1, 2, 1, 2, 3, 3, 1], noise: 0.15, taken: [], renames: [['C', '王工'], ['王工', 'A']] },
  { name: 'names already taken by hand', who: [1, 2], noise: 0.1, taken: ['A', 'C'], renames: [['B', 'A'], ['missing', 'X'], ['A', 'A']] },
  { name: 'noise that splits one voice', who: [4, 4, 4], noise: 3, taken: [], renames: [] },
  { name: 'many sentences, past the memory limit', who: Array.from({ length: 40 }, () => 5), noise: 0.2, taken: [], renames: [] },
].map((c) => {
  let voices = []
  const steps = c.who.map((who, k) => {
    const print = voiceSay(voicePerson(who), c.noise, k)
    const r = identify(voices, print, c.taken)
    voices = r.voices
    return { print, taken: c.taken, name: r.name, voices }
  })
  const renames = c.renames.map(([from, to]) => {
    voices = renameVoice(voices, from, to)
    return { from, to, voices }
  })
  return { name: c.name, steps, renames }
})
const nameCases = [[], ['A'], ['A', 'B', 'D'], Array.from({ length: 26 }, (_, i) => String.fromCharCode(65 + i)), [...Array.from({ length: 26 }, (_, i) => String.fromCharCode(65 + i)), 'A2']].map((taken) => ({ taken, next: nextName(taken) }))
const initialCases = ['李工', '小王', '老张', '阿强', '小', 'Alice', ' bob ', '说话人 2', 'Speaker 123', 'A2', '...', '', '🙂x', 'élan', '王', '田中さん', '-1a'].map((name) => ({ name, initial: initial(name) }))
const attributionNode = (id, label, more = {}) => ({ id, label, aliases: [], kind: 'node', marks: [], ink: 'ink', ...more })
const attributionEdge = (from, to, more = {}) => ({ from, to, ink: 'ink', ...more })
const attributionGraph = (nodes, edges = []) => ({ nodes, edges, groups: [], nextNode: nodes.length + 1, nextGroup: 1 })
const attributionCases = [
  { name: 'one speaker raises it all', before: attributionGraph([]), after: attributionGraph([attributionNode('n1', '网关'), attributionNode('n2', '登录')], [attributionEdge('n1', 'n2', { label: '转给' })]), batch: [{ speaker: 'A', text: '网关转给登录' }] },
  { name: 'two speakers, by what each said', before: attributionGraph([]), after: attributionGraph([attributionNode('n1', '网关'), attributionNode('n2', '用户库', { aliases: ['user db'] })], [attributionEdge('n1', 'n2')]), batch: [{ speaker: 'B', text: '先过网关' }, { speaker: 'A', text: '然后查 user db' }] },
  { name: 'what was there stays', before: attributionGraph([attributionNode('n1', '网关', { by: 'B' }), attributionNode('n2', '登录')], [attributionEdge('n1', 'n2')]), after: attributionGraph([attributionNode('n1', '网关', { by: 'B' }), attributionNode('n2', '登录'), attributionNode('n3', '日志')], [attributionEdge('n1', 'n2'), attributionEdge('n2', 'n3')]), batch: [{ speaker: 'A', text: '写日志' }] },
  { name: 'nobody said it', before: attributionGraph([]), after: attributionGraph([attributionNode('n1', '网关')]), batch: [{ speaker: 'A', text: '嗯' }, { speaker: 'B', text: '对' }] },
  { name: 'no speakers', before: attributionGraph([]), after: attributionGraph([attributionNode('n1', '网关')]), batch: [{ text: '网关' }] },
  { name: 'a single character', before: attributionGraph([]), after: attributionGraph([attributionNode('n1', '甲')]), batch: [{ speaker: 'A', text: '乙' }, { speaker: 'B', text: '甲' }] },
  { name: 'a tie goes to the first', before: attributionGraph([]), after: attributionGraph([attributionNode('n1', '网关')]), batch: [{ speaker: 'B', text: '网关' }, { speaker: 'A', text: '网关' }] },
  { name: 'a draft turned to ink', before: attributionGraph([attributionNode('n1', '网关', { ink: 'draft' })]), after: attributionGraph([attributionNode('n1', '网关')]), batch: [{ speaker: 'A', text: '网关' }] },
].map((c) => {
  const attributed = attribute(c.before, c.after, c.batch)
  return { ...c, attributed, renamed: renameBy(attributed, 'A', '甲') }
})
const prefix = (text) => messages(emptyGraph(), [], { text }).slice(0, -1)
// The fixed part of the prompt ships with the agent; this copy is the answer key it is tested against.
writeFileSync(new URL('../../agent/prompts.json', import.meta.url), `${JSON.stringify({ zh: prefix('今天讨论登录'), en: prefix('Let us discuss the login flow today') }, null, 1)}\n`)
writeFileSync(process.argv[3], `${JSON.stringify({
  prompts: { zh: prefix('今天讨论登录'), en: prefix('Let us discuss the login flow today') },
  steps,
  stress,
  summaries,
  tidy,
  plain,
  markdown: markdownCases,
  ranks: rankCases,
  spans: spanCases,
  arxiv: arxivCases,
  segments: segmentCases,
  reads: readCases,
  paper: paperCases,
  pages: pageCases,
  timelines: timelineCases,
  voices: voiceCases,
  names: nameCases,
  initials: initialCases,
  attribution: attributionCases,
})}\n`)
console.log(`${steps.length} steps`)
