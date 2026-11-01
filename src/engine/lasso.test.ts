import assert from 'node:assert/strict'
import { test } from 'node:test'
import { insideLoop } from '../ink/hit.ts'
import { dashes, merge, overlaps } from '../ink/lasso.ts'

type Pt = [number, number]
const ring = (cx: number, cy: number, r: number, n = 32): Pt[] =>
  Array.from({ length: n }, (_, i) => [cx + r * Math.cos((i / n) * Math.PI * 2), cy + r * Math.sin((i / n) * Math.PI * 2)])

test('loops that touch, cross or nest overlap; loops apart do not', () => {
  assert.equal(overlaps(ring(0, 0, 50), ring(80, 0, 50)), true)
  assert.equal(overlaps(ring(0, 0, 50), ring(0, 0, 20)), true)
  assert.equal(overlaps(ring(0, 0, 50), ring(200, 0, 50)), false)
  // 两个细长的圈十字交叉：谁的点都不在对方里面，但边交叉了
  const wide: Pt[] = [[-100, -5], [100, -5], [100, 5], [-100, 5]]
  const tall: Pt[] = [[-5, -100], [5, -100], [5, 100], [-5, 100]]
  assert.equal(overlaps(wide, tall), true)
})

test('merged loops become one round loop around both', () => {
  const a = ring(0, 0, 50)
  const b = ring(90, 10, 40)
  const m = merge([a, b])
  // 两圈里的东西都还在大圈里
  for (const [x, y] of [[0, 0], [-40, 0], [90, 10], [125, 10], [0, 45], [90, 45]] as Pt[]) assert.ok(insideLoop(m, x, y), `${x},${y}`)
  assert.equal(insideLoop(m, 300, 0), false)
  // 磨圆过：点比凸包多得多
  assert.ok(m.length > 40)
})

test('a loop is cut into pen dashes along its length', () => {
  const sq: Pt[] = [[0, 0], [100, 0], [100, 100], [0, 100]]
  const closed = dashes(sq, 16, 10, true)
  // 周长 400，一段 26
  assert.ok(closed.length >= 15 && closed.length <= 16, String(closed.length))
  const len = (s: Pt[]) => s.slice(1).reduce((t, p, i) => t + Math.hypot(p[0] - s[i][0], p[1] - s[i][1]), 0)
  for (const d of closed.slice(0, -1)) assert.ok(Math.abs(len(d) - 16) < 1e-6, String(len(d)))
  // 没闭合的（正在画的）不连回起点
  assert.ok(dashes(sq, 16, 10, false).length < closed.length)
  assert.deepEqual(dashes([[0, 0]], 16, 10, false), [])
})
