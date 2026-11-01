// 连线标签的位置：别飘离自己的线，也别压在节点、文字或者别的线上。

import { textWidth } from './layout.ts'

export type Pt = [number, number]
export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export interface Spot {
  /** 标签左上角 */
  x: number
  y: number
  w: number
  h: number
  /** 找到了一个不压框、不压字、不压线的位置；为 false 时调用方应该给标签垫一块底色 */
  clear: boolean
}

const PAD = 3

/** 线段有没有穿过（或贴着）这个矩形：沿线每隔几个单位取一个点看 */
function hitsSegment(r: Rect, a: Pt, b: Pt): boolean {
  const len = Math.hypot(b[0] - a[0], b[1] - a[1])
  const n = Math.max(1, Math.ceil(len / 4))
  for (let i = 0; i <= n; i++) {
    const x = a[0] + ((b[0] - a[0]) * i) / n
    const y = a[1] + ((b[1] - a[1]) * i) / n
    if (x > r.x - PAD && x < r.x + r.w + PAD && y > r.y - PAD && y < r.y + r.h + PAD) return true
  }
  return false
}

const hitsLine = (r: Rect, line: Pt[]) => line.some((p, i) => i > 0 && hitsSegment(r, line[i - 1], p))
const hitsRect = (r: Rect, o: Rect) => r.x < o.x + o.w && o.x < r.x + r.w && r.y < o.y + o.h && o.y < r.y + r.h

/**
 * 标签放哪：沿着自己的折线，从最长的一段开始，每段试中点和两侧的位置，横段写在线的上方或下方，竖段写在线的左右；
 * 挑第一个不压节点 / 文字（`blocks`）、不压任何一条线（`lines`，包括自己这条）的位置。
 * `prefer` 是调用方想优先试的位置（比如弧线的外侧）。都不行就退回第一个候选，并标记 clear = false。
 */
export function labelSpot(points: Pt[], label: string, size: number, blocks: Rect[], lines: Pt[][] = [], prefer: Pt[] = []): Spot {
  const w = textWidth(label, size)
  const h = size
  const cands: Pt[] = [...prefer]
  const segs = points
    .map((p, i) => (i ? ([points[i - 1], p] as [Pt, Pt]) : null))
    .filter((s): s is [Pt, Pt] => !!s)
    .sort((s, t) => Math.hypot(t[1][0] - t[0][0], t[1][1] - t[0][1]) - Math.hypot(s[1][0] - s[0][0], s[1][1] - s[0][1]))
  for (const [a, b] of segs) {
    const flat = Math.abs(b[1] - a[1]) <= Math.abs(b[0] - a[0])
    for (const t of [0.5, 0.3, 0.7]) {
      const mx = a[0] + (b[0] - a[0]) * t
      const my = a[1] + (b[1] - a[1]) * t
      if (flat) cands.push([mx - w / 2, my - h - 8], [mx - w / 2, my + 8])
      else cands.push([mx + 8, my - h / 2], [mx - w - 8, my - h / 2])
    }
  }
  const free = ([x, y]: Pt) => {
    const r = { x, y, w, h }
    return !blocks.some((b) => hitsRect(r, b)) && !lines.some((l) => hitsLine(r, l))
  }
  const hit = cands.find(free)
  const at = hit ?? cands[0] ?? [points[0][0], points[0][1]]
  return { x: at[0], y: at[1], w, h, clear: !!hit }
}
