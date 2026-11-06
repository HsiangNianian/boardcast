import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type Box, StableLayout } from '../board/layout.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { hitTest, selectionLine } from '../ink/hit.ts'

const overlap = (a: Box, b: Box) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h

test('explicit topic-to-item links keep their arrow and label outside text with complete bounds', async () => {
  for (const [title, item, label] of [['Meeting goals and decisions', 'Collect actionable follow-up items', 'Next steps'], ['会议目标与决定', '整理可执行的后续事项', '下一步']]) {
    const initial = applyOps(emptyGraph(), parseOps(`T 「${title}」\nL 「${title}」 「${item}」`))
    const graph = { ...initial, edges: [{ from: 'n1', to: 'n2', label, ink: 'ink' as const }] }
    const before = JSON.stringify(graph)
    const board = new StableLayout()
    const layout = await board.layout(graph)
    const route = layout.links[0]
    assert.equal(layout.diagram, undefined)
    assert.ok(route.points && route.labelBox)
    assert.equal(route.points.length, 4)
    const boxes = [layout.anchors.get('n1')!, layout.anchors.get('n2')!]
    for (const box of boxes) {
      assert.ok(!overlap(box, route.labelBox), 'edge label covers a node label')
      for (let n = 1; n < route.points.length; n++) {
        const a: [number, number] = route.points[n - 1], b: [number, number] = route.points[n]
        for (let step = 0; step <= 20; step++) {
          const x: number = a[0] + (b[0] - a[0]) * step / 20, y: number = a[1] + (b[1] - a[1]) * step / 20
          assert.ok(x < box.x || x > box.x + box.w || y < box.y || y > box.y + box.h, 'arrow crosses text')
        }
      }
    }
    assert.ok(route.labelBox.x >= layout.bounds.x0 && route.labelBox.x + route.labelBox.w <= layout.bounds.x1)
    assert.ok(route.labelBox.y >= layout.bounds.y0 && route.labelBox.y + route.labelBox.h <= layout.bounds.y1)
    const at = [route.points[1][0], (route.points[1][1] + route.points[2][1]) / 2]
    const selection = { kind: 'edge' as const, from: 'n1', to: 'n2' }
    assert.deepEqual(hitTest(layout, graph, at[0], at[1]), selection)
    assert.deepEqual(selectionLine(layout, selection), route.points)
    const reopened = new StableLayout(); reopened.restore(board.snapshot())
    assert.deepEqual((await reopened.layout(structuredClone(graph))).links, layout.links)
    assert.equal(JSON.stringify(graph), before, 'routing must not change edge semantics')
  }
})

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

test('an in-flight layout cannot overwrite a newly restored meeting', async () => {
  const board = new StableLayout()
  const old = applyOps(emptyGraph(), parseOps('E 「Old A」 「Old B」'))
  const pending = board.layout(old)
  // Let the first ELK computation start, then restore a different meeting while it waits.
  await Promise.resolve()
  const restored = {
    regionAt: [['new topic', { x: 700, y: 900 }]],
    nodeAt: [], routeAt: [], boxAt: [],
  } satisfies import('../board/layout.ts').LayoutMemory
  board.restore(restored)
  await pending
  assert.deepEqual(board.snapshot(), restored)
  const fresh = applyOps(emptyGraph(), parseOps('T 「New meeting」'))
  const result = await board.layout(fresh)
  assert.equal(result.anchors.size, 1)
  assert.equal(board.snapshot().nodeAt.length, 0)
})
