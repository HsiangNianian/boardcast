import assert from 'node:assert/strict'
import { test } from 'node:test'
import { layout } from '../board/layout.ts'
import { circled, hitTest, insideLoop, selectionBox } from '../ink/hit.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

test('clicks pick flowchart nodes, items, topic titles and edges; empty space picks nothing', async () => {
  const g = applyOps(emptyGraph(), parseOps('E 「用户」 「网关」 : 「请求」\nT 「爬虫」\nL n3 「监控任务」'))
  const l = await layout(g)
  const center = (b: { x: number; y: number; w: number; h: number }) => [b.x + b.w / 2, b.y + b.h / 2] as const

  const user = l.diagram!.nodes.find((b) => b.id === 'n1')!
  assert.deepEqual(hitTest(l, g, ...center(user)), { kind: 'node', id: 'n1' })

  const item = l.anchors.get('n4')!
  assert.deepEqual(hitTest(l, g, ...center(item)), { kind: 'node', id: 'n4' })

  // 话题：点标题选中话题；点在话题区域里但不在任何一行上，什么都不选
  const title = selectionBox(l, { kind: 'node', id: 'n3' })!
  assert.deepEqual(hitTest(l, g, ...center(title)), { kind: 'node', id: 'n3' })
  const region = l.regions.find((r) => r.id === 'n3')!
  assert.equal(hitTest(l, g, region.x + region.w - 2, region.y + region.h - 2), null)

  // 连线：点在线的中间
  const route = l.diagram!.edges[0].points
  const [a, b] = [route[0], route[route.length - 1]]
  const mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2] as const
  assert.deepEqual(hitTest(l, g, mid[0], mid[1] + 3), { kind: 'edge', from: 'n1', to: 'n2' })

  assert.equal(hitTest(l, g, l.bounds.x1 + 200, l.bounds.y1 + 200), null)
})

test('a drawn loop picks what its middle encloses, left to right', async () => {
  const g = applyOps(emptyGraph(), parseOps('E 「用户」 「网关」 : 「请求」\nT 「爬虫」\nL n3 「监控任务」\nL n3 「告警」'))
  const l = await layout(g)
  type B = { x: number; y: number; w: number; h: number }
  // 画一圈：把几个框都包进去的一个椭圆
  const around = (...bs: B[]): [number, number][] => {
    const x0 = Math.min(...bs.map((b) => b.x)) - 20
    const y0 = Math.min(...bs.map((b) => b.y)) - 20
    const x1 = Math.max(...bs.map((b) => b.x + b.w)) + 20
    const y1 = Math.max(...bs.map((b) => b.y + b.h)) + 20
    const cx = (x0 + x1) / 2
    const cy = (y0 + y1) / 2
    return Array.from({ length: 40 }, (_, i) => {
      const a = (i / 40) * Math.PI * 2
      return [cx + ((x1 - x0) / 2) * 1.42 * Math.cos(a), cy + ((y1 - y0) / 2) * 1.42 * Math.sin(a)]
    })
  }
  const box = (id: string) => l.diagram?.nodes.find((b) => b.id === id) ?? selectionBox(l, { kind: 'node', id })!
  assert.deepEqual(circled(l, g, around(box('n1'))), ['n1'])
  assert.deepEqual(circled(l, g, around(box('n2'), box('n1'))), ['n1', 'n2'])
  // 话题连同条目：都列出来，太多时只留话题
  assert.deepEqual(circled(l, g, around(box('n3'), box('n4'), box('n5'))), ['n3', 'n4', 'n5'])
  assert.deepEqual(circled(l, g, around(box('n3'), box('n4'), box('n5')), 2), ['n3'])
  // 圈在空白处
  const far = l.bounds.x1 + 300
  assert.deepEqual(circled(l, g, around({ x: far, y: 0, w: 50, h: 50 })), [])
  // 一小道不成圈
  assert.deepEqual(circled(l, g, [[0, 0], [5, 5]]), [])
})

test('inside a loop, even one not closed by hand', () => {
  const loop: [number, number][] = [[0, 0], [100, 0], [100, 100], [0, 100]]
  assert.equal(insideLoop(loop, 50, 50), true)
  assert.equal(insideLoop(loop, 150, 50), false)
  assert.equal(insideLoop(loop, -1, 50), false)
})
