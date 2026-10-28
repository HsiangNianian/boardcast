// 小图标：用几笔矩形、椭圆、线条拼出来，交给 Excalidraw 的手绘笔触，和板上其他东西是同一种画风。

import type { ExcalidrawElementSkeleton } from '@excalidraw/excalidraw/data/transform'
import type { Icon } from '../engine/graph.ts'

type Sk = ExcalidrawElementSkeleton

export function icon(kind: Icon, x: number, y: number, s: number, color: string): Sk[] {
  const base = { strokeColor: color, roughness: 1.5, strokeWidth: 2, backgroundColor: 'transparent' } as const
  const line = (pts: [number, number][]): Sk => ({
    type: 'line',
    x: x + pts[0][0] * s,
    y: y + pts[0][1] * s,
    points: pts.map(([a, b]) => [(a - pts[0][0]) * s, (b - pts[0][1]) * s]) as never,
    ...base,
  })
  const rect = (a: number, b: number, w: number, h: number, round = false): Sk => ({
    type: 'rectangle',
    x: x + a * s,
    y: y + b * s,
    width: w * s,
    height: h * s,
    roundness: round ? { type: 3 } : null,
    ...base,
  })
  const oval = (a: number, b: number, w: number, h: number): Sk => ({
    type: 'ellipse',
    x: x + a * s,
    y: y + b * s,
    width: w * s,
    height: h * s,
    ...base,
  })
  switch (kind) {
    case '数据库':
      return [oval(0.1, 0, 0.8, 0.28), line([[0.1, 0.14], [0.1, 0.86]]), line([[0.9, 0.14], [0.9, 0.86]]), oval(0.1, 0.72, 0.8, 0.28)]
    case '表格':
      return [rect(0, 0.1, 1, 0.8), line([[0, 0.37], [1, 0.37]]), line([[0, 0.63], [1, 0.63]]), line([[0.4, 0.1], [0.4, 0.9]])]
    case '手机':
      return [rect(0.22, 0, 0.56, 1, true), line([[0.42, 0.88], [0.58, 0.88]])]
    case '人':
      return [oval(0.3, 0, 0.4, 0.4), line([[0.08, 1], [0.2, 0.62], [0.5, 0.5], [0.8, 0.62], [0.92, 1]])]
    case '服务器':
      return [rect(0.05, 0.02, 0.9, 0.28), rect(0.05, 0.36, 0.9, 0.28), rect(0.05, 0.7, 0.9, 0.28)]
    case '文档':
      return [
        line([[0.15, 0], [0.65, 0], [0.85, 0.2], [0.85, 1], [0.15, 1], [0.15, 0]]),
        line([[0.3, 0.45], [0.7, 0.45]]),
        line([[0.3, 0.65], [0.7, 0.65]]),
      ]
    case '云':
      return [oval(0, 0.35, 0.5, 0.45), oval(0.25, 0.1, 0.5, 0.55), oval(0.5, 0.35, 0.5, 0.45)]
    case '图表':
      return [line([[0.05, 0], [0.05, 0.95], [1, 0.95]]), rect(0.2, 0.55, 0.15, 0.4), rect(0.45, 0.3, 0.15, 0.65), rect(0.7, 0.45, 0.15, 0.5)]
    case '消息':
      return [line([[0, 0.1], [1, 0.1], [1, 0.7], [0.4, 0.7], [0.2, 0.95], [0.25, 0.7], [0, 0.7], [0, 0.1]])]
  }
}
