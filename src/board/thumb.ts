// 板的缩略图：不真的渲染一遍，只从排版结果里取出框、文字行和连线的位置，画成一张示意的小图。
// 坐标先缩到 THUMB 宽的盒子里、取整，跟着会议存下来，切到别的板时也能显示。

import type { Layout } from './layout.ts'

/** 缩略图坐标系的宽度；高度按比例 */
export const THUMB = 160

export interface Thumb {
  w: number
  h: number
  /** 流程图的框 */
  boxes: [number, number, number, number][]
  /** 文字行（话题标题加粗）：起点、宽度，和是不是标题 */
  rows: [number, number, number, 0 | 1][]
  /** 连线 */
  lines: [number, number][][]
}

const MAX = 160

export function thumbOf(l: Layout): Thumb | undefined {
  const { x0, y0, x1, y1 } = l.bounds
  const bw = x1 - x0
  const bh = y1 - y0
  if (!(bw > 0 && bh > 0)) return undefined
  // 太扁或太高的板，按宽高里大的那边缩
  const k = THUMB / Math.max(bw, bh * 1.6)
  const X = (x: number) => Math.round((x - x0) * k)
  const Y = (y: number) => Math.round((y - y0) * k)
  const L = (n: number) => Math.max(1, Math.round(n * k))
  const boxes: Thumb['boxes'] = []
  const rows: Thumb['rows'] = []
  const lines: Thumb['lines'] = []
  // 导入页：画一个页面的框
  if (l.page) boxes.push([X(l.page.x), Y(l.page.y), L(l.page.w), L(l.page.h)])
  const d = l.diagram
  if (d) {
    for (const n of d.nodes.slice(0, MAX)) boxes.push([X(n.x), Y(n.y), L(n.w), L(n.h)])
    for (const e of d.edges.slice(0, MAX)) lines.push(e.points.map(([x, y]) => [X(x), Y(y)]))
  }
  for (const r of l.regions) {
    if (r.title) rows.push([X(r.x + r.title.x), Y(r.y), L(r.title.w), 1])
    for (const row of r.rows.slice(0, 40)) rows.push([X(r.x + row.x), Y(r.y + row.y), L(row.w), 0])
  }
  return { w: X(x1), h: Y(y1), boxes, rows: rows.slice(0, MAX * 2), lines }
}
