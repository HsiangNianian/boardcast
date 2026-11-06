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
const prefix = (text) => messages(emptyGraph(), [], { text }).slice(0, -1)
// The fixed part of the prompt ships with the agent; this copy is the answer key it is tested against.
writeFileSync(new URL('../../agent/prompts.json', import.meta.url), `${JSON.stringify({ zh: prefix('今天讨论登录'), en: prefix('Let us discuss the login flow today') }, null, 1)}\n`)
writeFileSync(process.argv[3], `${JSON.stringify({
  prompts: { zh: prefix('今天讨论登录'), en: prefix('Let us discuss the login flow today') },
  steps,
  stress,
})}\n`)
console.log(`${steps.length} steps`)
