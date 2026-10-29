import assert from 'node:assert/strict'
import { test } from 'node:test'
import { labelSpot } from '../board/labels.ts'

const box = (x: number, y: number, w = 110, h = 64) => ({ x, y, w, h })

test('edge labels sit on the longest segment of the route, above a horizontal one', () => {
  // 绕到下面的走线：最长的是底下那段横线，标签应该写在它上方，而不是两端点的中点
  const route: [number, number][] = [
    [55, 64],
    [55, 200],
    [655, 200],
    [655, 64],
  ]
  const [x, y] = labelSpot(route, '查', 16, [])
  assert.ok(x > 300 && x < 400, `x=${x}`)
  assert.ok(y < 200 && y > 160, `y=${y}`)
})

test('edge labels move to the other side of the line instead of covering a node', () => {
  const route: [number, number][] = [
    [0, 100],
    [400, 100],
  ]
  // 线上方正好有个节点
  const [, y] = labelSpot(route, '转发', 16, [box(150, 40)])
  assert.ok(y > 100, `应该挪到线下方，y=${y}`)
})

test('labels on vertical segments go to the right of the line', () => {
  const route: [number, number][] = [
    [100, 0],
    [100, 300],
  ]
  const [x, y] = labelSpot(route, '调用', 16, [])
  assert.ok(x > 100, `x=${x}`)
  assert.ok(y > 120 && y < 160, `y=${y}`)
})
