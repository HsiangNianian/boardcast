// 排版结果 → 一笔一笔的墨迹。和 board/scene.ts 吃同一份 Layout，只是换了一支「真的笔」。

import type { Layout, Row } from '../board/layout.ts'
import { FONT, TITLE } from '../board/layout.ts'
import type { Graph, Node } from '../engine/graph.ts'
import { type Pt, type Stroke, glyph } from './glyphs.ts'
import { type Inked, arrow, box, ink, line, loop, place, rng, strike } from './pen.ts'

export interface InkLook {
  paper: string
  ink: string
  pencil: string
  red: string
  blue: string
  highlight: string
  /** 粉笔模式下笔迹带一点颗粒感 */
  chalk: boolean
}

export interface Piece {
  /** 同一个 key 写过一次就不再重写：文字变了才重写，只是位置变了就直接挪过去 */
  key: string
  color: string
  opacity: number
  strokes: Inked[]
  /** 荧光笔一类的底色，画在字下面，不参与写字动画 */
  under?: boolean
}

const penFor = (size: number) => Math.max(1.8, size * 0.1)

async function write(text: string, x: number, y: number, size: number, seed: string): Promise<Stroke[]> {
  const r = rng(seed)
  const out: Stroke[] = []
  let cx = x
  for (const ch of text) {
    const g = await glyph(ch)
    if (!g) {
      cx += size * 0.6
      continue
    }
    for (const s of g.strokes) out.push(place(s, cx, y, size, r))
    cx += g.advance * size * (0.94 + r() * 0.1) + size * 0.04
  }
  return out
}

function edgePoint(b: { x: number; y: number; w: number; h: number }, c: Pt, p: Pt): Pt {
  const hw = b.w / 2 + 12
  const hh = b.h / 2 + 12
  const dx = p[0] - c[0]
  const dy = p[1] - c[1]
  if (!dx && !dy) return c
  const t = Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity, 1)
  return [c[0] + dx * t, c[1] + dy * t]
}

export async function compose(g: Graph, l: Layout, hot: Set<string>, look: InkLook): Promise<Piece[]> {
  const pieces: Piece[] = []
  const colorOf = (n: Node | undefined, fallback = look.ink) =>
    n?.ink === 'draft' ? look.pencil : n?.kind === 'question' ? look.red : n?.kind === 'note' ? look.blue : fallback
  const alpha = (n: Node | undefined) => (n?.ink === 'draft' ? 0.55 : n?.marks.includes('划掉') ? 0.55 : 1)

  const add = (key: string, color: string, opacity: number, strokes: Stroke[], size: number, extra: Partial<Piece> = {}) => {
    const r = rng(key)
    pieces.push({ key, color, opacity, strokes: strokes.map((s) => ink(s, size, r)), ...extra })
  }

  /** 一段文字连同它的标记 */
  const text = async (n: Node | undefined, id: string, str: string, x: number, y: number, size: number, color: string, w: number) => {
    const key = `${id}|${str}`
    if (n && hot.has(n.id)) {
      const r = rng(`${key}hl`)
      pieces.push({ key: `${key}|hot`, color: look.highlight, opacity: 0.55, under: true, strokes: [ink(line([x - 4, y + size * 0.62], [x + w + 4, y + size * 0.58], r, 0.01), size * 0.9, r, { wobble: 1, taper: 0 })] })
    }
    add(key, color, alpha(n), await write(str, x, y, size, key), penFor(size))
    if (!n) return
    const r = rng(`${key}m`)
    const markColor = n.ink === 'draft' ? look.pencil : look.red
    if (n.marks.includes('划掉')) add(`${key}|x`, n.ink === 'draft' ? look.pencil : look.ink, 1, strike(x, y + size * 0.6, w, r), penFor(size) * 1.1)
    if (n.marks.includes('下划线')) add(`${key}|u`, markColor, 1, [line([x - 2, y + size + 7], [x + w + 6, y + size + 5], r, 0.02)], penFor(size))
    if (n.marks.includes('圈出')) add(`${key}|o`, markColor, 1, [loop(x + w / 2, y + size / 2, w / 2 + 18, size / 2 + 14, r)], penFor(size))
    if (n.marks.includes('星标')) add(`${key}|s`, markColor, 1, await write('★', x + w + 10, y - 2, size, `${key}s`), penFor(size))
  }

  // —— 话题区域 ——
  for (const reg of l.regions) {
    if (reg.title) {
      const head = reg.head
      const tx = reg.x + reg.title.x
      const c = head ? colorOf(head) : look.blue
      await text(head, head?.id ?? reg.id, reg.title.text, tx, reg.y, TITLE, c, reg.title.w)
      const r = rng(`${reg.id}ul`)
      add(`${reg.id}|${reg.title.text}|ul`, c, alpha(head), [line([tx - 6, reg.y + TITLE + 10], [tx + reg.title.w + 14, reg.y + TITLE + 13], r, 0.03)], penFor(TITLE))
    }
    for (const row of reg.rows) await drawRow(row, reg.x, reg.y)
  }

  async function drawRow(row: Row, ox: number, oy: number) {
    const n = row.node
    const x = ox + row.x
    const y = oy + row.y
    const c = colorOf(n)
    const pre = row.prefix ? 30 : 0
    if (row.prefix) add(`${n.id}|pre|${row.prefix}`, c, alpha(n), await write(row.prefix, x, y, row.size, `${n.id}pre`), penFor(row.size))
    if (n.kind === 'formula') {
      const r = rng(`${n.id}f`)
      add(`${n.id}|f|${row.text}`, c, alpha(n), [loop(x + pre + row.w / 2, y + row.size / 2, row.w / 2 + 16, row.size / 2 + 10, r)], penFor(row.size) * 0.9)
    }
    await text(n, n.id, row.text, x + pre, y, row.size, c, row.w)
  }

  // —— 流程图区域 ——
  const d = l.diagram
  if (d) {
    for (const grp of d.groups) {
      const r = rng(grp.id)
      add(`${grp.id}|${grp.label}`, look.blue, 0.8, box(grp.x, grp.y, grp.w, grp.h, r), penFor(FONT) * 0.8)
      await text(undefined, `${grp.id}t`, grp.label, grp.x + 14, grp.y + 10, FONT - 2, look.blue, 0)
    }
    for (const b of d.nodes) {
      const n = g.nodes.find((x) => x.id === b.id)
      if (!n) continue
      const r = rng(n.id)
      add(`${n.id}|box`, colorOf(n), alpha(n), box(b.x + 4, b.y + 4, b.w - 8, b.h - 8, r), penFor(FONT))
      const tw = [...n.label].reduce((s, ch) => s + (ch.charCodeAt(0) < 128 ? 0.55 : 1), 0) * FONT
      await text(n, n.id, n.label, b.x + (b.w - tw) / 2, b.y + (b.h - FONT) / 2 - 2, FONT, colorOf(n), tw)
    }
    for (const rt of d.edges) {
      const e = g.edges.find((x) => x.from === rt.from && x.to === rt.to)
      if (!e) continue
      const r = rng(`${e.from}>${e.to}`)
      const c = e.ink === 'draft' ? look.pencil : look.ink
      add(`${e.from}>${e.to}`, c, e.ink === 'draft' ? 0.55 : 1, arrow(rt.points as Pt[], r), penFor(FONT) * 0.9)
      if (e.label) {
        const [a, b2] = [rt.points[0], rt.points[rt.points.length - 1]]
        await text(undefined, `${e.from}>${e.to}l`, e.label, (a[0] + b2[0]) / 2 - 10, (a[1] + b2[1]) / 2 - 24, FONT - 4, c, 0)
      }
    }
  }

  // —— 跨区域连线 ——
  for (const link of l.links) {
    const e = g.edges.find((x) => x.from === link.from && x.to === link.to)
    const a = l.anchors.get(link.from)
    const b = l.anchors.get(link.to)
    if (!e || !a || !b) continue
    const ca: Pt = [a.x + a.w / 2, a.y + a.h / 2]
    const cb: Pt = [b.x + b.w / 2, b.y + b.h / 2]
    const p0 = edgePoint(a, ca, cb)
    const p1 = edgePoint(b, cb, ca)
    const len = Math.hypot(p1[0] - p0[0], p1[1] - p0[1]) || 1
    const bend = Math.min(50, len * 0.15)
    const mid: Pt = [(p0[0] + p1[0]) / 2 - ((p1[1] - p0[1]) / len) * bend, (p0[1] + p1[1]) / 2 + ((p1[0] - p0[0]) / len) * bend]
    // 二次贝塞尔采样成一笔
    const curve: Pt[] = []
    for (let i = 0; i <= 16; i++) {
      const t = i / 16
      curve.push([
        (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * mid[0] + t * t * p1[0],
        (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * mid[1] + t * t * p1[1],
      ])
    }
    const c = e.ink === 'draft' ? look.pencil : look.ink
    const r = rng(`${e.from}~${e.to}`)
    add(`${e.from}~${e.to}`, c, e.ink === 'draft' ? 0.55 : 1, arrow(curve, r), penFor(FONT) * 0.9)
    // 标签放在曲线中点（t = 0.5），不是控制点
    const on: Pt = [(p0[0] + 2 * mid[0] + p1[0]) / 4, (p0[1] + 2 * mid[1] + p1[1]) / 4]
    // 再沿法线往弯的外侧挪一点，别压在线上
    const nx = -(p1[1] - p0[1]) / len
    const ny = (p1[0] - p0[0]) / len
    if (e.label) await text(undefined, `${e.from}~${e.to}l`, e.label, on[0] + nx * 22 - 12, on[1] + ny * 22 - 10, FONT - 4, c, 0)
  }
  return pieces
}
