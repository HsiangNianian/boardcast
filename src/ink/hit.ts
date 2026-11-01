// 手写板上点了哪儿：按排版结果判断点中的是节点、条目、话题标题还是连线。

import { type Box, type Layout, TITLE } from '../board/layout.ts'
import type { Selection } from '../engine/edit.ts'
import type { Graph } from '../engine/graph.ts'

type Pt = [number, number]

const inside = (b: { x: number; y: number; w: number; h: number }, x: number, y: number, pad = 0) =>
  x >= b.x - pad && x <= b.x + b.w + pad && y >= b.y - pad && y <= b.y + b.h + pad

/** 点到折线的距离 */
function distance(points: Pt[], x: number, y: number): number {
  let best = Infinity
  for (let i = 1; i < points.length; i++) {
    const [ax, ay] = points[i - 1]
    const [bx, by] = points[i]
    const dx = bx - ax
    const dy = by - ay
    const t = dx || dy ? Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy))) : 0
    best = Math.min(best, Math.hypot(x - (ax + t * dx), y - (ay + t * dy)))
  }
  return best
}

/** 话题标题占的地方（整块话题区域太大，不能拿来点选话题本身） */
function titleBox(l: Layout, id: string): Box | undefined {
  const r = l.regions.find((x) => x.head?.id === id)
  if (!r?.title) return undefined
  return { id, x: r.x + r.title.x, y: r.y, w: r.title.w, h: TITLE + 12 }
}

/** 跨区域连线没有存走线，按两端框的中心连一条直线近似 */
function linkLine(l: Layout, from: string, to: string): Pt[] | undefined {
  const a = l.anchors.get(from)
  const b = l.anchors.get(to)
  if (!a || !b) return undefined
  return [
    [a.x + a.w / 2, a.y + a.h / 2],
    [b.x + b.w / 2, b.y + b.h / 2],
  ]
}

/**
 * 点中了什么。`tol` 是连线的容差（画板坐标），一般取屏幕上 8 像素换算过来的值。
 * 先看框和文字，再看线：线细，容易被框里的点误判。
 */
export function hitTest(l: Layout, g: Graph, x: number, y: number, tol = 8): Selection | null {
  for (const b of l.diagram?.nodes ?? []) if (inside(b, x, y)) return { kind: 'node', id: b.id }
  const regionIds = new Set(l.regions.map((r) => r.id))
  for (const [id, b] of l.anchors) {
    if (regionIds.has(id)) continue
    if (inside(b, x, y, 2) && g.nodes.some((n) => n.id === id)) return { kind: 'node', id }
  }
  for (const r of l.regions) {
    const t = r.head && titleBox(l, r.head.id)
    if (t && inside(t, x, y, 2)) return { kind: 'node', id: t.id }
  }
  let best: { sel: Selection; d: number } | null = null
  const consider = (from: string, to: string, pts: Pt[] | undefined) => {
    if (!pts) return
    const d = distance(pts, x, y)
    if (d <= tol && (!best || d < best.d)) best = { sel: { kind: 'edge', from, to }, d }
  }
  for (const e of l.diagram?.edges ?? []) consider(e.from, e.to, e.points)
  for (const k of l.links) consider(k.from, k.to, linkLine(l, k.from, k.to))
  return (best as { sel: Selection } | null)?.sel ?? null
}

/** 选中的东西占的地方：画高亮框、放浮动按钮用 */
export function selectionBox(l: Layout, sel: Selection): Box | undefined {
  if (sel.kind === 'node') {
    const d = l.diagram?.nodes.find((b) => b.id === sel.id)
    if (d) return d
    return titleBox(l, sel.id) ?? (l.regions.some((r) => r.id === sel.id) ? undefined : l.anchors.get(sel.id))
  }
  const route = l.diagram?.edges.find((e) => e.from === sel.from && e.to === sel.to)?.points ?? linkLine(l, sel.from, sel.to)
  if (!route) return undefined
  const xs = route.map((p) => p[0])
  const ys = route.map((p) => p[1])
  const x = Math.min(...xs)
  const y = Math.min(...ys)
  return { id: `${sel.from}>${sel.to}`, x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y }
}

/** 选中连线时高亮它的走线 */
export function selectionLine(l: Layout, sel: Selection): Pt[] | undefined {
  if (sel.kind !== 'edge') return undefined
  return l.diagram?.edges.find((e) => e.from === sel.from && e.to === sel.to)?.points ?? linkLine(l, sel.from, sel.to)
}
