import assert from 'node:assert/strict'
import { test } from 'node:test'
import { beautify } from '../board/beautify.ts'
import { type Box, StableLayout } from '../board/layout.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

const overlap = (a: Box, b: Box) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h

test('beautify preserves graph, editable anchors and layout across save/restore, with separated regions', async () => {
  const graph = applyOps(emptyGraph(), parseOps('T 「Release review」\nL 「Release review」 「Decision: keep the existing fallback」\nE 「Input」 「Length check」\nE 「Length check」 「Retry」\nE 「Retry」 「Font resize」\nE 「Length check」 「Output」\nE 「Font resize」 「Output」\nT 「Follow-up」\nL 「Follow-up」 「Verify alignment」\nQ 「Follow-up」 「Does the transcript match?」'))
  graph.groups.push({ id: 'g1', label: 'Pipeline' })
  graph.nodes.filter((n) => ['Input', 'Length check'].includes(n.label)).forEach((n) => { n.group = 'g1' })
  const original = structuredClone(graph)
  const board = new StableLayout()
  await board.layout(graph)
  const before = board.snapshot()
  const next = await beautify(graph, before)
  assert.deepEqual(graph, original)
  assert.notDeepEqual(next, before)
  board.restore(next)
  const layout = await board.layout(graph)
  for (const n of graph.nodes) assert.ok(layout.anchors.has(n.id), `${n.id} disappeared`)
  const areas = [...layout.regions, ...(layout.diagram ? [layout.diagram.box] : [])]
  for (let i = 0; i < areas.length; i++) for (let j = i + 1; j < areas.length; j++) assert.ok(!overlap(areas[i], areas[j]))
  assert.equal(layout.diagram?.edges.length, graph.edges.length)
  assert.equal(layout.diagram?.groups[0]?.label, 'Pipeline')
  const saved = board.snapshot()
  board.restore(JSON.parse(JSON.stringify(saved)))
  assert.deepEqual(await board.layout(graph), layout)
  const repeated = await beautify(graph, saved)
  board.restore(repeated)
  assert.deepEqual(await board.layout(graph), layout, 'a second beautify must not shuffle the board')
  board.restore(before)
  assert.deepEqual(board.snapshot(), before, 'undo restores every original coordinate')
})

test('beautify leaves imported document anchors in place and places other regions outside the page', async () => {
  const graph = applyOps(emptyGraph(), parseOps('T 「Notes」\nL 「Notes」 「Review source」'))
  graph.underlay = { w: 600, h: 800 }
  graph.nodes.push({ id: 'n99', kind: 'region', label: 'Source', aliases: [], marks: [], ink: 'ink', at: { x: 30, y: 40, w: 100, h: 30 } })
  const board = new StableLayout()
  await board.layout(graph)
  board.restore(await beautify(graph, board.snapshot()))
  const layout = await board.layout(graph)
  assert.deepEqual(layout.anchors.get('n99'), { id: 'n99', x: 30, y: 40, w: 100, h: 30 })
  assert.ok(layout.regions.every((r) => r.x > layout.page!.w))
})

test('children of questions and formulas remain visible in topic regions', async () => {
  const graph = applyOps(emptyGraph(), parseOps('T 「Review」\nQ 「Review」 「Limit?」'))
  const parent = graph.nodes.find((n) => n.kind === 'question')!
  graph.nodes.push({ id: 'n99', kind: 'note', label: 'Still under discussion', parent: parent.id, aliases: [], marks: [], ink: 'ink' })
  const layout = await new StableLayout().layout(graph)
  assert.ok(layout.anchors.has('n99'))
})

test('flowchart annotations retain nested notes and do not label confirmed facts as open questions', async () => {
  const graph = applyOps(emptyGraph(), parseOps('E 「Input」 「Output」\nF 「Input」 「max = 3」'))
  const parent = graph.nodes.find((n) => n.kind === 'formula')!
  graph.nodes.push({ id: 'n99', kind: 'note', label: 'Already implemented', parent: parent.id, aliases: [], marks: [], ink: 'ink' })
  graph.nodes.push({ id: 'n100', kind: 'note', label: 'Meeting context', aliases: [], marks: [], ink: 'ink' })
  const layout = await new StableLayout().layout(graph)
  for (const n of graph.nodes) assert.ok(layout.anchors.has(n.id), `${n.id} disappeared`)
  assert.equal(layout.regions.find((r) => r.id === '__loose')?.title?.text, 'Notes')
})
