// 排版结果 → 一笔一笔的墨迹。和 board/scene.ts 吃同一份 Layout，只是换了一支「真的笔」。

import type { Layout, Row } from '../board/layout.ts'
import { type Rect, labelSpot } from '../board/labels.ts'
import { FONT, TITLE } from '../board/layout.ts'
import { inherited } from '../engine/base.ts'
import type { Graph, Node } from '../engine/graph.ts'
import { type Pt, type Stroke, glyph } from './glyphs.ts'
import { inkIcon } from './icons.ts'
import { type Inked, arrow, box, ink, line, loop, outline, placeGlyph, rng, strike } from './pen.ts'

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

/** 带着旧板开会（#49）：底板上原样留下来的东西用淡墨，这么浓 */
export const OLD_INK = 0.38

/** 一笔；`thin` 是连笔时带出来的细丝，用更细的笔画 */
type Mark = { pts: Stroke; thin?: boolean }

/** 潦草度超过这个值，英文从印刷体换成连写体 */
export const SCRIPT_AT = 0.45

const isLower = (ch: string) => /[a-z]/.test(ch)
const isHan = (ch: string) => /[㐀-鿿]/.test(ch)
const dist = (a: Pt, b: Pt) => Math.hypot(a[0] - b[0], a[1] - b[1])

async function write(text: string, x: number, y: number, size: number, seed: string, messy: number): Promise<Mark[]> {
  const r = rng(seed)
  const cursive = messy >= SCRIPT_AT
  const out: Mark[] = []
  let cx = x
  let prev: { ch: string; end: Pt } | undefined
  for (const ch of text) {
    // 连写体的大写字母是花体，在板上认不出来；真人手写也多是大写印刷、小写连写
    const latin = cursive && isLower(ch) ? 'script' : 'print'
    const g = await glyph(ch, latin)
    if (!g) {
      cx += size * 0.6
      prev = undefined
      continue
    }
    const placed = placeGlyph(g.strokes, cx, y, size, r, messy)
    // 英文连写：同一个单词里，上一个小写字母的收笔接到这个小写字母的起笔
    if (latin === 'script' && prev && isLower(prev.ch) && placed[0]?.length) {
      const start = placed[0][0]
      if (dist(prev.end, start) < size * 0.7) out.push({ pts: [prev.end, start] })
    }
    for (let i = 0; i < placed.length; i++) {
      out.push({ pts: placed[i] })
      // 汉字连笔：越潦草，上一笔的收笔越容易带出一根细丝连到下一笔的起笔
      const next = placed[i + 1]
      if (isHan(ch) && next && messy > 0.5 && r() < (messy - 0.5) * 1.4) {
        const a = placed[i][placed[i].length - 1]
        const b = next[0]
        if (dist(a, b) < size * 0.55) out.push({ pts: [a, [(a[0] + b[0]) / 2 + (r() - 0.5) * size * 0.1, (a[1] + b[1]) / 2], b], thin: true })
      }
    }
    const last = placed[placed.length - 1]
    prev = last?.length ? { ch, end: last[last.length - 1] } : undefined
    // 越潦草字距越挤、越不均匀
    cx += g.advance * size * (0.96 - 0.1 * messy + r() * (0.08 + 0.22 * messy)) + size * 0.04
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

/**
 * `weight`：笔画粗细的倍数，1 是默认粗细。
 * `base`：带着旧板开会时的底板。它上面原样留下来的东西（节点、连线、名字、标记）用淡墨，
 * 本场新加的、改了名字的、新做的标记用正常墨迹，一眼看出这场会改了哪儿。
 */
export async function compose(
  g: Graph,
  l: Layout,
  hot: Set<string>,
  look: InkLook,
  messy = 0.3,
  weight = 1,
  base?: Graph,
): Promise<Piece[]> {
  const pieces: Piece[] = []
  const colorOf = (n: Node | undefined, fallback = look.ink) =>
    n?.ink === 'draft' ? look.pencil : n?.kind === 'question' ? look.red : n?.kind === 'note' ? look.blue : fallback
  // 底板上原样留下来的用淡墨。改了名字的旧节点：框还是旧的，名字用新墨写
  const old = inherited(base)
  const fade = (yes: boolean) => (yes ? OLD_INK : 1)
  const alpha = (n: Node | undefined) => (n?.ink === 'draft' ? 0.55 : n?.marks.includes('划掉') ? 0.55 : 1)

  const add = (key: string, color: string, opacity: number, strokes: (Stroke | Mark)[], size: number, extra: Partial<Piece> = {}) => {
    const r = rng(key)
    const wobble = size * (0.12 + 0.5 * messy)
    const pen = size * weight
    const inked = strokes.map((s) =>
      Array.isArray(s) ? ink(s, pen, r, { messy, wobble }) : ink(s.pts, s.thin ? pen * 0.45 : pen, r, { messy, wobble }),
    )
    pieces.push({ key, color, opacity, strokes: inked, ...extra })
  }

  /** 一段文字连同它的标记。不属于节点的字（分组名、连线标签）用 `old` 说它是不是底板上原样留下的 */
  const text = async (n: Node | undefined, id: string, str: string, x: number, y: number, size: number, color: string, w: number, faded = false) => {
    const key = `${id}|${str}`
    if (n && hot.has(n.id)) {
      const r = rng(`${key}hl`)
      pieces.push({ key: `${key}|hot`, color: look.highlight, opacity: 0.55, under: true, strokes: [ink(line([x - 4, y + size * 0.62], [x + w + 4, y + size * 0.58], r, 0.01), size * 0.9, r, { wobble: 1, taper: 0 })] })
    }
    add(key, color, alpha(n) * fade(n ? old.label(n) : faded), await write(str, x, y, size, key, messy), penFor(size))
    if (!n) return
    const r = rng(`${key}m`)
    const markColor = n.ink === 'draft' ? look.pencil : look.red
    // 标记是这场会做的就用新墨，哪怕标在淡墨的旧字上
    if (n.marks.includes('划掉')) add(`${key}|x`, n.ink === 'draft' ? look.pencil : look.ink, fade(old.mark(n, '划掉')), strike(x, y + size * 0.6, w, r), penFor(size) * 1.1)
    if (n.marks.includes('下划线')) add(`${key}|u`, markColor, fade(old.mark(n, '下划线')), [line([x - 2, y + size + 7], [x + w + 6, y + size + 5], r, 0.02)], penFor(size))
    if (n.marks.includes('圈出')) add(`${key}|o`, markColor, fade(old.mark(n, '圈出')), [loop(x + w / 2, y + size / 2, w / 2 + 18, size / 2 + 14, r)], penFor(size))
    if (n.marks.includes('星标')) add(`${key}|s`, markColor, fade(old.mark(n, '星标')), await write('★', x + w + 10, y - 2, size, `${key}s`, messy), penFor(size))
  }

  // —— 导入页上的原文：本身不画（底图上就有），只画勾画——圈、划、下划线、星标，被提到时整块淡淡地涂一下 ——
  for (const n of g.nodes) {
    if (n.kind !== 'region' || !n.at) continue
    const { x, y, w, h } = n.at
    const r = rng(`${n.id}|pin`)
    const c = n.ink === 'draft' ? look.pencil : look.red
    if (hot.has(n.id))
      pieces.push({ key: `${n.id}|hot`, color: look.highlight, opacity: 0.45, under: true, strokes: [ink(line([x - 4, y + h / 2], [x + w + 4, y + h / 2], r, 0.005), h + 6, r, { wobble: 1, taper: 0 })] })
    if (n.marks.includes('圈出')) add(`${n.id}|o`, c, fade(old.mark(n, '圈出')), [loop(x + w / 2, y + h / 2, w / 2 + 14, h / 2 + 12, r)], penFor(FONT))
    if (n.marks.includes('下划线')) add(`${n.id}|u`, c, fade(old.mark(n, '下划线')), [line([x - 2, y + h + 6], [x + w + 4, y + h + 4], r, 0.01)], penFor(FONT))
    if (n.marks.includes('划掉')) add(`${n.id}|x`, c, fade(old.mark(n, '划掉')), h > 40 ? [line([x, y], [x + w, y + h], r, 0.02), line([x + w, y], [x, y + h], r, 0.02)] : strike(x, y + h / 2, w, r), penFor(FONT) * 1.1)
    if (n.marks.includes('星标')) add(`${n.id}|s`, c, fade(old.mark(n, '星标')), await write('★', x - FONT - 10, y + (h - FONT) / 2, FONT, `${n.id}s`, messy), penFor(FONT))
    // 圈画在一个词、一句话上的
    for (const s of n.spans ?? []) {
      const key = `${n.id}|${s.mark}|${s.boxes.map((b) => `${b.x},${b.y}`).join(';')}`
      const sr = rng(key)
      const thin = penFor(FONT) * 0.8
      const o = fade(old.span(n, s))
      if (s.mark === '圈出') {
        const b = s.boxes[0]
        add(key, c, o, [s.boxes.length === 1 ? loop(b.x + b.w / 2, b.y + b.h / 2, b.w / 2 + 6, b.h / 2 + 4, sr) : outline(s.boxes, 4, sr)], thin)
      }
      if (s.mark === '下划线') add(key, c, o, s.boxes.map((b) => line([b.x, b.y + b.h + 2], [b.x + b.w, b.y + b.h + 2], sr, 0.004)), thin)
      if (s.mark === '划掉') add(key, c, o, s.boxes.flatMap((b) => strike(b.x, b.y + b.h / 2, b.w, sr)), thin)
      if (s.mark === '星标') add(key, c, o, await write('★', s.boxes[0].x - 14, s.boxes[0].y, 12, key, messy), thin)
    }
  }
  // 边栏里的批注：一根细线连回它说的那块原文
  for (const row of l.regions.find((x) => x.id === '__margin')?.rows ?? []) {
    const host = g.nodes.find((p) => p.id === row.node.parent)?.at
    if (!host) continue
    const r = rng(`${row.id}|tether`)
    add(`${row.id}|tether`, look.blue, 0.6 * fade(old.node(row.node)), [line([host.x + host.w + 6, host.y + Math.min(host.h, 30) / 2], [row.x - 8, row.y + row.size / 2], r, 0.02)], penFor(FONT) * 0.5)
  }

  // —— 话题区域 ——
  for (const reg of l.regions) {
    if (reg.title) {
      const head = reg.head
      const tx = reg.x + reg.title.x
      const c = head ? colorOf(head) : look.blue
      // 没有话题的那块（「待定」之类）：里面全是旧的，标题也是旧的
      const was = head ? old.node(head) : !!base && reg.rows.length > 0 && reg.rows.every((x) => old.node(x.node))
      if (head?.icon) add(`${head.id}|icon|${head.icon}`, c, alpha(head) * fade(old.icon(head)), inkIcon(head.icon, reg.x, reg.y - 2, 36, rng(`${head.id}i`)), penFor(FONT) * 0.85)
      await text(head, head?.id ?? reg.id, reg.title.text, tx, reg.y, TITLE, c, reg.title.w, was)
      const r = rng(`${reg.id}ul`)
      add(`${reg.id}|${reg.title.text}|ul`, c, alpha(head) * fade(was), [line([tx - 6, reg.y + TITLE + 10], [tx + reg.title.w + 14, reg.y + TITLE + 13], r, 0.03)], penFor(TITLE))
    }
    for (const row of reg.rows) await drawRow(row, reg.x, reg.y)
  }

  async function drawRow(row: Row, ox: number, oy: number) {
    const n = row.node
    const x = ox + row.x
    const y = oy + row.y
    const c = colorOf(n)
    const pre = row.prefix ? 30 : 0
    const o = alpha(n) * fade(old.node(n))
    if (row.prefix) add(`${n.id}|pre|${row.prefix}`, c, o, await write(row.prefix, x, y, row.size, `${n.id}pre`, messy), penFor(row.size))
    if (n.kind === 'formula') {
      const r = rng(`${n.id}f`)
      add(`${n.id}|f|${row.text}`, c, o, [loop(x + pre + row.w / 2, y + row.size / 2, row.w / 2 + 16, row.size / 2 + 10, r)], penFor(row.size) * 0.9)
    }
    await text(n, n.id, row.text, x + pre, y, row.size, c, row.w)
    if (n.icon) add(`${n.id}|icon|${n.icon}`, c, alpha(n) * fade(old.icon(n)), inkIcon(n.icon, x + pre + row.w + 10, y - 2, row.size + 6, rng(`${n.id}i`)), penFor(row.size) * 0.85)
  }

  // 所有连线的走线和标签：标签留到最后统一放
  const lines: Pt[][] = []
  const labels: { key: string; text: string; route: Pt[]; color: string; prefer?: Pt[]; old: boolean }[] = []

  // —— 流程图区域 ——
  const d = l.diagram
  if (d) {
    for (const grp of d.groups) {
      const r = rng(grp.id)
      const was = old.group(grp)
      add(`${grp.id}|${grp.label}`, look.blue, 0.8 * fade(was), box(grp.x, grp.y, grp.w, grp.h, r), penFor(FONT) * 0.8)
      await text(undefined, `${grp.id}t`, grp.label, grp.x + 14, grp.y + 10, FONT - 2, look.blue, 0, was)
    }
    for (const b of d.nodes) {
      const n = g.nodes.find((x) => x.id === b.id)
      if (!n) continue
      const r = rng(n.id)
      add(`${n.id}|box`, colorOf(n), alpha(n) * fade(old.node(n)), box(b.x + 4, b.y + 4, b.w - 8, b.h - 8, r), penFor(FONT))
      const tw = [...n.label].reduce((s, ch) => s + (ch.charCodeAt(0) < 128 ? 0.55 : 1), 0) * FONT
      if (n.icon) {
        add(`${n.id}|icon|${n.icon}`, colorOf(n), alpha(n) * fade(old.icon(n)), inkIcon(n.icon, b.x + 14, b.y + (b.h - 30) / 2, 30, rng(`${n.id}i`)), penFor(FONT) * 0.85)
        await text(n, n.id, n.label, b.x + 54, b.y + (b.h - FONT) / 2 - 2, FONT, colorOf(n), tw)
      } else await text(n, n.id, n.label, b.x + (b.w - tw) / 2, b.y + (b.h - FONT) / 2 - 2, FONT, colorOf(n), tw)
    }
    for (const rt of d.edges) {
      const e = g.edges.find((x) => x.from === rt.from && x.to === rt.to)
      if (!e) continue
      const r = rng(`${e.from}>${e.to}`)
      const c = e.ink === 'draft' ? look.pencil : look.ink
      add(`${e.from}>${e.to}`, c, (e.ink === 'draft' ? 0.55 : 1) * fade(old.edge(e)), arrow(rt.points as Pt[], r), penFor(FONT) * 0.9)
      lines.push(rt.points as Pt[])
      if (e.label) labels.push({ key: `${e.from}>${e.to}l`, text: e.label, route: rt.points as Pt[], color: c, old: old.edge(e, true) })
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
    add(`${e.from}~${e.to}`, c, (e.ink === 'draft' ? 0.55 : 1) * fade(old.edge(e)), arrow(curve, r), penFor(FONT) * 0.9)
    lines.push(curve)
    // 标签优先放在曲线中点（t = 0.5，不是控制点）沿法线往弯的外侧挪一点
    const on: Pt = [(p0[0] + 2 * mid[0] + p1[0]) / 4, (p0[1] + 2 * mid[1] + p1[1]) / 4]
    const nx = -(p1[1] - p0[1]) / len
    const ny = (p1[0] - p0[0]) / len
    if (e.label) labels.push({ key: `${e.from}~${e.to}l`, text: e.label, route: curve, color: c, prefer: [[on[0] + nx * 22 - 12, on[1] + ny * 22 - 10]], old: old.edge(e, true) })
  }

  // —— 连线标签：所有线都画完了再放，这样能避开每一条线；后画的也会盖在线上面 ——
  // 能挡住标签的东西：流程图节点、话题里一行行的字（不算整块话题区域，否则标签哪儿都放不了）
  const regionIds = new Set(l.regions.map((x) => x.id))
  const blocks = [...l.anchors.entries()].filter(([id]) => !regionIds.has(id)).map(([, b]) => b)
  const placed: Rect[] = []
  for (const lb of labels) {
    const size = FONT - 4
    const spot = labelSpot(lb.route, lb.text, size, [...blocks, ...placed], lines, lb.prefer)
    placed.push(spot)
    // 实在找不到干净的地方：先垫一块纸色的底，线从下面过，字照样看得清
    if (!spot.clear) {
      const r = rng(`${lb.key}bg`)
      const cy = spot.y + spot.h / 2 + 1
      add(`${lb.key}|bg`, look.paper, 0.92, [line([spot.x - 4, cy], [spot.x + spot.w + 4, cy], r, 0)], spot.h + 8)
    }
    await text(undefined, lb.key, lb.text, spot.x, spot.y, size, lb.color, 0, lb.old)
  }
  return pieces
}
