// 板书 → 坐标。排版完全是确定性的，模型不参与。
//
// 一块板由若干「区域」拼起来：
// - 每个话题一块：标题 + 编号条目 + 挂在条目旁的问题、公式、备注；
// - 所有流程图节点合成一块，内部用 ELK 分层排版；
// - 没有挂靠的问题、公式放进「待定」一块。
// 区域按第一次出现的先后摆成几行，带一点错位，看起来更像随手写的。
//
// 增量排版（StableLayout）：会开到一半板书不该乱跳。
// - 已经摆好的区域记住位置，新区域找一块空地放；区域长大压到别人时，只把后来的往下推；
// - 流程图只在第一次用 ELK 排，之后旧节点不动，新节点放在它连着的节点旁边，再往下找空位；
//   两端都没动的连线沿用原来的走线；
// - 「整理排版」时清掉记忆，整块重新排。

import type { ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk.bundled.js'
import { type Graph, type Node, children } from '../engine/graph.ts'

// ELK 打包后有一百多万字节，只有出现流程图时才用得到：第一次要排流程图时再加载
let elkReady: Promise<{ layout: (g: ElkNode) => Promise<ElkNode> }> | undefined
const elk = () => (elkReady ??= import('elkjs/lib/elk.bundled.js').then((m) => new m.default()))

export const FONT = 20
export const TITLE = 28
const SMALL = 17

export interface Box {
  id: string
  x: number
  y: number
  w: number
  h: number
}

export interface Route {
  from: string
  to: string
  points: [number, number][]
}

export interface Row {
  id: string
  node: Node
  /** 「1.」「a.」「？」之类的前缀，单独画，不进模型看到的名字 */
  prefix: string
  text: string
  x: number
  y: number
  w: number
  size: number
}

export interface Region extends Box {
  head?: Node
  title?: { text: string; x: number; y: number; w: number }
  rows: Row[]
}

export interface Layout {
  regions: Region[]
  diagram?: { box: Box; nodes: Box[]; groups: (Box & { label: string })[]; edges: Route[] }
  /** 不在流程图内部的连线（话题、条目之间，或跨区域），画成弯曲的手绘箭头 */
  links: { from: string; to: string }[]
  /** 每个可以被箭头连接的东西的位置 */
  anchors: Map<string, Box>
  bounds: { x0: number; y0: number; x1: number; y1: number }
}

/** 粗估文字宽度：中日韩字符按一个字宽，其余按大半个。 */
export function textWidth(s: string, size: number): number {
  let w = 0
  for (const ch of s) w += /[⺀-鿿＀-￯]/.test(ch) ? size : size * 0.62
  return w
}

/** 稳定的伪随机：同一个 id 每次错位都一样，重排时不抖。 */
function jitter(id: string, amp: number): number {
  let h = 2166136261
  for (const c of id) h = Math.imul(h ^ c.charCodeAt(0), 16777619)
  return ((h >>> 0) / 0xffffffff - 0.5) * 2 * amp
}

const isAttach = (n: Node) => n.kind === 'question' || n.kind === 'formula' || n.kind === 'note'

export function isHead(g: Graph, n: Node): boolean {
  if (n.parent && g.nodes.some((p) => p.id === n.parent)) return false
  if (n.kind === 'topic') return true
  return n.kind === 'node' && children(g, n.id).some((c) => c.kind === 'item')
}

export function isDiagram(g: Graph, n: Node): boolean {
  return n.kind === 'node' && !isHead(g, n) && !(n.parent && g.nodes.some((p) => p.id === n.parent))
}

const LETTERS = 'abcdefghijklmnopqrstuvwxyz'

function prefixFor(n: Node, depth: number, index: number): string {
  if (n.kind === 'question') return '？'
  if (n.kind === 'note') return '↳'
  if (n.kind === 'formula') return ''
  if (depth === 1) return `${index + 1}.`
  if (depth === 2) return `${LETTERS[index % 26]}.`
  return '·'
}

/** 话题区域：标题在上，条目逐行往下，挂件紧跟在被挂的东西下面。 */
function topicRegion(g: Graph, head: Node): Region {
  const rows: Row[] = []
  let y = TITLE + 26
  const walk = (parent: Node, depth: number) => {
    let index = 0
    // 条目在前，挂件在后
    const kids = children(g, parent.id).sort((a, b) => Number(isAttach(a)) - Number(isAttach(b)))
    for (const c of kids) {
      const d = isAttach(c) ? Math.max(1, depth) : depth
      const size = isAttach(c) ? SMALL : FONT
      const prefix = prefixFor(c, d, index)
      if (!isAttach(c)) index++
      const x = 8 + (d - 1) * 30
      rows.push({ id: c.id, node: c, prefix, text: c.label, x, y, w: textWidth(c.label, size), size })
      y += c.kind === 'formula' ? size + 30 : size + 16
      if (!isAttach(c)) walk(c, depth + 1)
    }
  }
  walk(head, 1)
  const titleW = textWidth(head.label, TITLE) + (head.icon ? 46 : 0)
  const w = Math.max(titleW, ...rows.map((r) => r.x + (r.prefix ? 30 : 0) + r.w + 24)) + 16
  return {
    id: head.id,
    head,
    x: 0,
    y: 0,
    w,
    h: y + 8,
    title: { text: head.label, x: head.icon ? 46 : 0, y: 0, w: textWidth(head.label, TITLE) },
    rows,
  }
}

/** 没有挂靠、或者挂在流程图节点上的问题和公式，统一收进「待定」。 */
function looseRegion(g: Graph, loose: Node[]): Region {
  const rows: Row[] = []
  let y = TITLE + 26
  for (const n of loose) {
    const host = n.parent ? g.nodes.find((p) => p.id === n.parent) : undefined
    const text = host ? `${host.label}：${n.label}` : n.label
    rows.push({ id: n.id, node: n, prefix: prefixFor(n, 1, 0), text, x: 8, y, w: textWidth(text, FONT), size: FONT })
    y += n.kind === 'formula' ? FONT + 30 : FONT + 16
  }
  const w = Math.max(textWidth('待定', TITLE), ...rows.map((r) => r.x + 30 + r.w + 24)) + 16
  return { id: '__loose', x: 0, y: 0, w, h: y + 8, title: { text: '待定', x: 0, y: 0, w: textWidth('待定', TITLE) }, rows }
}

const nodeW = (n: Node) => Math.max(110, textWidth(n.label, FONT) + 44 + (n.icon ? 40 : 0))
const NODE_H = 64

async function diagramLayout(g: Graph, ids: Set<string>) {
  const leaf = (n: Node): ElkNode => ({ id: n.id, width: nodeW(n), height: NODE_H })
  const nodes = g.nodes.filter((n) => ids.has(n.id))
  const kids: ElkNode[] = []
  for (const grp of g.groups) {
    const members = nodes.filter((n) => n.group === grp.id)
    if (!members.length) continue
    kids.push({
      id: grp.id,
      layoutOptions: { 'elk.padding': '[top=44,left=24,bottom=24,right=24]' },
      children: members.map(leaf),
    })
  }
  for (const n of nodes) if (!n.group || !g.groups.some((x) => x.id === n.group)) kids.push(leaf(n))
  const inner = g.edges.filter((e) => ids.has(e.from) && ids.has(e.to))
  const edges: ElkExtendedEdge[] = inner.map((e, i) => ({ id: `e${i}`, sources: [e.from], targets: [e.to] }))
  const out = await (await elk()).layout({
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'RIGHT',
      'elk.spacing.nodeNode': '40',
      'elk.layered.spacing.nodeNodeBetweenLayers': '80',
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.edgeRouting': 'POLYLINE',
    },
    children: kids,
    edges,
  })
  const boxes: Box[] = []
  const groups: (Box & { label: string })[] = []
  const walk = (n: ElkNode, ox: number, oy: number) => {
    for (const c of n.children ?? []) {
      const x = ox + (c.x ?? 0)
      const y = oy + (c.y ?? 0)
      const box = { id: c.id, x, y, w: c.width ?? 0, h: c.height ?? 0 }
      const grp = g.groups.find((q) => q.id === c.id)
      if (grp) {
        groups.push({ ...box, label: grp.label })
        walk(c, x, y)
      } else boxes.push(box)
    }
  }
  walk(out, 0, 0)
  // INCLUDE_CHILDREN 时边的坐标相对于它所在的容器，这里统一换成绝对坐标
  const offset = new Map<string, [number, number]>([['root', [0, 0]]])
  for (const grp of groups) offset.set(grp.id, [grp.x, grp.y])
  const routes: Route[] = []
  const collect = (n: ElkNode) => {
    for (const e of (n.edges ?? []) as (ElkExtendedEdge & { container?: string })[]) {
      const [ox, oy] = offset.get(e.container ?? 'root') ?? [0, 0]
      const s = e.sections?.[0]
      const src = inner[Number(e.id.slice(1))]
      if (!s || !src) continue
      routes.push({
        from: src.from,
        to: src.to,
        points: [s.startPoint, ...(s.bendPoints ?? []), s.endPoint].map((p) => [p.x + ox, p.y + oy]),
      })
    }
    for (const c of n.children ?? []) collect(c)
  }
  collect(out)
  const root = out as ElkNode
  return { x0: 0, y0: 0, w: root.width ?? 0, h: root.height ?? 0, nodes: boxes, groups, edges: routes }
}

type Diagram = Awaited<ReturnType<typeof diagramLayout>>

const overlaps = (a: Box, b: Box, m: number) =>
  a.x < b.x + b.w + m && b.x < a.x + a.w + m && a.y < b.y + b.h + m && b.y < a.y + a.h + m

type P = [number, number]

/** 折线的某一段（横的或竖的）有没有穿过别的节点 */
function crosses(points: P[], boxes: Box[]): boolean {
  for (let i = 1; i < points.length; i++) {
    const [x1, y1] = points[i - 1]
    const [x2, y2] = points[i]
    const seg = { id: '', x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) }
    if (boxes.some((b) => overlaps(seg, { ...b, x: b.x + 4, y: b.y + 4, w: b.w - 8, h: b.h - 8 }, 0))) return true
  }
  return false
}

/**
 * 增量放进来的连线：顺着往右走折线，往回连从下面绕，上下摆着就竖着连；
 * 路上压到别的节点时，改从这些节点的下方（或上方）绕过去。
 */
function route(a: Box, b: Box, others: Box[] = []): P[] {
  const ay = a.y + a.h / 2
  const by = b.y + b.h / 2
  const ax = a.x + a.w / 2
  const bx = b.x + b.w / 2
  const cands: P[][] = []
  if (b.x >= a.x + a.w + 20) {
    const sx = a.x + a.w
    const mx = (sx + b.x) / 2
    cands.push(
      Math.abs(ay - by) < 2
        ? [
            [sx, ay],
            [b.x, by],
          ]
        : [
            [sx, ay],
            [mx, ay],
            [mx, by],
            [b.x, by],
          ],
    )
  } else if (b.y >= a.y + a.h + 20 || a.y >= b.y + b.h + 20) {
    // 上下摆着：从一个的底边（或顶边）竖着连到另一个
    const [top, bottom] = b.y >= a.y ? [a.y + a.h, b.y] : [a.y, b.y + b.h]
    const mid = (top + bottom) / 2
    cands.push([
      [ax, top],
      [ax, mid],
      [bx, mid],
      [bx, bottom],
    ])
  }
  // 绕行：从两端节点之间所有挡路节点的下面（或上面）走一条通道
  const x0 = Math.min(a.x, b.x)
  const x1 = Math.max(a.x + a.w, b.x + b.w)
  const between = others.filter((o) => o.x < x1 && o.x + o.w > x0)
  const low = Math.max(a.y + a.h, b.y + b.h, ...between.map((o) => o.y + o.h)) + 28
  const high = Math.min(a.y, b.y, ...between.map((o) => o.y)) - 28
  cands.push(
    [
      [ax, a.y + a.h],
      [ax, low],
      [bx, low],
      [bx, b.y + b.h],
    ],
    [
      [ax, a.y],
      [ax, high],
      [bx, high],
      [bx, b.y],
    ],
  )
  return cands.find((c) => !crosses(c, others)) ?? cands[cands.length - 2]
}

const GAP = 90
const NODE_GAP = 40

/** 一次排版：区域位置和流程图节点都记在实例里，下次排同一块板时尽量不动 */
export class StableLayout {
  /** 区域原点的绝对位置（流程图区域记的是它内部坐标 (0,0) 的位置） */
  private regionAt = new Map<string, { x: number; y: number }>()
  /** 流程图节点在流程图内部坐标里的位置 */
  private nodeAt = new Map<string, Box>()
  /** 流程图连线的走线，连同当时两端的位置；两端都没动就沿用 */
  private routeAt = new Map<string, { sig: string; points: [number, number][] }>()
  /** 上一次每块区域占的地方（绝对坐标）；流程图长新节点时避开别的区域 */
  private boxAt = new Map<string, Box>()
  private queue: Promise<unknown> = Promise.resolve()
  private cache = new WeakMap<Graph, Promise<Layout>>()

  /** 忘掉所有位置，下一次整块重新排（清空白板、整理排版时用） */
  reset() {
    this.regionAt.clear()
    this.nodeAt.clear()
    this.routeAt.clear()
    this.boxAt.clear()
    this.cache = new WeakMap()
  }

  /** 同一个图对象只排一次；排版按调用顺序一个个来，后一次能看到前一次记下的位置 */
  layout(g: Graph): Promise<Layout> {
    let p = this.cache.get(g)
    if (!p) {
      p = this.queue.then(() => this.compute(g))
      this.queue = p.catch(() => undefined)
      this.cache.set(g, p)
    }
    return p
  }

  private async diagram(g: Graph, ids: Set<string>): Promise<Diagram> {
    for (const id of [...this.nodeAt.keys()]) if (!ids.has(id)) this.nodeAt.delete(id)
    const inner = g.edges.filter((e) => ids.has(e.from) && ids.has(e.to))
    const sig = (e: { from: string; to: string }, at: Map<string, Box>) => {
      const a = at.get(e.from)!
      const b = at.get(e.to)!
      return [a.x, a.y, a.w, b.x, b.y, b.w].map((v) => v.toFixed(1)).join(',')
    }

    // 第一次：交给 ELK 整块排，记下来
    if (!this.nodeAt.size) {
      const d = await diagramLayout(g, ids)
      for (const b of d.nodes) this.nodeAt.set(b.id, b)
      this.routeAt.clear()
      for (const e of d.edges) this.routeAt.set(`${e.from}>${e.to}`, { sig: sig(e, this.nodeAt), points: e.points })
      return d
    }

    // 之后：旧节点原地不动（改名变宽只改宽度），新节点挨着连着它的节点放
    // 别的区域换算到流程图内部坐标，新节点也不往那里放，免得把它们挤走
    const o = this.regionAt.get('__diagram') ?? { x: 0, y: 0 }
    const others = [...this.boxAt]
      .filter(([k]) => k !== '__diagram')
      .map(([, b]) => ({ ...b, x: b.x - o.x - GAP / 2, y: b.y - o.y - GAP / 2, w: b.w + GAP, h: b.h + GAP }))
    const placed = new Map<string, Box>()
    for (const n of g.nodes) {
      const old = this.nodeAt.get(n.id)
      if (ids.has(n.id) && old) placed.set(n.id, { ...old, w: nodeW(n) })
    }
    for (const n of g.nodes) {
      if (!ids.has(n.id) || placed.has(n.id)) continue
      const w = nodeW(n)
      const all = [...placed.values()]
      const from = inner.find((e) => e.to === n.id && placed.has(e.from))
      const to = inner.find((e) => e.from === n.id && placed.has(e.to))
      const mate = n.group ? g.nodes.find((m) => m.group === n.group && placed.has(m.id)) : undefined
      let box: Box
      if (from) {
        const p = placed.get(from.from)!
        box = { id: n.id, x: p.x + p.w + 80, y: p.y, w, h: NODE_H }
      } else if (to) {
        const q = placed.get(to.to)!
        box = { id: n.id, x: q.x - w - 80, y: q.y, w, h: NODE_H }
      } else if (mate) {
        const m = placed.get(mate.id)!
        box = { id: n.id, x: m.x, y: m.y + m.h + NODE_GAP, w, h: NODE_H }
      } else {
        const x0 = all.length ? Math.min(...all.map((b) => b.x)) : 0
        const y1 = all.length ? Math.max(...all.map((b) => b.y + b.h)) : 0
        box = { id: n.id, x: x0, y: all.length ? y1 + NODE_GAP : 0, w, h: NODE_H }
      }
      // 这个位置被占了，就上下交替一格格找空位；流程图整体的外框也不能因此压到别的区域
      const step = NODE_H + NODE_GAP
      const free = (b: Box, strict: boolean) => {
        if (all.some((p) => overlaps(b, p, 24))) return false
        if (!strict || !all.length) return true
        const hull = {
          id: '',
          x: Math.min(b.x, ...all.map((p) => p.x)),
          y: Math.min(b.y, ...all.map((p) => p.y)),
          w: 0,
          h: 0,
        }
        hull.w = Math.max(b.x + b.w, ...all.map((p) => p.x + p.w)) - hull.x
        hull.h = Math.max(b.y + b.h, ...all.map((p) => p.y + p.h)) - hull.y
        return !others.some((r) => overlaps(hull, r, 0))
      }
      const tries = [0, ...Array.from({ length: 30 }, (_, i) => [-(i + 1), i + 1]).flat()]
      const y0 = box.y
      const hit = tries.find((k) => free({ ...box, y: y0 + k * step }, true)) ?? tries.find((k) => free({ ...box, y: y0 + k * step }, false))
      box.y = y0 + (hit ?? 0) * step
      placed.set(n.id, box)
    }
    this.nodeAt = placed

    const groups: (Box & { label: string })[] = []
    for (const grp of g.groups) {
      const members = [...placed.values()].filter((b) => g.nodes.find((n) => n.id === b.id)?.group === grp.id)
      if (!members.length) continue
      const x = Math.min(...members.map((b) => b.x)) - 24
      const y = Math.min(...members.map((b) => b.y)) - 44
      const x1 = Math.max(...members.map((b) => b.x + b.w)) + 24
      const y1 = Math.max(...members.map((b) => b.y + b.h)) + 24
      groups.push({ id: grp.id, x, y, w: x1 - x, h: y1 - y, label: grp.label })
    }

    const edges: Route[] = inner.map((e) => {
      const key = `${e.from}>${e.to}`
      const s = sig(e, placed)
      const old = this.routeAt.get(key)
      const others = [...placed.values()].filter((b) => b.id !== e.from && b.id !== e.to)
      // 沿用旧走线的前提：两端没动，而且没有新节点压在线上
      const keep = old && old.sig === s && !crosses(old.points, others)
      const points = keep ? old.points : route(placed.get(e.from)!, placed.get(e.to)!, others)
      this.routeAt.set(key, { sig: s, points })
      return { from: e.from, to: e.to, points }
    })

    const boxes = [...placed.values(), ...groups]
    const pts = edges.flatMap((e) => e.points)
    const x0 = Math.min(...boxes.map((b) => b.x), ...pts.map((p) => p[0]))
    const y0 = Math.min(...boxes.map((b) => b.y), ...pts.map((p) => p[1]))
    const x1 = Math.max(...boxes.map((b) => b.x + b.w), ...pts.map((p) => p[0]))
    const y1 = Math.max(...boxes.map((b) => b.y + b.h), ...pts.map((p) => p[1]))
    return { x0, y0, w: x1 - x0, h: y1 - y0, nodes: [...placed.values()], groups, edges }
  }

  private async compute(g: Graph): Promise<Layout> {
    const order = new Map(g.nodes.map((n, i) => [n.id, i]))
    const diagramIds = new Set(g.nodes.filter((n) => isDiagram(g, n)).map((n) => n.id))
    const loose = g.nodes.filter(
      (n) =>
        isAttach(n) &&
        (!n.parent || !g.nodes.some((p) => p.id === n.parent) || diagramIds.has(n.parent)) &&
        n.kind !== 'note',
    )
    // 挂在流程图节点上的备注也收进待定，写成「节点：备注」
    for (const n of g.nodes) if (n.kind === 'note' && n.parent && diagramIds.has(n.parent)) loose.push(n)

    // 每块区域：key、大小，以及内部坐标的左上角（流程图增量排时可能是负的）
    type Slot = { key: string; at: number; region?: Region; x0: number; y0: number; w: number; h: number }
    const slots: Slot[] = []
    for (const n of g.nodes)
      if (isHead(g, n)) {
        const r = topicRegion(g, n)
        slots.push({ key: r.id, at: order.get(n.id)!, region: r, x0: 0, y0: 0, w: r.w, h: r.h })
      }
    let dia: Diagram | undefined
    if (diagramIds.size) {
      dia = await this.diagram(g, diagramIds)
      const at = Math.min(...[...diagramIds].map((id) => order.get(id)!))
      slots.push({ key: '__diagram', at, x0: dia.x0, y0: dia.y0, w: dia.w, h: dia.h })
    } else this.nodeAt.clear()
    if (loose.length) {
      const r = looseRegion(g, loose)
      slots.push({ key: r.id, at: Math.min(...loose.map((n) => order.get(n.id)!)), region: r, x0: 0, y0: 0, w: r.w, h: r.h })
    }
    slots.sort((a, b) => a.at - b.at)
    const live = new Set(slots.map((s) => s.key))
    for (const k of [...this.regionAt.keys()]) if (!live.has(k)) this.regionAt.delete(k)
    for (const k of [...this.boxAt.keys()]) if (!live.has(k)) this.boxAt.delete(k)

    // 摆区域：记得位置的放回原处（被前面长大的区域压到就往下推），新的找一块空地
    const placed: Box[] = []
    const origin = new Map<string, { x: number; y: number }>()
    for (const s of slots) {
      const was = this.regionAt.get(s.key)
      let box: Box
      if (was) {
        box = { id: s.key, x: was.x + s.x0, y: was.y + s.y0, w: s.w, h: s.h }
        for (let i = 0; i < 60; i++) {
          const hit = placed.filter((b) => overlaps(box, b, GAP / 2 - 1))
          if (!hit.length) break
          box.y = Math.max(...hit.map((b) => b.y + b.h)) + GAP
        }
      } else box = this.freeSpot(s, placed)
      placed.push(box)
      this.boxAt.set(s.key, box)
      const at = { x: box.x - s.x0, y: box.y - s.y0 }
      this.regionAt.set(s.key, at)
      origin.set(s.key, at)
    }

    const regions: Region[] = []
    let diagram: Layout['diagram']
    const anchors = new Map<string, Box>()
    for (const s of slots) {
      const { x: px, y: py } = origin.get(s.key)!
      if (s.region) {
        const r = { ...s.region, x: px, y: py }
        regions.push(r)
        anchors.set(r.id, { id: r.id, x: px, y: py, w: r.w, h: r.h })
        for (const row of r.rows)
          anchors.set(row.id, { id: row.id, x: px + row.x, y: py + row.y, w: row.w + (row.prefix ? 30 : 0), h: row.size + 8 })
      } else if (dia) {
        const shift = (b: Box): Box => ({ ...b, x: b.x + px, y: b.y + py })
        diagram = {
          box: { id: '__diagram', x: px + dia.x0, y: py + dia.y0, w: dia.w, h: dia.h },
          nodes: dia.nodes.map(shift),
          groups: dia.groups.map((b) => ({ ...shift(b), label: b.label })),
          edges: dia.edges.map((e) => ({ ...e, points: e.points.map(([a, b]) => [a + px, b + py] as [number, number]) })),
        }
        for (const b of diagram.nodes) anchors.set(b.id, b)
      }
    }

    const inDiagram = (e: { from: string; to: string }) => diagramIds.has(e.from) && diagramIds.has(e.to)
    const links = g.edges.filter((e) => !inDiagram(e) && anchors.has(e.from) && anchors.has(e.to))

    const all = [...anchors.values(), ...(diagram ? [diagram.box] : [])]
    const bounds = all.length
      ? {
          x0: Math.min(...all.map((b) => b.x)),
          y0: Math.min(...all.map((b) => b.y)),
          x1: Math.max(...all.map((b) => b.x + b.w)),
          y1: Math.max(...all.map((b) => b.y + b.h)),
        }
      : { x0: 0, y0: 0, x1: 1, y1: 1 }
    return { regions, diagram, links, anchors, bounds }
  }

  /** 新区域的位置：挨着已有区域的右边或下面，挑一个不压别人、整块板最接近横向长方形的 */
  private freeSpot(s: { key: string; w: number; h: number }, placed: Box[]): Box {
    const jx = jitter(s.key, 14)
    const jy = jitter(`${s.key}y`, 10)
    if (!placed.length) return { id: s.key, x: jx, y: jy, w: s.w, h: s.h }
    const cands: Box[] = []
    for (const p of placed) {
      cands.push({ id: s.key, x: p.x + p.w + GAP + jx, y: p.y + jy, w: s.w, h: s.h })
      cands.push({ id: s.key, x: p.x + jx, y: p.y + p.h + GAP + jy, w: s.w, h: s.h })
    }
    const x0 = Math.min(...placed.map((b) => b.x))
    const y0 = Math.min(...placed.map((b) => b.y))
    const x1 = Math.max(...placed.map((b) => b.x + b.w))
    const y1 = Math.max(...placed.map((b) => b.y + b.h))
    let best: Box | undefined
    let bestCost = Infinity
    for (const c of cands) {
      if (placed.some((b) => overlaps(c, b, GAP / 2 - 1))) continue
      const w = Math.max(x1, c.x + c.w) - Math.min(x0, c.x)
      const h = Math.max(y1, c.y + c.h) - Math.min(y0, c.y)
      // 板子偏横（16:10 左右）最好看；同样大小时优先上面、左边
      const cost = Math.max(w / 1.6, h) + c.y * 0.01 + c.x * 0.001
      if (cost < bestCost) {
        bestCost = cost
        best = c
      }
    }
    return best ?? { id: s.key, x: x0 + jx, y: y1 + GAP + jy, w: s.w, h: s.h }
  }
}

/** 不带记忆的一次排版（评测、测试用） */
export function layout(g: Graph): Promise<Layout> {
  return new StableLayout().layout(g)
}

/** 白板上用的那一份：铅笔和手写两种画法共用，位置保持一致 */
export const board = new StableLayout()
