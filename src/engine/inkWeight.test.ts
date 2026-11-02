import assert from 'node:assert/strict'
import { test } from 'node:test'
import { compose, type InkLook } from '../ink/compose.ts'
import { glyph } from '../ink/glyphs.ts'
import { emptyGraph, type Node } from './graph.ts'
import type { Layout } from '../board/layout.ts'

test('English weight changes only Latin strokes inside mixed Chinese/Japanese text', async (t) => {
  const host = globalThis as typeof globalThis & { __boardcastHanzi?: Record<string, number[][][]> }
  const previous = host.__boardcastHanzi
  host.__boardcastHanzi = { 中: [[[100, 800], [800, 100]]], ア: [[[100, 700], [900, 700], [300, 200]]] }
  t.after(() => { host.__boardcastHanzi = previous })
  const node: Node = { id: 'n1', kind: 'item', label: 'A中ア，', aliases: [], marks: [], ink: 'ink' }
  const graph = { ...emptyGraph(), nodes: [node] }
  const layout: Layout = {
    regions: [{ id: 'r1', x: 0, y: 0, w: 200, h: 100, rows: [{ id: node.id, node, prefix: '', text: node.label, x: 0, y: 0, w: 100, size: 24 }] }],
    links: [], anchors: new Map(), bounds: { x0: 0, y0: 0, x1: 200, y1: 100 },
  }
  const look: InkLook = { paper: 'white', ink: 'black', pencil: 'gray', red: 'red', blue: 'blue', highlight: 'yellow', chalk: false }
  const original = await compose(graph, layout, new Set(), look)
  const boldEnglish = await compose(graph, layout, new Set(), look, 0, 0.65, undefined, undefined, 1.5)
  const boldCjk = await compose(graph, layout, new Set(), look, 0, 1.5)
  const latinCount = (await glyph('A'))!.strokes.length
  assert(latinCount > 0)
  assert.notDeepEqual(original[0].strokes.slice(0, latinCount), boldEnglish[0].strokes.slice(0, latinCount))
  assert.deepEqual(original[0].strokes.slice(latinCount), boldEnglish[0].strokes.slice(latinCount))
  assert.deepEqual(original[0].strokes.slice(0, latinCount), boldCjk[0].strokes.slice(0, latinCount))
  assert.equal((await glyph('Ａ'))?.latin, true)
  assert(!((await glyph('，'))?.latin))
})
