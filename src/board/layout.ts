// 图 → 坐标。排版完全是确定性的，模型不参与。
// considerModelOrder 让节点按加入顺序排，新东西出现时旧的尽量不跳。

import ELK, { type ElkExtendedEdge, type ElkNode } from 'elkjs/lib/elk.bundled.js'
import type { Graph } from '../engine/graph.ts'

const elk = new ELK()

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

export interface Layout {
  nodes: Box[]
  groups: (Box & { label: string })[]
  edges: Route[]
}

/** 粗估文字宽度：中日韩字符按一个字宽，其余按半个。 */
function textWidth(s: string, size: number): number {
  let w = 0
  for (const ch of s) w += /[⺀-鿿＀-￯]/.test(ch) ? size : size * 0.6
  return w
}

export const FONT = 20

export async function layout(g: Graph): Promise<Layout> {
  const opts = {
    'elk.algorithm': 'layered',
    'elk.direction': 'RIGHT',
    'elk.spacing.nodeNode': '40',
    'elk.layered.spacing.nodeNodeBetweenLayers': '80',
    'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
    'elk.edgeRouting': 'POLYLINE',
  }
  const leaf = (n: Graph['nodes'][number]): ElkNode => ({
    id: n.id,
    width: Math.max(110, textWidth(n.label, FONT) + 44),
    height: 64,
  })
  const children: ElkNode[] = []
  for (const grp of g.groups) {
    children.push({
      id: grp.id,
      layoutOptions: { 'elk.padding': '[top=44,left=24,bottom=24,right=24]' },
      children: g.nodes.filter((n) => n.group === grp.id).map(leaf),
    })
  }
  for (const n of g.nodes) if (!n.group || !g.groups.some((x) => x.id === n.group)) children.push(leaf(n))
  const edges: ElkExtendedEdge[] = g.edges.map((e, i) => ({ id: `e${i}`, sources: [e.from], targets: [e.to] }))

  const out = await elk.layout({ id: 'root', layoutOptions: opts, children, edges })

  const nodes: Box[] = []
  const groups: Layout['groups'] = []
  const walk = (n: ElkNode, ox: number, oy: number) => {
    for (const c of n.children ?? []) {
      const x = ox + (c.x ?? 0)
      const y = oy + (c.y ?? 0)
      const box = { id: c.id, x, y, w: c.width ?? 0, h: c.height ?? 0 }
      const grp = g.groups.find((x) => x.id === c.id)
      if (grp) {
        groups.push({ ...box, label: grp.label })
        walk(c, x, y)
      } else {
        nodes.push(box)
      }
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
      const i = Number(e.id.slice(1))
      const src = g.edges[i]
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
  return { nodes, groups, edges: routes }
}
