import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type Box, StableLayout } from '../board/layout.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

const overlap = (a: Box, b: Box) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h

test('incremental layout: old nodes and regions stay put as the board grows', async () => {
  const board = new StableLayout()
  const steps = [
    'E 「用户」 「小程序」',
    'E 「小程序」 「登录接口」',
    'T 「爬虫」\nL 「爬虫」 「监控任务」',
    'E 「登录接口」 「用户库」\nL 「爬虫」 「视频搜索」',
    'Q 「爬虫」 「要不要上 MQ」\nE 「网关」 「登录接口」',
    'T 「情感打分」\nL 「情感打分」 「Go 或 Rust」',
    // 同一行最左连到最右：直着连会穿过中间的节点
    'E 「用户」 「用户库」',
  ]
  let g = emptyGraph()
  let prev = new Map<string, Box>()
  for (const step of steps) {
    g = applyOps(g, parseOps(step))
    const l = await board.layout(g)
    // 以前出现过的东西一个都没动
    for (const [id, b] of prev) {
      const now = l.anchors.get(id)
      assert.ok(now, `${id} 不见了`)
      assert.deepEqual([now.x, now.y], [b.x, b.y], `${id} 挪了位置（第 ${steps.indexOf(step) + 1} 步）`)
    }
    // 流程图节点之间不重叠
    const nodes = l.diagram?.nodes ?? []
    for (let i = 0; i < nodes.length; i++)
      for (let j = i + 1; j < nodes.length; j++) assert.ok(!overlap(nodes[i], nodes[j]), `${nodes[i].id} 和 ${nodes[j].id} 重叠`)
    // 流程图连线不穿过别的节点
    for (const e of l.diagram?.edges ?? []) {
      for (let i = 1; i < e.points.length; i++) {
        const [[x1, y1], [x2, y2]] = [e.points[i - 1], e.points[i]]
        const seg = { id: '', x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) }
        for (const n of nodes) {
          if (n.id === e.from || n.id === e.to) continue
          const inner = { ...n, x: n.x + 4, y: n.y + 4, w: n.w - 8, h: n.h - 8 }
          const hit = seg.x < inner.x + inner.w && inner.x < seg.x + seg.w && seg.y < inner.y + inner.h && inner.y < seg.y + seg.h
          assert.ok(!hit, `${e.from}→${e.to} 穿过了 ${n.id}（第 ${steps.indexOf(step) + 1} 步）`)
        }
      }
    }
    // 区域之间不重叠
    const areas = [...l.regions, ...(l.diagram ? [l.diagram.box] : [])]
    for (let i = 0; i < areas.length; i++)
      for (let j = i + 1; j < areas.length; j++) assert.ok(!overlap(areas[i], areas[j]), `${areas[i].id} 和 ${areas[j].id} 重叠`)
    prev = new Map([...l.anchors].filter(([, b]) => b.id.startsWith('n')))
  }
})

test('the same graph object is laid out once; reset starts over', async () => {
  const board = new StableLayout()
  const g = applyOps(emptyGraph(), parseOps('E 「甲」 「乙」'))
  assert.equal(board.layout(g), board.layout(g))
  const a = await board.layout(g)
  board.reset()
  const b = await board.layout(g)
  assert.notEqual(a, b)
  assert.deepEqual(a.bounds, b.bounds)
})
