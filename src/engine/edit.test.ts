import assert from 'node:assert/strict'
import { test } from 'node:test'
import { History, describe, exists, remove, rename, toggleMark } from './edit.ts'
import { emptyGraph, findNode } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

const board = () =>
  applyOps(emptyGraph(), parseOps('T 「爬虫」\nL n1 「监控任务」\nL n2 「定时拉取」\nE 「用户」 「网关」 : 「请求」\nE 「网关」 「登录接口」'))

test('deleting a topic takes its items with it; deleting an edge keeps the nodes', () => {
  const g = board()
  const a = remove(g, { kind: 'node', id: 'n1' })
  assert.deepEqual(
    a.nodes.map((n) => n.label),
    ['用户', '网关', '登录接口'],
  )
  const b = remove(g, { kind: 'edge', from: 'n4', to: 'n5' })
  assert.equal(b.edges.length, 1)
  assert.equal(b.nodes.length, g.nodes.length)
  // 原图不变
  assert.equal(g.nodes.length, 6)
})

test('renaming keeps the old name as an alias; edge labels can be changed or cleared', () => {
  const g = board()
  const a = rename(g, { kind: 'node', id: 'n5' }, '  网关集群 ')
  assert.equal(findNode(a, '网关')?.label, '网关集群')
  assert.equal(rename(g, { kind: 'node', id: 'n5' }, '   '), g)
  const b = rename(g, { kind: 'edge', from: 'n4', to: 'n5' }, '转发')
  assert.equal(b.edges[0].label, '转发')
  const c = rename(g, { kind: 'edge', from: 'n4', to: 'n5' }, '')
  assert.equal(c.edges[0].label, undefined)
})

test('marks toggle on and off', () => {
  const g = board()
  const a = toggleMark(g, 'n2', '星标')
  assert.deepEqual(a.nodes.find((n) => n.id === 'n2')?.marks, ['星标'])
  const b = toggleMark(a, 'n2', '星标')
  assert.deepEqual(b.nodes.find((n) => n.id === 'n2')?.marks, [])
})

test('descriptions and existence checks for the transcript and stale selections', () => {
  const g = board()
  assert.equal(describe(g, { kind: 'node', id: 'n1' }), '「爬虫」')
  assert.equal(describe(g, { kind: 'edge', from: 'n4', to: 'n5' }), '「用户 → 网关」（请求）')
  assert.ok(exists(g, { kind: 'edge', from: 'n4', to: 'n5' }))
  assert.ok(!exists(remove(g, { kind: 'node', id: 'n5' }), { kind: 'edge', from: 'n4', to: 'n5' }))
})

test('undo and redo step through whole boards, and a new change drops the redo steps', () => {
  const h = new History()
  const g0 = emptyGraph()
  const g1 = board()
  const g2 = remove(g1, { kind: 'node', id: 'n1' })
  h.record(g0)
  h.record(g1)
  assert.equal(h.undo(g2), g1)
  assert.equal(h.undo(g1), g0)
  assert.equal(h.undo(g0), null)
  assert.equal(h.redo(g0), g1)
  assert.ok(h.canRedo)
  h.record(g1)
  assert.ok(!h.canRedo)
})
