// 圈着说（#48）画的圈：两圈叠在一起就合成一个大圈；画的时候按笔迹切成一段一段的虚线。

import { insideLoop } from './hit.ts'

type Pt = [number, number]

/** 两条线段相交 */
function cross(a: Pt, b: Pt, c: Pt, d: Pt): boolean {
  const o = (p: Pt, q: Pt, r: Pt) => Math.sign((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]))
  return o(a, b, c) !== o(a, b, d) && o(c, d, a) !== o(c, d, b)
}

/** 两圈有没有叠在一起：一圈的点落在另一圈里，或者两圈的边交叉 */
export function overlaps(a: Pt[], b: Pt[]): boolean {
  if (a.length < 3 || b.length < 3) return false
  if (a.some((p) => insideLoop(b, p[0], p[1])) || b.some((p) => insideLoop(a, p[0], p[1]))) return true
  for (let i = 0; i < a.length; i++)
    for (let j = 0; j < b.length; j++)
      if (cross(a[i], a[(i + 1) % a.length], b[j], b[(j + 1) % b.length])) return true
  return false
}

/** 凸包（单调链），逆时针 */
function hull(pts: Pt[]): Pt[] {
  const s = [...pts].sort((p, q) => p[0] - q[0] || p[1] - q[1])
  if (s.length < 3) return s
  const turn = (o: Pt, a: Pt, b: Pt) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
  const half = (xs: Pt[]) => {
    const out: Pt[] = []
    for (const p of xs) {
      while (out.length >= 2 && turn(out[out.length - 2], out[out.length - 1], p) <= 0) out.pop()
      out.push(p)
    }
    out.pop()
    return out
  }
  return [...half(s), ...half([...s].reverse())]
}

/** 把一圈的角磨圆（Chaikin 切角），看着像手画的一圈而不是多边形 */
function round(loop: Pt[], times = 3): Pt[] {
  let pts = loop
  for (let t = 0; t < times; t++) {
    const next: Pt[] = []
    for (let i = 0; i < pts.length; i++) {
      const [a, b] = [pts[i], pts[(i + 1) % pts.length]]
      next.push([a[0] * 0.75 + b[0] * 0.25, a[1] * 0.75 + b[1] * 0.25], [a[0] * 0.25 + b[0] * 0.75, a[1] * 0.25 + b[1] * 0.75])
    }
    pts = next
  }
  return pts
}

/** 几圈合成一个大圈：把它们都包进去，往外放一点，再磨圆 */
export function merge(loops: Pt[][]): Pt[] {
  const h = hull(loops.flat())
  if (h.length < 3) return loops.flat()
  const cx = h.reduce((s, p) => s + p[0], 0) / h.length
  const cy = h.reduce((s, p) => s + p[1], 0) / h.length
  return round(h.map(([x, y]) => [cx + (x - cx) * 1.06, cy + (y - cy) * 1.06]))
}

/**
 * 沿着一圈切成一段段虚线：每段 `dash` 长，隔 `gap`。`closed` 时首尾也连上。
 * 返回每一段经过的点，交给笔去画。
 */
export function dashes(loop: Pt[], dash: number, gap: number, closed: boolean): Pt[][] {
  const pts = closed && loop.length > 2 ? [...loop, loop[0]] : loop
  const out: Pt[][] = []
  let cur: Pt[] = []
  let on = true
  let left = dash
  for (let i = 1; i < pts.length; i++) {
    let [ax, ay] = pts[i - 1]
    const [bx, by] = pts[i]
    let seg = Math.hypot(bx - ax, by - ay)
    if (on && !cur.length) cur.push([ax, ay])
    while (seg > left) {
      const t = left / seg
      const p: Pt = [ax + (bx - ax) * t, ay + (by - ay) * t]
      if (on) {
        cur.push(p)
        out.push(cur)
        cur = []
      } else cur = [p]
      on = !on
      ;[ax, ay] = p
      seg -= left
      left = on ? dash : gap
    }
    left -= seg
    if (on) cur.push([bx, by])
  }
  if (on && cur.length > 1) out.push(cur)
  return out
}
