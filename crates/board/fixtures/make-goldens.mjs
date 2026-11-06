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
  pages: pageCases,
  timelines: timelineCases,
})}\n`)
console.log(`${steps.length} steps`)
