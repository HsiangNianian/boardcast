import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type Pt, labelSpot } from '../board/labels.ts'

const box = (x: number, y: number, w = 110, h = 64) => ({ x, y, w, h })

test('edge labels sit on the longest segment of the route, above a horizontal one', () => {
  // 绕到下面的走线：最长的是底下那段横线，标签应该写在它上方，而不是两端点的中点
  const route: Pt[] = [
    [55, 64],
    [55, 200],
    [655, 200],
    [655, 64],
  ]
  const { x, y, clear } = labelSpot(route, '查', 16, [], [route])
  assert.ok(x > 300 && x < 400, `x=${x}`)
  assert.ok(y < 200 && y > 160, `y=${y}`)
  assert.ok(clear)
})

test('edge labels move to the other side of the line instead of covering a node', () => {
  const route: Pt[] = [
    [0, 100],
    [400, 100],
  ]
  // 线上方正好有个节点
  const { y } = labelSpot(route, '转发', 16, [box(150, 40)], [route])
  assert.ok(y > 100, `应该挪到线下方，y=${y}`)
})

test('labels on vertical segments go to the right of the line', () => {
  const route: Pt[] = [
    [100, 0],
    [100, 300],
  ]
  const { x, y } = labelSpot(route, '调用', 16, [], [route])
  assert.ok(x > 100, `x=${x}`)
  assert.ok(y > 120 && y < 160, `y=${y}`)
})

test("labels avoid other edges' lines, not just nodes", () => {
  // 自己这条是横线；另一条线正好从标签默认的位置（线的正上方中间）横穿过去
  const own: Pt[] = [
    [0, 200],
    [400, 200],
  ]
  const other: Pt[] = [
    [-50, 180],
    [450, 180],
  ]
  const s = labelSpot(own, '锁库存', 16, [], [own, other])
  assert.ok(s.clear)
  const hitsOther = s.y < 180 + 3 && s.y + s.h > 180 - 3
  assert.ok(!hitsOther, `标签 y=${s.y}..${s.y + s.h} 压到了 y=180 的那条线`)
})

test('when every spot is taken the label is flagged so it gets a backing', () => {
  const own: Pt[] = [
    [0, 100],
    [200, 100],
  ]
  // 一堵墙一样的节点把上下两侧都占满
  const s = labelSpot(own, '锁库存', 16, [box(-100, 0, 400, 95), box(-100, 105, 400, 95)], [own])
  assert.equal(s.clear, false)
})
