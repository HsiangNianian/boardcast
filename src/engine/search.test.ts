import assert from 'node:assert/strict'
import { test } from 'node:test'
import { applyOps, parseOps } from './ops.ts'
import { pageGraph } from './regions.ts'
import { searchMeeting } from './search.ts'

const paper = pageGraph({ w: 800, h: 1000 }, [{ text: 'Computational chemistry has become an indispensable tool for probing and predicting systems today.', x: 0, y: 0, w: 700, h: 80 }])
const meeting = {
  id: 'm1',
  title: '客服系统',
  summary: '决定用自建方案。',
  lines: [
    { text: '豆包那块交给小王', page: 1 },
    { text: '删掉了「小王」', page: 1, manual: true },
  ],
  pages: [{ graph: applyOps(pageGraph({ w: 1, h: 1 }, []), parseOps('T 「部署方案」')) }, { graph: paper }],
}

test('hits say where they are, so the result can jump there', () => {
  assert.deepEqual(searchMeeting(meeting, '小王')?.map((h) => [h.where, h.line, h.page]), [['line', 0, 1]])
  assert.deepEqual(searchMeeting(meeting, '部署')?.map((h) => [h.where, h.page]), [['board', 0]])
  assert.equal(searchMeeting(meeting, '部署')?.[0].node, meeting.pages[0].graph.nodes[0].id)
  assert.equal(searchMeeting(meeting, '自建')?.[0].where, 'summary')
  assert.equal(searchMeeting(meeting, '客服')?.[0].where, 'title')
})

test('the full source text is searched, not just the short label', () => {
  const [h] = searchMeeting(meeting, 'PREDICTING')!
  assert.equal(h.page, 1)
  const [a, b] = h.marks[0]
  assert.equal(h.text.slice(a, b), 'predicting')
})

test('every word must appear somewhere in the meeting', () => {
  assert.equal(searchMeeting(meeting, '小王 部署')?.length, 2)
  assert.equal(searchMeeting(meeting, '小王 不存在'), null)
  assert.equal(searchMeeting(meeting, '   '), null)
})

test('repeating a search term does not hide matching meetings', () => {
  for (const [query, original] of [['小王 小王', '小王'], ['部署 小王 部署', '部署 小王'], ['PREDICTING predicting', 'predicting']]) {
    assert.deepEqual(searchMeeting(meeting, query), searchMeeting(meeting, original))
  }
})

test('overlapping search highlights preserve the snippet and every matched character', () => {
  for (const query of ['cache ca he', 'he cache ca', 'ca he cache']) {
    const [hit] = searchMeeting({ id: 'overlap', title: 'Cache' }, query)!
    assert.deepEqual(hit.marks, [[0, 5]])
  }
  const [hit] = searchMeeting({ id: 'overlap', title: 'abc' }, 'ab bc')!
  assert.deepEqual(hit.marks, [[0, 3]])
})

test('legacy transcript hits without a page point to the first board', () => {
  const [hit] = searchMeeting({ ...meeting, lines: [{ text: 'Legacy transcript' }] }, 'Legacy')!
  assert.equal(hit.where, 'line')
  assert.equal(hit.line, 0)
  assert.equal(hit.page, 0)
})
