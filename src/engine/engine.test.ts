import assert from 'node:assert/strict'
import { test } from 'node:test'
import { emptyGraph, findNode } from './graph.ts'
import { applyOps, eraseDrafts, formatOps, markDrafts, parseOps } from './ops.ts'
import { grammar } from './prompt.ts'
import { mentions, quickOps } from './rules.ts'

test('parse and apply a small batch', () => {
  const ops = parseOps('E 「用户」 「小程序」 「打开」\nE 「小程序」 「登录接口」')
  assert.equal(ops.length, 2)
  const g = applyOps(emptyGraph(), ops)
  assert.deepEqual(
    g.nodes.map((n) => n.label),
    ['用户', '小程序', '登录接口'],
  )
  assert.equal(g.edges.length, 2)
  assert.equal(g.edges[0].label, '打开')
})

test('ids, corrections and groups', () => {
  let g = applyOps(emptyGraph(), parseOps('E 「小程序」 「登录接口」'))
  g = applyOps(g, parseOps('X n1 n2\nE n1 「网关」\nE 「网关」 n2'))
  assert.deepEqual(
    g.edges.map((e) => `${e.from}>${e.to}`),
    ['n1>n3', 'n3>n2'],
  )
  g = applyOps(g, parseOps('G 「入口」 n1 n3\nA n2 「login」'))
  assert.equal(g.groups.length, 1)
  assert.equal(findNode(g, 'LOGIN')?.id, 'n2')
  assert.equal(g.nodes.find((n) => n.id === 'n3')?.group, 'g1')
})

test('rename keeps the old name as alias', () => {
  let g = applyOps(emptyGraph(), parseOps('N 「老系统」'))
  g = applyOps(g, parseOps('R n1 「旧版后台」'))
  assert.equal(findNode(g, '老系统')?.label, '旧版后台')
})

test('bad lines are skipped, bare names fall back to lookup', () => {
  const ops = parseOps('嗯我觉得\nE n9\nE 小程序 「后端」\n-')
  assert.equal(ops.length, 1)
  const g = applyOps(emptyGraph(), ops)
  assert.equal(g.edges.length, 1)
})

test('format round-trips', () => {
  const text = 'E n1 -> 「网关」 : 「转发」\nG 「交易域」 n1 n2\nD n4'
  assert.equal(formatOps(parseOps(text)), text)
})

test('drafts become ink when confirmed and are erased otherwise', () => {
  let g = applyOps(emptyGraph(), quickOps('画一下：前端到网关，然后到订单服务'), 'draft')
  assert.equal(g.edges.length, 2)
  assert.ok(g.nodes.every((n) => n.ink === 'draft'))
  const mark = markDrafts(g)
  g = applyOps(g, parseOps('E 「前端」 「网关」'), 'ink')
  // 模型跑的时候又来了一条铅笔稿，它不该被这一轮擦掉
  g = applyOps(g, quickOps('A -> B'), 'draft')
  g = eraseDrafts(g, mark)
  assert.deepEqual(
    g.nodes.map((n) => `${n.label}:${n.ink}`),
    ['前端:ink', '网关:ink', 'A:draft', 'B:draft'],
  )
})

test('quick lane ignores ordinary talk', () => {
  assert.equal(quickOps('我觉得用户到时候会很多').length, 0)
  assert.equal(quickOps('支付 -> 账务').length, 1)
})

test('mentions find labels and aliases', () => {
  const g = applyOps(emptyGraph(), parseOps('N 「库存服务」\nA n1 「stock」'))
  assert.deepEqual(mentions(g, '那 Stock 这边怎么扣？'), ['n1'])
})

test('grammar only allows existing ids', () => {
  const g = applyOps(emptyGraph(), parseOps('E 「甲」 「乙」'))
  const gbnf = grammar(g)
  assert.match(gbnf, /ref ::= name \| "n1" \| "n2"|ref ::= name \| "n2" \| "n1"/)
  assert.match(grammar(emptyGraph()), /ref ::= name$/m)
})
