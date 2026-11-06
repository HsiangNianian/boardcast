import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { RegionRow } from './graph.ts'
import { applyOps, formatOps, parseOps } from './ops.ts'
import { pageGraph } from './regions.ts'
import { grammar } from './prompt.ts'
import { describe } from './graph.ts'
import { locate, rotatePoint, spanAt, toggleSpan } from './spans.ts'

const row = (y: number, t: string, x = 100, w = 600): RegionRow => ({ y, h: 18, segs: [{ t, x, w }] })

test('vertical word clicks distinguish equal-height columns and keep a word across a column break', () => {
  const column = (x: number, text: string) => [...text].map((t, i) => row(100 + i * 20, t, x, 20))
  const source = [...column(212, '本地系统需要保存会'), ...column(176, '议记录︒下一句︒')]
  const first = spanAt(source, 222, 270, '高亮', 'word')!
  assert.equal(first.text, '会议')
  assert.deepEqual(spanAt(source, 186, 110, '高亮', 'word'), first)
  assert.deepEqual(first.boxes, [{ x: 212, y: 260, w: 20, h: 18 }, { x: 176, y: 100, w: 20, h: 18 }])
  assert.equal(spanAt(source, 186, 110, '圈出', 'sentence')!.text, '本地系统需要保存会议记录︒')
  const neighbors = [...column(622, '右列需要保存甲项︒'), ...column(586, '左列需要保存乙项︒')]
  for (const [x, sentence] of [[622, '右列需要保存甲项︒'], [586, '左列需要保存乙项︒']] as const) {
    const word = spanAt(neighbors, x + 10, 190, '高亮', 'word')!
    assert.equal(word.text, '保存')
    assert(word.boxes.every(b => b.x === x))
    assert.equal(spanAt(neighbors, x + 10, 190, '高亮', 'sentence')!.text, sentence)
  }
})

test('rotated source words and sentences retain their text coordinates through save and hit testing', () => {
  for (const angle of [Math.PI / 2, -Math.PI / 2, Math.PI, 25 * Math.PI / 180]) {
    const source = [{ ...row(80, 'Keep selection here.', 60, 200), angle }, { ...row(110, 'Second sentence.', 60, 180), angle }]
    const expected = locate(source, 'selection', '高亮')!
    const b = expected.boxes[0]
    const point = rotatePoint([b.x + b.w / 2, b.y + b.h / 2], angle)
    assert.deepEqual(spanAt(JSON.parse(JSON.stringify(source)), ...point, '高亮', 'word'), expected)
    assert.equal(spanAt(source, ...point, '圈出', 'sentence')!.text, 'Keep selection here.')
    assert.equal(b.angle, angle)
  }
})

test('vertical presentation punctuation closes its own sentence without changing source text', () => {
  const text = '﹁保存记录︒﹂下一句话︒'
  const source = [...text].map((t, i) => row(i * 20, t, 400, 20))
  assert.equal(spanAt(source, 410, 25, '高亮', 'sentence')!.text, '﹁保存记录︒﹂')
  assert.equal(locate(source, '保存记录。', '圈出', 'sentence')!.text, '﹁保存记录︒﹂')
  assert.equal(source.map(r => r.segs[0].t).join(''), text)
})
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

test('Chinese and Japanese word clicks select dictionary words instead of the whole unspaced phrase', () => {
  for (const [text, index] of [['系统需要保存会议记录。', 4], ['会議記録を保存します。', 5]] as const) {
    const rs = [row(0, text, 0, [...text].length * 20)]
    assert.equal(spanAt(rs, index * 20 + 10, 10, '高亮', 'word')!.text, '保存')
  }
})

test('mixed Latin and Chinese text keeps separate word targets and intact ASCII identifiers', () => {
  const rs = [{ y: 0, h: 20, segs: [{ t: '使用', x: 0, w: 40 }, { t: 'API', x: 40, w: 30 }, { t: '并保存记录。', x: 70, w: 120 }] }]
  assert.equal(spanAt(rs, 55, 10, '高亮', 'word')!.text, 'API')
  assert.equal(spanAt(rs, 100, 10, '高亮', 'word')!.text, '保存')
  for (const text of ['foo-bar', 'source_text']) assert.equal(spanAt([row(0, text, 0, 100)], 50, 10, '高亮', 'word')!.text, text)
  const mixed = [{ y: 0, h: 20, segs: [{ t: '使用', x: 0, w: 40 }, { t: 'foo-bar', x: 40, w: 70 }, { t: '工具', x: 110, w: 40 }] }]
  for (const x of [45, 75, 105]) assert.equal(spanAt(mixed, x, 10, '高亮', 'word')!.text, 'foo-bar')
})

test('a Chinese word split across PDF lines has two boxes and no inserted space', () => {
  const rs = [row(0, '本地系统需要及时保存每场会', 0, 260), row(30, '议记录，方便大家复盘。下一句。', 0, 300)]
  const first = spanAt(rs, 250, 10, '高亮', 'word')!
  const second = spanAt(rs, 10, 40, '高亮', 'word')!
  assert.equal(first.text, '会议')
  assert.deepEqual(first, second)
  assert.deepEqual(first.boxes, [{ x: 240, y: 0, w: 20, h: 18 }, { x: 0, y: 30, w: 20, h: 18 }])
  assert.equal(spanAt(rs, 170, 10, '高亮', 'sentence')!.text, '本地系统需要及时保存每场会议记录，方便大家复盘。')
  assert.equal(locate(rs, '会议', '高亮')!.text, '会议')
  assert.equal(locate([row(0, 'API'), row(30, '𠮷系统。')], 'API𠮷', '高亮')!.text, 'API𠮷')
})

test('sentence-ending closing quotes and repeated punctuation stay with their own sentence', () => {
  for (const text of ['「系统需要保存记录。」下一句。', '他说「保存记录？！」下一句。', 'He said "Save it." Next sentence.']) {
    const rs = [row(0, text)]
    const next = text.includes('Next') ? 'Next sentence.' : '下一句。'
    const first = text.slice(0, text.indexOf(next)).trim()
    assert.equal(locate(rs, text.includes('Save') ? 'Save' : '保存', '高亮', 'sentence')!.text, first)
    assert.equal(locate(rs, next, '高亮', 'sentence')!.text, next)
  }
  const rs = [row(0, '「保存。」下一句。', 0, 180)]
  assert.equal(spanAt(rs, 90, 5, '高亮', 'sentence')!.text, '「保存。」')
})

test('quoted ranges map UTF-16 matches back to whole PDF characters', () => {
  const rs = [row(0, '𠮷系统保存记录。', 0, 160)]
  assert.deepEqual(locate(rs, '保存', '高亮')!.boxes, [{ x: 60, y: 0, w: 40, h: 18 }])
  assert.equal(locate(rs, '保存', '高亮')!.text, '保存')
  assert.equal(locate(rs, '𠮷', '高亮')!.text, '𠮷')
  assert.equal(locate(rs, '𠮷系统', '高亮')!.text, '𠮷系统')
  const expanded = [row(0, 'İ Alpha 😀 保存。')]
  assert.equal(locate(expanded, 'Alpha', '高亮')!.text, 'Alpha')
  assert.equal(locate(expanded, 'İ', '高亮')!.text, 'İ')
  assert.equal(locate(expanded, '😀', '高亮')!.text, '😀')
})

test('quoted text matches PDF compatibility glyphs without rewriting the source', () => {
  const rs = [row(0, '使⽤PDF保存oﬃce记录。')]
  assert.equal(locate(rs, '使用PDF', '高亮')!.text, '使⽤PDF')
  assert.equal(locate(rs, 'office', '高亮')!.text, 'oﬃce')
  assert.equal(rs[0].segs[0].t, '使⽤PDF保存oﬃce记录。')
  const accent = [row(0, 'Cafe\u0301 and café')]
  for (const quote of ['Cafe\u0301', 'Café']) assert.equal(locate(accent, quote, '高亮')!.text, 'Cafe\u0301')
})

test('clicking punctuation does not expand a word selection into neighboring Chinese sentences', () => {
  const rs = [row(0, '保存记录。下一句。', 0, 180)]
  assert.equal(spanAt(rs, 90, 5, '高亮', 'word')!.text, '。')
  assert.equal(spanAt([], 0, 0, '高亮', 'word'), null)
})

test('new word boundaries preserve existing saved annotations and their toggle identity', () => {
  const rs = [row(0, '系统需要保存会议记录。', 0, 220)]
  const g = pageGraph({ w: 500, h: 300 }, [{ text: rs[0].segs[0].t, x: 0, y: 0, w: 220, h: 18, rows: rs }])
  const old = { mark: '高亮' as const, text: '系统需要保存会议记录', boxes: [{ x: 0, y: 0, w: 200, h: 18 }] }
  g.nodes[0].spans = [old]
  const before = JSON.stringify(g)
  const narrow = spanAt(rs, 90, 5, '高亮', 'word')!
  const next = toggleSpan(g, 'n1', narrow)
  assert.deepEqual(next.nodes[0].spans, [old, narrow])
  assert.deepEqual(toggleSpan(next, 'n1', narrow), g)
  assert.equal(JSON.stringify(g), before)
})
