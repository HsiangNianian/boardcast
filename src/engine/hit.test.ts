import assert from 'node:assert/strict'
import { test } from 'node:test'
import { layout } from '../board/layout.ts'
import { hitTest, selectionBox } from '../ink/hit.ts'
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
