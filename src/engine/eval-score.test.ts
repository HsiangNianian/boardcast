import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import { score, f1, revisionFailures } from '../../eval/score.ts'
import { emptyGraph, type Graph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

test('revision checks expose stale values even when fuzzy node F1 is perfect', () => {
  const cases = JSON.parse(readFileSync(new URL('../../eval/revisions.json', import.meta.url), 'utf8'))
  for (const c of cases) {
    const g: Graph = c.board
    const old = g.nodes[0].label
    const sc = score(g, c.gold)
    assert.equal(f1(sc.nodeP, sc.nodeR), 1)
    assert.deepEqual(revisionFailures(g, c.turns, c.expect), [
      `node n1: expected ${JSON.stringify(c.expect.nodes.n1)}, got ${JSON.stringify(old)}`,
    ])
    const corrected = applyOps(g, parseOps(`R n1 「${c.expect.nodes.n1}」`))
    assert.deepEqual(revisionFailures(corrected, c.turns, c.expect), [])
    assert.match(revisionFailures(corrected, c.history, c.expect)[0], /history capture:1: expected .*got /)
    assert.deepEqual(revisionFailures(corrected, [...c.turns, ...c.turns], c.expect), ['history capture:1: expected one entry, got 2'])
    corrected.nodes[0].id = 'n3'
    assert.match(revisionFailures(corrected, c.turns, c.expect)[0], /node n1: expected .*got null/)
  }
})

test('strike F1 penalizes false positives, misses and duplicate matches', () => {
  const g = applyOps(emptyGraph(), parseOps('N 「灰度发布」\nN 「全量发布」\nM n1 划掉\nM n2 划掉'))
  const s = score(g, { struck: [['灰度发布'], ['监控告警']] })
  assert.equal(s.strikeP, 0.5)
  assert.equal(s.strikeR, 0.5)
  assert.equal(f1(s.strikeP!, s.strikeR!), 0.5)
  assert.equal(score(g, {}).strikeP, null)
  assert.equal(score(g, { struck: [] }).strikeP, 0)
  assert.equal(score(emptyGraph(), { struck: [] }).strikeP, 1)
  assert.equal(score(emptyGraph(), { struck: [['灰度发布']] }).strikeR, 0)
  g.nodes[1].label = '灰度发布'
  assert.equal(score(g, { struck: [['灰度发布']] }).strikeP, 0.5)
})

test('pointing fixtures have unique IDs, valid selections and fully annotated strikes', () => {
  const cases = JSON.parse(readFileSync(new URL('../../eval/pointing.json', import.meta.url), 'utf8'))
  assert.equal(new Set(cases.map((c: { id: string }) => c.id)).size, cases.length)
  for (const c of cases) {
    const g: Graph = c.board
    for (const t of c.turns) for (const id of t.pointing) assert.ok(g.nodes.some((n) => n.id === id), `${c.id}: ${id}`)
    assert.equal(c.gold.struck.length + c.gold.not_struck.length, g.nodes.length)
    assert.equal(score(g, c.gold).itemR, c.id.endsWith('add-item') ? 0.75 : 1)
  }
})
