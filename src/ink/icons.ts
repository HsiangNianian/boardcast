// 手写模式的小图标：和铅笔模式（board/icons.ts）同一套造型，换成一笔一笔的轨迹，交给同一支笔写。
// 坐标以图标边长为 1，(x, y) 是左上角。

import type { Icon } from '../engine/graph.ts'
import type { Pt, Stroke } from './glyphs.ts'
import { line } from './pen.ts'

/** 随手画的椭圆：一圈，起点随机 */
function oval(cx: number, cy: number, rx: number, ry: number, r: () => number): Stroke {
  const start = r() * Math.PI * 2
  const pts: Stroke = []
  for (let i = 0; i <= 28; i++) {
    const t = start + (i / 28) * Math.PI * 2.04
    pts.push([cx + Math.cos(t) * rx, cy + Math.sin(t) * ry])
  }
  return pts
}

export function inkIcon(kind: Icon, x: number, y: number, s: number, r: () => number): Stroke[] {
  const P = (a: number, b: number): Pt => [x + a * s, y + b * s]
  const seg = (a: [number, number], b: [number, number]) => line(P(...a), P(...b), r, 0.03)
  const poly = (pts: [number, number][]): Stroke => pts.map(([a, b]) => P(a, b))
  const rect = (a: number, b: number, w: number, h: number): Stroke =>
    poly([
      [a, b],
      [a + w, b],
      [a + w, b + h],
      [a, b + h],
      [a, b + 0.02],
    ])
  const ell = (a: number, b: number, w: number, h: number) => oval(x + (a + w / 2) * s, y + (b + h / 2) * s, (w / 2) * s, (h / 2) * s, r)
  switch (kind) {
    case '数据库':
      return [ell(0.1, 0, 0.8, 0.28), seg([0.1, 0.14], [0.1, 0.86]), seg([0.9, 0.14], [0.9, 0.86]), poly(arc(0.1, 0.86, 0.9, 0.14))]
    case '表格':
      return [rect(0, 0.1, 1, 0.8), seg([0, 0.37], [1, 0.37]), seg([0, 0.63], [1, 0.63]), seg([0.4, 0.1], [0.4, 0.9])]
    case '手机':
      return [rect(0.22, 0, 0.56, 1), seg([0.42, 0.88], [0.58, 0.88])]
    case '人':
      return [
        ell(0.3, 0, 0.4, 0.4),
        poly([
          [0.08, 1],
          [0.2, 0.62],
          [0.5, 0.5],
          [0.8, 0.62],
          [0.92, 1],
        ]),
      ]
    case '服务器':
      return [rect(0.05, 0.02, 0.9, 0.28), rect(0.05, 0.36, 0.9, 0.28), rect(0.05, 0.7, 0.9, 0.28)]
    case '文档':
      return [
        poly([
          [0.15, 0],
          [0.65, 0],
          [0.85, 0.2],
          [0.85, 1],
          [0.15, 1],
          [0.15, 0],
        ]),
        seg([0.3, 0.45], [0.7, 0.45]),
        seg([0.3, 0.65], [0.7, 0.65]),
      ]
    case '云':
      return [ell(0, 0.35, 0.5, 0.45), ell(0.25, 0.1, 0.5, 0.55), ell(0.5, 0.35, 0.5, 0.45)]
    case '图表':
      return [
        poly([
          [0.05, 0],
          [0.05, 0.95],
          [1, 0.95],
        ]),
        rect(0.2, 0.55, 0.15, 0.4),
        rect(0.45, 0.3, 0.15, 0.65),
        rect(0.7, 0.45, 0.15, 0.5),
      ]
    case '消息':
      return [
        poly([
          [0, 0.1],
          [1, 0.1],
          [1, 0.7],
          [0.4, 0.7],
          [0.2, 0.95],
          [0.25, 0.7],
          [0, 0.7],
          [0, 0.1],
        ]),
      ]
  }
}

/** 数据库底下那道弧：椭圆的下半圈 */
function arc(x0: number, y: number, x1: number, h: number): [number, number][] {
  const out: [number, number][] = []
  for (let i = 0; i <= 12; i++) {
    const t = Math.PI * (i / 12)
    out.push([x0 + ((x1 - x0) / 2) * (1 - Math.cos(t)), y + Math.sin(t) * h])
  }
  return out
}
