// 坐标 → Excalidraw 元素。手绘感来自 Excalidraw 自己的笔触和 Excalifont（中文会自动落到小赖体）。
// 铅笔稿：灰色、虚线、半透明；墨线：深色实线。被提到的东西会短暂涂上高亮。

import { convertToExcalidrawElements } from '@excalidraw/excalidraw'
import type { ExcalidrawElementSkeleton } from '@excalidraw/excalidraw/data/transform'
import type { Graph, Node } from '../engine/graph.ts'
import { icon } from './icons.ts'
import { FONT, type Layout, type Row, TITLE, textWidth } from './layout.ts'

type Sk = ExcalidrawElementSkeleton

export interface Look {
  ink: string
  pencil: string
  highlight: string
  group: string
  question: string
  note: string
}

/** Excalifont：手写风格，CJK 自动回落到小赖体 */
const HAND = 5

/** 从框中心 c 朝 p 方向走，返回在框（外扩一点）边上的那个点。 */
function edgePoint(b: { x: number; y: number; w: number; h: number }, c: [number, number], p: [number, number]): [number, number] {
  const pad = 14
  const hw = b.w / 2 + pad
  const hh = b.h / 2 + pad
  const dx = p[0] - c[0]
  const dy = p[1] - c[1]
  if (!dx && !dy) return c
  const t = Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity, 1)
  return [c[0] + dx * t, c[1] + dy * t]
}

export function scene(g: Graph, l: Layout, hot: Set<string>, look: Look) {
  const sk: Sk[] = []
  const color = (n: Node) => (n.ink === 'draft' ? look.pencil : look.ink)
  const dim = (n: Node) => (n.ink === 'draft' ? 55 : n.marks.includes('划掉') ? 45 : 100)

  /** 划掉、圈出、下划线、星标 */
  const marks = (n: Node, x: number, y: number, w: number, size: number) => {
    const c = n.ink === 'draft' ? look.pencil : look.ink
    if (n.marks.includes('划掉'))
      sk.push({ type: 'line', x: x - 4, y: y + size * 0.62, points: [[0, 0], [w + 8, -2]] as never, strokeColor: c, strokeWidth: 2, roughness: 2 })
    if (n.marks.includes('下划线'))
      sk.push({ type: 'line', x: x - 2, y: y + size + 6, points: [[0, 0], [w + 4, 1]] as never, strokeColor: c, strokeWidth: 2, roughness: 1.5 })
    if (n.marks.includes('圈出'))
      sk.push({ type: 'ellipse', x: x - 16, y: y - 10, width: w + 32, height: size + 22, strokeColor: look.question, roughness: 2, backgroundColor: 'transparent' })
    if (n.marks.includes('星标'))
      sk.push({ type: 'text', x: x + w + 10, y: y - 2, text: '★', fontSize: size, fontFamily: HAND, strokeColor: look.question })
  }

  const hotBg = (id: string, x: number, y: number, w: number, h: number) => {
    if (hot.has(id))
      sk.push({ type: 'rectangle', x: x - 6, y: y - 4, width: w + 12, height: h + 8, strokeColor: 'transparent', backgroundColor: look.highlight, fillStyle: 'hachure', roughness: 1 })
  }

  // —— 话题区域 ——
  for (const r of l.regions) {
    if (r.title) {
      const head = r.head
      const tx = r.x + r.title.x
      const c = head ? color(head) : look.group
      if (head) hotBg(head.id, tx, r.y, r.title.w, TITLE)
      if (head?.icon) sk.push(...icon(head.icon, r.x, r.y - 2, 36, c))
      sk.push({
        type: 'text',
        id: `${head?.id ?? r.id}:t`,
        x: tx,
        y: r.y,
        text: r.title.text,
        fontSize: TITLE,
        fontFamily: HAND,
        strokeColor: c,
        opacity: head ? dim(head) : 100,
      })
      // 标题下面一道手绘横线，像板书里随手划的
      sk.push({ type: 'line', x: tx - 4, y: r.y + TITLE + 10, points: [[0, 0], [r.title.w + 16, 3]] as never, strokeColor: c, strokeWidth: 2, roughness: 2, opacity: head ? dim(head) : 100 })
      if (head) marks(head, tx, r.y, r.title.w, TITLE)
    }
    for (const row of r.rows) drawRow(row, r.x, r.y)
  }

  function drawRow(row: Row, ox: number, oy: number) {
    const n = row.node
    const x = ox + row.x
    const y = oy + row.y
    const pre = row.prefix ? 30 : 0
    const c =
      n.ink === 'draft' ? look.pencil : n.kind === 'question' ? look.question : n.kind === 'note' ? look.note : look.ink
    hotBg(n.id, x, y, pre + row.w, row.size + 4)
    if (row.prefix)
      sk.push({ type: 'text', x, y, text: row.prefix, fontSize: row.size, fontFamily: HAND, strokeColor: c, opacity: dim(n) })
    if (n.kind === 'formula')
      sk.push({ type: 'ellipse', x: x + pre - 14, y: y - 10, width: row.w + 28, height: row.size + 20, strokeColor: c, roughness: 1.5, backgroundColor: 'transparent', opacity: dim(n) })
    sk.push({ type: 'text', id: `${n.id}:t`, x: x + pre, y, text: row.text, fontSize: row.size, fontFamily: HAND, strokeColor: c, opacity: dim(n) })
    if (n.icon) sk.push(...icon(n.icon, x + pre + row.w + 10, y - 2, row.size + 6, c))
    marks(n, x + pre, y, row.w, row.size)
  }

  // —— 流程图区域 ——
  const d = l.diagram
  if (d) {
    for (const grp of d.groups) {
      sk.push({ type: 'rectangle', id: grp.id, x: grp.x, y: grp.y, width: grp.w, height: grp.h, strokeColor: look.group, strokeStyle: 'dashed', roughness: 2, roundness: { type: 3 }, backgroundColor: 'transparent' })
      sk.push({ type: 'text', x: grp.x + 14, y: grp.y + 10, text: grp.label, fontSize: FONT - 2, fontFamily: HAND, strokeColor: look.group })
    }
    for (const b of d.nodes) {
      const n = g.nodes.find((x) => x.id === b.id)
      if (!n) continue
      const draft = n.ink === 'draft'
      const iconW = n.icon ? 40 : 0
      sk.push({
        type: 'rectangle',
        id: n.id,
        x: b.x,
        y: b.y,
        width: b.w,
        height: b.h,
        strokeColor: color(n),
        strokeStyle: draft ? 'dashed' : 'solid',
        opacity: dim(n),
        backgroundColor: hot.has(n.id) ? look.highlight : 'transparent',
        fillStyle: 'hachure',
        roughness: 1,
        roundness: { type: 3 },
        label: n.icon ? undefined : { text: n.label, fontSize: FONT, fontFamily: HAND, strokeColor: color(n) },
      })
      if (n.icon) {
        sk.push(...icon(n.icon, b.x + 12, b.y + (b.h - 30) / 2, 30, color(n)))
        sk.push({ type: 'text', x: b.x + 12 + iconW, y: b.y + (b.h - FONT) / 2 - 2, text: n.label, fontSize: FONT, fontFamily: HAND, strokeColor: color(n), opacity: dim(n) })
      }
      marks(n, b.x + (b.w - textWidth(n.label, FONT)) / 2, b.y + (b.h - FONT) / 2, textWidth(n.label, FONT), FONT)
    }
    for (const r of d.edges) {
      const e = g.edges.find((x) => x.from === r.from && x.to === r.to)
      if (!e) continue
      const draft = e.ink === 'draft'
      const [x0, y0] = r.points[0]
      sk.push({
        type: 'arrow',
        x: x0,
        y: y0,
        points: r.points.map(([x, y]) => [x - x0, y - y0] as const) as never,
        strokeColor: draft ? look.pencil : look.ink,
        strokeStyle: draft ? 'dashed' : 'solid',
        opacity: draft ? 55 : 100,
        roughness: 1,
        start: { id: e.from },
        end: { id: e.to },
        label: e.label ? { text: e.label, fontSize: FONT - 4, fontFamily: HAND } : undefined,
      })
    }
  }

  // —— 跨区域的连线：一条带弧度的手绘箭头 ——
  // 箭头不能直接绑在文字上（Excalidraw 会把文字挪到箭头端点），
  // 所以给被连到的话题和条目各垫一个透明框，箭头绑在框上
  const anchored = new Set<string>()
  for (const link of l.links)
    for (const id of [link.from, link.to]) {
      const b = l.anchors.get(id)
      if (!b || anchored.has(id) || d?.nodes.some((x) => x.id === id)) continue
      anchored.add(id)
      sk.push({ type: 'rectangle', id, x: b.x - 8, y: b.y - 6, width: b.w + 16, height: b.h + 6, strokeColor: 'transparent', backgroundColor: 'transparent', roughness: 0 })
    }
  for (const link of l.links) {
    const e = g.edges.find((x) => x.from === link.from && x.to === link.to)
    const a = l.anchors.get(link.from)
    const b = l.anchors.get(link.to)
    if (!e || !a || !b) continue
    // 从中心连线，再裁到两个框的边上，箭头就不会穿过框里的字
    const ca: [number, number] = [a.x + a.w / 2, a.y + a.h / 2]
    const cb: [number, number] = [b.x + b.w / 2, b.y + b.h / 2]
    const [ax, ay] = edgePoint(a, ca, cb)
    const [bx, by] = edgePoint(b, cb, ca)
    // 中点往法线方向偏一点，箭头就弯了
    const len = Math.hypot(bx - ax, by - ay) || 1
    const bend = Math.min(60, len * 0.15)
    const mx = (ax + bx) / 2 + (-(by - ay) / len) * bend
    const my = (ay + by) / 2 + ((bx - ax) / len) * bend
    const draft = e.ink === 'draft'
    sk.push({
      type: 'arrow',
      x: ax,
      y: ay,
      points: [[0, 0], [mx - ax, my - ay], [bx - ax, by - ay]] as never,
      roundness: { type: 2 },
      strokeColor: draft ? look.pencil : look.ink,
      strokeStyle: draft ? 'dashed' : 'solid',
      opacity: draft ? 55 : 100,
      roughness: 1.5,
      start: { id: e.from },
      end: { id: e.to },
      label: e.label ? { text: e.label, fontSize: FONT - 4, fontFamily: HAND } : undefined,
    })
  }

  return convertToExcalidrawElements(sk, { regenerateIds: false })
}
