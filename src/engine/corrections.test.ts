import assert from 'node:assert/strict'
import { test } from 'node:test'
import { score } from '../../eval/score.ts'
import { Corrections, type Round, casesFile, exportable, fixedOf, opsBetween, pairsFile, toCase, toPair, wanted } from './corrections.ts'
import { remove, rename, toggleMark } from './edit.ts'
import { type Graph, emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { messages } from './prompt.ts'

const board = (text: string, g: Graph = emptyGraph()) => applyOps(g, parseOps(text))

/** 板上已经有「订单服务 → 库存服务」，这一句说的是库存服务再调支付服务；模型多连了一条订单服务 → 支付服务 */
function wrongEdge(): Round {
  const seen = board('E 「订单服务」 -> 「库存服务」')
  const ops = parseOps('E n2 -> 「支付服务」\nE n1 -> 「支付服务」')
  return { at: 1, seen, context: [{ speaker: 'A', text: '下单先走订单服务，再到库存服务。' }], said: { speaker: 'B', text: '库存扣完再调支付服务。' }, ops, drawn: applyOps(seen, ops) }
}

test('deleting a line the model just drew is recorded, with what it should have drawn', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  assert.equal(t.edit(remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n3' }), '删掉了「订单服务 → 支付服务」'), true)
  t.close()
  assert.equal(t.list.length, 1)
  const c = t.list[0]
  assert.equal(c.ops, 'E n2 -> 「支付服务」\nE n1 -> 「支付服务」')
  assert.equal(wanted(c), 'E n2 -> 「支付服务」')
  assert.deepEqual(c.edits, ['删掉了「订单服务 → 支付服务」'])
  assert.ok(exportable(c))

  const k = toCase(c, 'fix-1')
  assert.deepEqual(k.turns, [r.said])
  assert.deepEqual(k.history, r.context)
  assert.deepEqual(k.gold.nodes, [['订单服务'], ['库存服务'], ['支付服务']])
  assert.deepEqual(k.gold.edges, [['库存服务', '支付服务']])
  assert.deepEqual(k.gold.optional_edges, [['订单服务', '库存服务']])
  assert.deepEqual(k.gold.forbidden_edges, [['订单服务', '支付服务']])
  // 拿这个用例给两版板书打分：模型原来那版违规，用户改完那版全对
  assert.deepEqual(score(r.drawn, k.gold).violations, ['不该出现的连线：订单服务 → 支付服务'])
  const ok = score(fixedOf(c), k.gold)
  assert.deepEqual(ok.violations, [])
  assert.deepEqual([ok.nodeP, ok.nodeR, ok.edgeP, ok.edgeR], [1, 1, 1, 1])
  // 模型原来那版多了一条线
  assert.equal(score(r.drawn, k.gold).edgeP, 0.5)
})

test('a preference pair is the exact prompt the model saw, the fix as chosen and its output as rejected', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  t.edit(remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n3' }), '删掉了')
  const p = toPair(t.list[0], 'fix-1')
  assert.deepEqual(p.prompt, messages(r.seen, r.context, r.said))
  assert.deepEqual(p.chosen, [{ role: 'assistant', content: 'E n2 -> 「支付服务」' }])
  assert.deepEqual(p.rejected, [{ role: 'assistant', content: 'E n2 -> 「支付服务」\nE n1 -> 「支付服务」' }])
})

test('a name the user fixed is the one to draw, and the wrong one is not kept as an alias', () => {
  const t = new Corrections()
  const seen = board('E 「订单服务」 -> 「库存服务」')
  const ops = parseOps('E n2 -> 「MQ」')
  const drawn = applyOps(seen, ops)
  t.round({ at: 1, seen, context: [], said: { text: '库存锁住以后发一条消息到 Kafka。' }, ops, drawn })
  assert.equal(t.edit(rename(drawn, { kind: 'node', id: 'n3' }, 'Kafka'), '「MQ」改成了「Kafka」'), true)
  const c = t.list[0]
  assert.equal(wanted(c), 'E n2 -> 「Kafka」')
  const k = toCase(c, 'fix-2')
  assert.deepEqual(k.gold.nodes, [['订单服务'], ['库存服务'], ['Kafka']])
  assert.deepEqual(k.gold.forbidden_nodes, ['MQ'])
  assert.deepEqual(score(drawn, k.gold).violations, ['不该出现：MQ'])
  assert.deepEqual(score(fixedOf(c), k.gold).violations, [])
})

test('undoing the whole round says the line should not have changed the board', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  assert.equal(t.edit(r.seen, '撤销'), true)
  assert.equal(wanted(t.list[0]), '-')
  const k = toCase(t.list[0], 'fix-3')
  assert.deepEqual(k.gold.forbidden_nodes, ['支付服务'])
})

test('striking out and un-striking: marks the user added must be drawn, ones they removed must not', () => {
  const t = new Corrections()
  const seen = board('T 「方案」\nL n1 「双写」\nL n1 「消息队列」')
  const ops = parseOps('M n2 划掉')
  const drawn = applyOps(seen, ops)
  t.round({ at: 1, seen, context: [], said: { text: '消息队列这个方案不行。' }, ops, drawn })
  const fixed = toggleMark(toggleMark(drawn, 'n2', '划掉'), 'n3', '划掉')
  assert.equal(t.edit(toggleMark(drawn, 'n2', '划掉'), '去掉了「双写」的划掉'), true)
  assert.equal(t.edit(fixed, '划掉了「消息队列」'), true)
  const c = t.list[0]
  assert.equal(wanted(c), 'M n3 划掉')
  assert.deepEqual(c.edits, ['去掉了「双写」的划掉', '划掉了「消息队列」'])
  const k = toCase(c, 'fix-4')
  assert.deepEqual(k.gold.struck, [['消息队列']])
  assert.deepEqual(k.gold.not_struck, [['双写']])
  assert.equal(score(drawn, k.gold).markAcc, 0)
  assert.equal(score(fixedOf(c), k.gold).markAcc, 1)
})

test('changing it and changing it back is not a correction', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  const cut = remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n3' })
  assert.equal(t.edit(cut, '删掉了'), true)
  assert.equal(t.edit(r.drawn, '撤销'), false)
  t.close()
  assert.equal(t.list.length, 0)
})

test('fixing something older than this round is not put on it', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  // 改的是原来就在板上的「订单服务 → 库存服务」
  assert.equal(t.edit(remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n2' }), '删掉了'), false)
  // 之后再删这一轮的线也不收了：板上已经混着一处和这句话无关的修改
  const g = remove(remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n2' }), { kind: 'edge', from: 'n1', to: 'n3' })
  assert.equal(t.edit(g, '删掉了'), false)
  assert.equal(t.list.length, 0)
})

test('a board edited while the model was still drawing is not used', () => {
  const t = new Corrections()
  const first = wrongEdge()
  t.round(first)
  t.close()
  // 模型在画下一句的时候，用户先手动改了板：模型看到的是改之前的
  const edited = rename(first.drawn, { kind: 'node', id: 'n1' }, '订单')
  t.edit(edited, '改名')
  const ops = parseOps('E n3 -> 「通知服务」')
  t.round({ at: 2, seen: first.drawn, context: [], said: { text: '付完款发通知。' }, ops, drawn: applyOps(edited, ops) })
  assert.equal(t.edit(remove(applyOps(edited, ops), { kind: 'node', id: 'n4' }), '删掉了'), false)
  assert.equal(t.list.length, 0)
})

test('a line that drew nothing in between keeps the earlier round open', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  t.round({ at: 2, seen: r.drawn, context: [], said: { text: '嗯，对。' }, ops: [], drawn: r.drawn })
  assert.equal(t.edit(remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n3' }), '删掉了'), true)
  assert.equal(t.list[0].said.text, '库存扣完再调支付服务。')
})

test('the next round that draws something ends the sample; samples load back with the meeting', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  const cut = remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n3' })
  t.edit(cut, '删掉了')
  const ops = parseOps('E n3 -> 「通知服务」')
  t.round({ at: 2, seen: cut, context: [], said: { text: '付完款发通知。' }, ops, drawn: applyOps(cut, ops) })
  // 这次改的是新的一轮画的，记成新的一条
  assert.equal(t.edit(cut, '撤销'), true)
  assert.equal(t.list.length, 2)
  assert.equal(wanted(t.list[1]), '-')
  const saved = JSON.parse(JSON.stringify(t.list))
  const u = new Corrections()
  u.load(saved)
  assert.equal(wanted(u.list[0]), 'E n2 -> 「支付服务」')
})

test('ops between two boards rebuild the second from the first', () => {
  const a = board('E 「网关」 -> 「登录接口」\nT 「分工」\nL n3 「小王：网关」')
  const b = board(
    [
      'R n2 「登录服务」',
      'E n2 -> 「用户库」 : 「查用户」',
      'G 「后端」 n1 n2',
      'L n3 「小李：登录」',
      'M n6 星标',
      'Q n5 「要不要缓存」',
      'I n5 数据库',
      'X n1 -> n2',
      'T 「风险」',
      'L n8 「缓存击穿」',
      'K n1 「限流」',
      'D n4',
    ].join('\n'),
    a,
  )
  const ops = opsBetween(a, b)
  const shape = (g: Graph) => {
    const name = (id: string) => g.nodes.find((n) => n.id === id)?.label
    return {
      nodes: g.nodes.map((n) => JSON.stringify([n.label, n.kind, name(n.parent ?? ''), g.groups.find((x) => x.id === n.group)?.label, n.marks, n.icon])).sort(),
      edges: g.edges.map((e) => JSON.stringify([name(e.from), name(e.to), e.label])).sort(),
    }
  }
  assert.deepEqual(shape(applyOps(a, ops)), shape(b))
})

test('follow-up fixes elsewhere count only while the user is still at it', () => {
  const t = new Corrections()
  const seen = board('T 「方案」\nL n1 「双写」\nL n1 「消息队列」\nT 「风险」')
  const ops = parseOps('M n2 划掉')
  const drawn = applyOps(seen, ops)
  t.round({ at: 1, seen, context: [], said: { text: '消息队列这个方案不行。' }, ops, drawn })
  const unstruck = toggleMark(drawn, 'n2', '划掉')
  assert.equal(t.edit(unstruck, '去掉了划掉', 1000), true)
  // 过了好几分钟才去改「风险」：和这一句无关
  assert.equal(t.edit(rename(unstruck, { kind: 'node', id: 'n4' }, '风险点'), '改名', 1000 + 5 * 60_000), false)
  assert.deepEqual(t.list[0].edits, ['去掉了划掉'])
})

test('export files keep only real corrections and number them per meeting', () => {
  const t = new Corrections()
  const r = wrongEdge()
  t.round(r)
  t.edit(remove(r.drawn, { kind: 'edge', from: 'n1', to: 'n3' }), '删掉了')
  t.close()
  const all = [{ id: 'm1', corrections: t.list }, { id: 'm2' }]
  const cases = JSON.parse(casesFile(all))
  assert.equal(cases.length, 1)
  assert.equal(cases[0].id, 'm1-1')
  assert.deepEqual(cases[0].board, JSON.parse(JSON.stringify(r.seen)))
  const lines = pairsFile(all).trim().split('\n').map((l) => JSON.parse(l))
  assert.equal(lines.length, 1)
  assert.equal(lines[0].chosen[0].content, 'E n2 -> 「支付服务」')
})
