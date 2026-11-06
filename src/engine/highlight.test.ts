import assert from 'node:assert/strict'
import { test } from 'node:test'
import { emptyGraph, type Graph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { grammar } from './prompt.ts'
import { toggleMark } from './edit.ts'
import { toggleSpan } from './spans.ts'
import { diffGraph, replayTo } from './steps.ts'
import { compose, type InkLook } from '../ink/compose.ts'
import { ranks, timeline } from '../ink/animate.ts'
import type { Layout } from '../board/layout.ts'
import { toMarkdown } from '../board/markdown.ts'
import { withPageUnderlay, withoutPageUnderlay, type FreeScene } from '../board/freeScene.ts'

const source = (): Graph => ({ ...emptyGraph(), underlay: { w: 900, h: 680 }, nextNode: 2, nodes: [{
  id: 'n1', kind: 'region', label: 'Alpha source…', text: 'Alpha source text continues across two lines.', aliases: [], marks: [], ink: 'ink',
  at: { x: 50, y: 100, w: 240, h: 44 }, rows: [
    { y: 100, h: 16, segs: [{ t: 'Alpha source text', x: 50, w: 150 }] },
    { y: 128, h: 16, segs: [{ t: 'continues across two lines.', x: 50, w: 240 }] },
  ],
}] })
const look: InkLook = { paper: 'white', ink: 'black', pencil: 'gray', red: 'red', blue: 'blue', highlight: 'yellow', chalk: false }
const layout: Layout = { regions: [], links: [], anchors: new Map(), bounds: { x0: 0, y0: 0, x1: 900, y1: 680 } }

test('a circle spanning a vertical column break does not connect across unselected text', async () => {
  const g = source()
  const boxes = [{ x: 212, y: 260, w: 20, h: 20 }, { x: 176, y: 100, w: 20, h: 20 }]
  g.nodes[0].spans = [{ mark: '圈出', text: '会议', boxes }]
  const pieces = await compose(g, layout, new Set(), look)
  assert.equal(pieces.length, 1)
  assert.equal(pieces[0].strokes.length, 2)
  for (const [i, stroke] of pieces[0].strokes.entries()) {
    const ys = [...stroke.center.matchAll(/(-?\d+\.\d+)[ ,]+(-?\d+\.\d+)/g)].map(m => Number(m[2]))
    const b = boxes[i]
    assert(Math.min(...ys) > b.y - 12 && Math.max(...ys) < b.y + b.h + 12)
  }
})

test('highlight uses the existing graph, source spans, toggles and replay operations', () => {
  const original = source()
  const g = applyOps(original, parseOps('M n1 高亮 句 「Alpha」'))
  const span = g.nodes[0].spans![0]
  assert.equal(span.mark, '高亮')
  assert.equal(span.boxes.length, 2)
  assert.equal(span.text, original.nodes[0].text)
  assert.equal(toggleSpan(g, 'n1', span).nodes[0].spans, undefined)
  const block = toggleMark(original, 'n1', '高亮')
  assert.deepEqual(toggleMark(block, 'n1', '高亮').nodes[0].marks, [])
  const steps = [{ t: 1, line: 0, d: diffGraph(emptyGraph(), original)! }, { t: 2, line: 1, d: diffGraph(original, g)! }]
  assert.deepEqual(replayTo(steps, 2), g)
  assert.match(grammar(g), /高亮/)
  assert.deepEqual(original.nodes[0].marks, [])
})

test('persistent highlights render without temporary speech state and keep source boxes', async () => {
  const base = source()
  const g = applyOps(toggleMark(base, 'n1', '高亮'), parseOps('M n1 高亮 句 「Alpha」'))
  const pieces = await compose(g, layout, new Set(), look)
  assert.equal(pieces.length, 2)
  assert(pieces.every(p => p.under && p.color === 'yellow' && p.opacity === 0.45))
  assert.deepEqual(pieces.map(p => p.strokes.length), [1, 2])
  const continued = await compose(g, layout, new Set(), look, undefined, undefined, g)
  assert(continued.every(p => p.opacity < 0.45))
  assert.deepEqual(continued.map(p => p.strokes), pieces.map(p => p.strokes))
  const traced = await compose(g, layout, new Set(), look, undefined, undefined, undefined, { who: 'Other', color: 'green' })
  assert(traced.every(p => p.opacity < 0.45))
})

test('rotated highlights and underlines follow source direction instead of local page axes', async () => {
  const g = source()
  g.nodes[0].spans = (['高亮', '下划线', '划掉', '圈出'] as const).map(mark => ({ mark, text: 'Rotated', boxes: [{ x: 100, y: -250, w: 120, h: 20, angle: Math.PI / 2 }] }))
  const pieces = await compose(g, layout, new Set(), look)
  assert.equal(pieces.length, 4)
  for (const piece of pieces) {
    const points = piece.strokes.flatMap(s => [...s.center.matchAll(/(-?\d+\.\d+)[ ,]+(-?\d+\.\d+)/g)].map(m => [Number(m[1]), Number(m[2])]))
    const xs = points.map(p => p[0]), ys = points.map(p => p[1])
    assert(Math.min(...xs) > 210 && Math.max(...xs) < 275, piece.key)
    assert(Math.min(...ys) > 85 && Math.max(...ys) < 235, piece.key)
    assert(Math.max(...ys) - Math.min(...ys) > 100, piece.key)
  }
})

test('normal text highlight and animated export wait for the mark step', async () => {
  const original = applyOps(emptyGraph(), parseOps('T 「Decision」'))
  const g = toggleMark(original, 'n1', '高亮'), n = g.nodes[0]
  const l = { ...layout, regions: [{ id: 'r1', x: 0, y: 0, w: 200, h: 80, rows: [{ id: n.id, node: n, prefix: '', text: n.label, x: 0, y: 0, w: 100, size: 24 }] }] }
  const pieces = await compose(g, l, new Set(), look)
  const steps = [{ t: 1, line: 0, d: diffGraph(emptyGraph(), original)! }, { t: 2, line: 1, d: diffGraph(original, g)! }]
  const order = ranks(g, l, steps, pieces.map(p => p.key))
  assert.equal(order[pieces.findIndex(p => p.key.endsWith('|h'))], 1)
  const timed = timeline(pieces, order).timed.find(t => t.piece.under)!
  assert(timed.delays[0] > 0)
  assert.deepEqual(timed.durs, [0])
})

test('striking a short multiline source block crosses its text instead of its line gap', async () => {
  const g = toggleMark(source(), 'n1', '划掉')
  g.nodes[0].at!.h = 39
  const pieces = await compose(g, layout, new Set(), look)
  assert.equal(pieces.find(p => p.key === 'n1|x')!.strokes.length, 2)
})

test('Markdown retains full annotated source text and labels flowchart highlights', () => {
  const g = applyOps(toggleMark(source(), 'n1', '高亮'), parseOps('M n1 高亮 「Alpha」'))
  const text = toMarkdown(g)
  assert(text.includes(`<mark>${g.nodes[0].text}</mark>`))
  assert.match(text, /(?:Highlight|高亮)[：:]\s*Alpha/)
  const diagram = applyOps(emptyGraph(), parseOps('N 「Decision」\nM n1 高亮'))
  assert.match(toMarkdown(diagram), /Decision[：:]\s*(?:Highlight|高亮)/)
  const formula = applyOps(emptyGraph(), parseOps('F 「E = mc²」\nM n1 高亮'))
  assert(toMarkdown(formula).includes('<mark>`E = mc²`</mark>'))
})

test('display/export underlay is locked, collision-safe and removed before saving edits', () => {
  const state: FreeScene = { elements: [], generated: [], files: { __boardcast_underlay__: { id: '__boardcast_underlay__', dataURL: 'data:image/png;base64,AAAA', mimeType: 'image/png', created: 0 } } as unknown as FreeScene['files'] }
  const drawn = withPageUnderlay(state, source(), 'data:image/png;base64,BBBB')
  assert.equal(drawn.elements[0].id, '__boardcast_underlay___')
  assert.equal(drawn.elements[0].locked, true)
  assert.deepEqual(withoutPageUnderlay(drawn), state)
  assert.equal(withPageUnderlay(state, emptyGraph(), 'data:image/png;base64,BBBB'), state)
  assert.throws(() => withPageUnderlay(state, source(), 'https://example.invalid/private.png'))
  assert.throws(() => withPageUnderlay(state, { ...source(), underlay: { w: -1, h: 20 } }, 'data:image/png;base64,BBBB'))
  assert.equal(state.elements.length, 0)
})
