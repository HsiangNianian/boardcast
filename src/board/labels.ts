// 连线标签的位置：手写和铅笔两种画法都要避免标签飘离连线、压在节点上。

import { textWidth } from './layout.ts'

type Pt = [number, number]
interface Rect {
  x: number
  y: number
  w: number
  h: number
}

/**
 * 流程图连线的标签放哪：贴着折线最长的那一段写（横的写在线上方，竖的写在线右边），
 * 压到节点就换到线的另一边。返回标签左上角。
 * 以前放在两端点的中点，折线绕路时标签会飘到离线很远、甚至压在框上。
 */
export function labelSpot(points: Pt[], label: string, size: number, nodes: Rect[]): Pt {
  const w = textWidth(label, size)
  let best = 1
  let len = -1
  for (let i = 1; i < points.length; i++) {
    const l = Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1])
    if (l > len) {
      len = l
      best = i
    }
  }
  const [a, b] = [points[best - 1], points[best]]
  const mx = (a[0] + b[0]) / 2
  const my = (a[1] + b[1]) / 2
  const flat = Math.abs(b[1] - a[1]) <= Math.abs(b[0] - a[0])
  const spots: Pt[] = flat
    ? [
        [mx - w / 2, my - size - 8],
        [mx - w / 2, my + 8],
      ]
    : [
        [mx + 8, my - size / 2],
        [mx - w - 8, my - size / 2],
      ]
  const free = ([x, y]: Pt) => !nodes.some((n) => x < n.x + n.w && n.x < x + w && y < n.y + n.h && n.y < y + size)
  return spots.find(free) ?? spots[0]
}
