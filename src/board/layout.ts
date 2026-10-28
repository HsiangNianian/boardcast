// 板书 → 坐标。排版完全是确定性的，模型不参与。
//
// 一块板由若干「区域」拼起来：
// - 每个话题一块：标题 + 编号条目 + 挂在条目旁的问题、公式、备注；
// - 所有流程图节点合成一块，内部用 ELK 分层排版；
// - 没有挂靠的问题、公式放进「待定」一块。
// 区域按第一次出现的先后摆成几行，带一点错位，看起来更像随手写的。

import ELK, { type ElkExtendedEdge, type ElkNode } from 'elkjs/lib/elk.bundled.js'
import { type Graph, type Node, children } from '../engine/graph.ts'

const elk = new ELK()

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

function isHead(g: Graph, n: Node): boolean {
  if (n.parent && g.nodes.some((p) => p.id === n.parent)) return false
  if (n.kind === 'topic') return true
  return n.kind === 'node' && children(g, n.id).some((c) => c.kind === 'item')
}

function isDiagram(g: Graph, n: Node): boolean {
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

async function diagramLayout(g: Graph, ids: Set<string>) {
  const leaf = (n: Node): ElkNode => ({
    id: n.id,
    width: Math.max(110, textWidth(n.label, FONT) + 44 + (n.icon ? 40 : 0)),
    height: 64,
  })
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
  const out = await elk.layout({
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
  return { w: root.width ?? 0, h: root.height ?? 0, nodes: boxes, groups, edges: routes }
}

export async function layout(g: Graph): Promise<Layout> {
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

  type Slot = { at: number; region?: Region; diagram?: boolean; w: number; h: number }
  const slots: Slot[] = []
  for (const n of g.nodes) if (isHead(g, n)) {
    const r = topicRegion(g, n)
    slots.push({ at: order.get(n.id)!, region: r, w: r.w, h: r.h })
  }
  let dia: Awaited<ReturnType<typeof diagramLayout>> | undefined
  if (diagramIds.size) {
    dia = await diagramLayout(g, diagramIds)
    slots.push({ at: Math.min(...[...diagramIds].map((id) => order.get(id)!)), diagram: true, w: dia.w, h: dia.h })
  }
  if (loose.length) {
    const r = looseRegion(g, loose)
    slots.push({ at: Math.min(...loose.map((n) => order.get(n.id)!)), region: r, w: r.w, h: r.h })
  }
  slots.sort((a, b) => a.at - b.at)

  // 简单的按行装箱：一行放满就换行，行宽按总面积取一个偏横的比例
  const GAP = 90
  const area = slots.reduce((s, x) => s + (x.w + GAP) * (x.h + GAP), 0)
  const maxW = Math.max(900, Math.sqrt(area * 1.7), ...slots.map((s) => s.w))
  let x = 0
  let y = 0
  let rowH = 0
  const regions: Region[] = []
  let diagram: Layout['diagram']
  const anchors = new Map<string, Box>()
  for (const s of slots) {
    if (x > 0 && x + s.w > maxW) {
      x = 0
      y += rowH + GAP
      rowH = 0
    }
    const key = s.region?.id ?? '__diagram'
    const px = x + jitter(key, 14)
    const py = y + jitter(`${key}y`, 10)
    if (s.region) {
      const r = { ...s.region, x: px, y: py }
      regions.push(r)
      anchors.set(r.id, { id: r.id, x: px, y: py, w: r.w, h: r.h })
      for (const row of r.rows)
        anchors.set(row.id, { id: row.id, x: px + row.x, y: py + row.y, w: row.w + (row.prefix ? 30 : 0), h: row.size + 8 })
    } else if (dia) {
      const shift = (b: Box): Box => ({ ...b, x: b.x + px, y: b.y + py })
      diagram = {
        box: { id: '__diagram', x: px, y: py, w: dia.w, h: dia.h },
        nodes: dia.nodes.map(shift),
        groups: dia.groups.map((b) => ({ ...shift(b), label: b.label })),
        edges: dia.edges.map((e) => ({ ...e, points: e.points.map(([a, b]) => [a + px, b + py] as [number, number]) })),
      }
      for (const b of diagram.nodes) anchors.set(b.id, b)
    }
    x += s.w + GAP
    rowH = Math.max(rowH, s.h)
  }

  const inDiagram = (e: { from: string; to: string }) => diagramIds.has(e.from) && diagramIds.has(e.to)
  const links = g.edges.filter((e) => !inDiagram(e) && anchors.has(e.from) && anchors.has(e.to))

  const all = [...anchors.values()]
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
