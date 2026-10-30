// 回放用的记录：墨线板书每变一次，记下和上一版的差异（新增或改了的节点 / 连线 / 分组、删掉的、编号计数），
// 加上时间和当时说到第几句。从空板开始依次打上这些差异，就能还原任意时刻的板书。
// 不记操作文本：铅笔稿会先占掉节点编号，只重放模型的操作，编号会对不上；手动修改、撤销也不是操作。

import { type Edge, type Graph, type Group, type Node, cloneGraph, emptyGraph } from './graph.ts'

interface Part<T> {
  /** 新增或者改了的（整条存） */
  set?: T[]
  /** 删掉的 */
  del?: string[]
  /** 顺序变了（不只是删掉一些、末尾加一些）时，完整的新顺序 */
  order?: string[]
}

export interface Diff {
  nodes?: Part<Node>
  edges?: Part<Edge>
  groups?: Part<Group>
  nextNode?: number
  nextGroup?: number
}

export interface Step {
  /** 什么时候（毫秒时间戳） */
  t: number
  /** 这时会议记录有几句（回放时显示到这一句） */
  line: number
  d: Diff
}

const edgeKey = (e: Edge) => `${e.from}>${e.to}`

function diffPart<T>(a: T[], b: T[], key: (x: T) => string): Part<T> | undefined {
  const before = new Map(a.map((x) => [key(x), JSON.stringify(x)]))
  const after = new Set(b.map(key))
  const set = b.filter((x) => before.get(key(x)) !== JSON.stringify(x))
  const del = [...before.keys()].filter((k) => !after.has(k))
  // 按「删掉的拿走、新增的接在末尾」能不能得到新顺序；不能就把顺序记下来
  const kept = a.map(key).filter((k) => after.has(k))
  const added = b.map(key).filter((k) => !before.has(k))
  const guess = [...kept, ...added]
  const real = b.map(key)
  const order = guess.join('\n') === real.join('\n') ? undefined : real
  if (!set.length && !del.length && !order) return undefined
  return { ...(set.length ? { set } : {}), ...(del.length ? { del } : {}), ...(order ? { order } : {}) }
}

function patchPart<T>(a: T[], p: Part<T> | undefined, key: (x: T) => string): T[] {
  if (!p) return a
  const del = new Set(p.del)
  const byKey = new Map(a.filter((x) => !del.has(key(x))).map((x) => [key(x), x]))
  const order = a.map(key).filter((k) => byKey.has(k))
  for (const x of p.set ?? []) {
    if (!byKey.has(key(x))) order.push(key(x))
    byKey.set(key(x), structuredClone(x))
  }
  return (p.order ?? order).flatMap((k) => (byKey.has(k) ? [byKey.get(k)!] : []))
}

/** 两版板书的差异；一样就返回 null */
export function diffGraph(a: Graph, b: Graph): Diff | null {
  const d: Diff = {}
  const nodes = diffPart(a.nodes, b.nodes, (n) => n.id)
  const edges = diffPart(a.edges, b.edges, edgeKey)
  const groups = diffPart(a.groups, b.groups, (x) => x.id)
  if (nodes) d.nodes = nodes
  if (edges) d.edges = edges
  if (groups) d.groups = groups
  if (a.nextNode !== b.nextNode) d.nextNode = b.nextNode
  if (a.nextGroup !== b.nextGroup) d.nextGroup = b.nextGroup
  return Object.keys(d).length ? d : null
}

export function patchGraph(g: Graph, d: Diff): Graph {
  const out = cloneGraph(g)
  out.nodes = patchPart(out.nodes, d.nodes, (n) => n.id)
  out.edges = patchPart(out.edges, d.edges, edgeKey)
  out.groups = patchPart(out.groups, d.groups, (x) => x.id)
  if (d.nextNode !== undefined) out.nextNode = d.nextNode
  if (d.nextGroup !== undefined) out.nextGroup = d.nextGroup
  return out
}

/** 只留墨线：回放只放确定下来的板书，铅笔稿是一闪而过的 */
export function inkOnly(g: Graph): Graph {
  const out = cloneGraph(g)
  out.nodes = out.nodes.filter((n) => n.ink !== 'draft')
  const alive = new Set(out.nodes.map((n) => n.id))
  out.edges = out.edges.filter((e) => e.ink !== 'draft' && alive.has(e.from) && alive.has(e.to))
  return out
}

/** 前 `count` 步打完以后的板书 */
export function replayTo(steps: Step[], count: number): Graph {
  let g = emptyGraph()
  for (const s of steps.slice(0, count)) g = patchGraph(g, s.d)
  return g
}

/** 把所有步打完，得到的板书和 `g` 一致吗（用来判断这场会能不能完整回放） */
export function replays(steps: Step[], g: Graph): boolean {
  return diffGraph(replayTo(steps, steps.length), inkOnly(g)) === null
}
