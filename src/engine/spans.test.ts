import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { RegionRow } from './graph.ts'
import { applyOps, formatOps, parseOps } from './ops.ts'
import { pageGraph } from './regions.ts'
import { grammar } from './prompt.ts'
import { describe } from './graph.ts'
import { locate, spanAt, toggleSpan } from './spans.ts'

const row = (y: number, t: string, x = 100, w = 600): RegionRow => ({ y, h: 18, segs: [{ t, x, w }] })
const rows = [
  row(100, 'Computational chemistry is a tool for probing, predicting, and more. Most of the automa-'),
  row(135, 'tion today is done by hand, e.g. by Fig. 3 people. Nobody likes it.', 100, 480),
]

test('one word gets one small box where it is', () => {
  const s = locate(rows, 'Predicting', '圈出')!
  assert.equal(s.boxes.length, 1)
  assert.equal(s.text, 'predicting')
  const b = s.boxes[0]
  assert.ok(b.x > 350 && b.x + b.w < 560 && b.w < 100, JSON.stringify(b))
  assert.equal(b.y, 100)
})

test('a word broken at the line end is found and marked on both lines', () => {
  const s = locate(rows, 'automation', '下划线')!
  assert.equal(s.boxes.length, 2)
  assert.equal(s.text, 'automation')
})

test('a sentence spans lines, and abbreviations do not end it', () => {
  const s = locate(rows, 'Most of the', '圈出', 'sentence')!
  assert.equal(s.boxes.length, 2)
  assert.ok(s.text.startsWith('Most of the'), s.text)
  assert.ok(s.text.endsWith('people.'), s.text)
})

test('clicking picks the word or the sentence under the pointer', () => {
  const b = locate(rows, 'predicting', '圈出')!.boxes[0]
  assert.equal(spanAt(rows, b.x + b.w / 2, 105, '圈出', 'word')!.text, 'predicting')
  assert.equal(spanAt(rows, 500, 140, '圈出', 'sentence')!.text.slice(-13), 'Nobody likes it.'.slice(-13))
})

const board = () =>
  pageGraph({ w: 800, h: 1000 }, [
    { text: 'Heading text', x: 0, y: 0, w: 100, h: 20, rows: [row(0, 'Heading text', 0, 100)] },
    { text: rows.map((r) => r.segs[0].t).join(' '), x: 100, y: 100, w: 600, h: 53, rows },
  ])

test('the op marks a word, a sentence or the whole block', () => {
  const ops = parseOps('M n2 圈出 「predicting」\nM n2 下划线 句 「Nobody likes」\nM n1 星标')
  assert.equal(formatOps(ops), 'M n2 圈出 「predicting」\nM n2 下划线 句 「Nobody likes」\nM n1 星标')
  const g = applyOps(board(), ops)
  assert.deepEqual(g.nodes[1].marks, [])
  assert.deepEqual(g.nodes[1].spans!.map((s) => [s.mark, s.text]), [
    ['圈出', 'predicting'],
    ['下划线', 'Nobody likes it.'],
  ])
  assert.deepEqual(g.nodes[0].marks, ['星标'])
  assert.ok(describe(g).includes('圈出「predicting」'))
})

test('a word on the wrong block lands on the block that has it; unknown text marks the block', () => {
  const g = applyOps(board(), parseOps('M n1 圈出 「predicting」\nM n1 划掉 「no such words」'))
  assert.equal(g.nodes[1].spans![0].text, 'predicting')
  assert.deepEqual(g.nodes[0].marks, ['划掉'])
})

test('toggling removes a mark by hand', () => {
  const g = board()
  const s = locate(rows, 'predicting', '圈出')!
  const on = toggleSpan(g, 'n2', s)
  assert.equal(on.nodes[1].spans!.length, 1)
  assert.equal(toggleSpan(on, 'n2', s).nodes[1].spans, undefined)
})

test('the grammar allows a quoted range', () => {
  assert.ok(grammar(board()).includes('quote ::='))
})
