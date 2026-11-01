import assert from 'node:assert/strict'
import { test } from 'node:test'
import { firstSeen, renameBy, renameByInSteps, saidAt, whoSaid } from './attribute.ts'
import { History } from './edit.ts'
import { type Graph, emptyGraph } from './graph.ts'
import type { ChatResponse } from './llm.ts'
import { applyOps, parseOps } from './ops.ts'
import { Session } from './session.ts'
import { type Step, diffGraph, replayTo } from './steps.ts'

const node = (g: Graph, label: string) => g.nodes.find((n) => n.label === label)

test('whoSaid: one speaker is that speaker; a mixed batch goes by whose words match the name', () => {
  assert.equal(whoSaid([{ speaker: '小王', text: '加个缓存' }], ['Redis']), '小王')
  assert.equal(whoSaid([{ text: '加个缓存' }], ['缓存']), undefined)
  const batch = [
    { speaker: '小王', text: '订单服务先调库存服务' },
    { speaker: '小李', text: '中间加一个消息队列' },
  ]
  assert.equal(whoSaid(batch, ['消息队列']), '小李')
  assert.equal(whoSaid(batch, ['库存服务']), '小王')
  // 两个人都提到了：算先说的
  assert.equal(whoSaid([...batch, { speaker: '小李', text: '库存服务要拆' }], ['库存服务']), '小王')
  // 谁的话都对不上：不乱认
  assert.equal(whoSaid(batch, ['鉴权']), undefined)
})

test('the session records who raised each new node and edge, and keeps it through later edits', async () => {
  const replies = ['T 「库存」\nL 「库存」 「库存服务要拆」', 'N 「消息队列」\nN 「订单服务」\nE 「订单服务」 -> 「消息队列」 : 「下单」', 'R 「消息队列」 「Kafka」\nM 「库存服务要拆」 星标']
  let release: () => void = () => {}
  const gate = new Promise<void>((r) => (release = r))
  let calls = 0
  const s = new Session({
    baseUrl: '',
    send: async (): Promise<ChatResponse> => {
      const i = calls++
      // 第一轮卡住，让后两句攒成一批
      if (i === 0) await gate
      return { choices: [{ message: { content: replies[i] } }] }
    },
  })
  s.say({ speaker: '小王', text: '库存服务要拆出来' })
  s.say({ speaker: '小李', text: '中间加一个消息队列' })
  s.say({ speaker: '老张', text: '订单服务下单以后发消息' })
  release()
  await s.drain()
  s.say({ speaker: '老张', text: '消息队列就用 Kafka，库存那条标个星' })
  await s.drain()
  const g = s.graph
  assert.equal(node(g, '库存')?.by, '小王')
  assert.equal(node(g, '库存服务要拆')?.by, '小王')
  assert.equal(node(g, 'Kafka')?.by, '小李', '改了名、换人改的，提的人还是原来那个')
  assert.ok(node(g, '库存服务要拆')?.marks.includes('星标'))
  assert.equal(node(g, '订单服务')?.by, '老张')
  assert.equal(g.edges[0].by, '老张')
})

test('renaming a speaker follows through the board, the replay steps and the undo history', () => {
  let g = applyOps(emptyGraph(), parseOps('N 「网关」\nN 「鉴权」\nE 「网关」 -> 「鉴权」'))
  g = { ...g, nodes: g.nodes.map((n) => ({ ...n, by: n.label === '网关' ? '说话人 1' : '说话人 2' })), edges: g.edges.map((e) => ({ ...e, by: '说话人 1' })) }
  const steps: Step[] = [{ t: 0, line: 2, d: diffGraph(emptyGraph(), g)! }]
  assert.equal(renameBy(g, '没这个人', '小王'), g, '没有要改的原样返回')
  const r = renameBy(g, '说话人 1', '小王')
  assert.deepEqual(
    r.nodes.map((n) => n.by),
    ['小王', '说话人 2'],
  )
  assert.equal(r.edges[0].by, '小王')
  assert.equal(g.nodes[0].by, '说话人 1', '不改原图')
  assert.deepEqual(replayTo(renameByInSteps(steps, '说话人 1', '小王'), 1), r)
  const h = new History<Graph>()
  h.record(g)
  h.map((x) => renameBy(x, '说话人 1', '小王'))
  assert.equal(h.undo(r)?.nodes[0].by, '小王')
})

test('saidAt finds the line a board item came from', () => {
  const lines = [
    { speaker: '小王', text: '我们先把登录理一下', page: 0 },
    { speaker: '小王', text: '小程序直接调登录接口', page: 0 },
    { speaker: '小李', text: '登录接口要先过网关', page: 0 },
    { text: '挪了「网关」的位置', manual: true, page: 0 },
    { speaker: '小王', text: '登录接口返回 token', page: 0 },
    { speaker: '小王', text: '登录接口', page: 1 },
  ]
  assert.equal(saidAt(lines, '小王', ['登录接口'], 0), 4, '一样像的取最靠后的')
  assert.equal(saidAt(lines, '小王', ['登录接口'], 0, 3), 1, '只在上板之前找')
  assert.equal(saidAt(lines, '小李', ['网关'], 0), 2)
  assert.equal(saidAt(lines, '小王', ['登录接口'], 1), 5, '只找这张板上的')
  assert.equal(saidAt(lines, '老张', ['网关'], 0), undefined)
  const a = applyOps(emptyGraph(), parseOps('N 「网关」'))
  const b = applyOps(a, parseOps('N 「鉴权」\nE 「网关」 -> 「鉴权」'))
  const steps: Step[] = [
    { t: 0, line: 3, d: diffGraph(emptyGraph(), a)! },
    { t: 1, line: 5, d: diffGraph(a, b)! },
  ]
  assert.equal(firstSeen(steps, { kind: 'node', id: 'n1' }), 3)
  assert.equal(firstSeen(steps, { kind: 'edge', from: 'n1', to: 'n2' }), 5)
  assert.equal(firstSeen(steps, { kind: 'node', id: 'n9' }), undefined)
})
