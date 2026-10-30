// 手动修改板书：在手写板上点选以后的删除、改字、标记。都返回新图，不改原图。
// 能用现成操作表达的（删节点、改名、删连线）就走 applyOps，和模型改板书是同一套规则。

import { type Graph, type Mark, cloneGraph } from './graph.ts'
import { applyOps } from './ops.ts'

/** 手写板上选中的东西 */
export type Selection = { kind: 'node'; id: string } | { kind: 'edge'; from: string; to: string }

/** 删掉节点（话题连同挂在下面的条目一起删）或者连线 */
export function remove(g: Graph, sel: Selection): Graph {
  if (sel.kind === 'node') return applyOps(g, [{ op: 'D', target: { id: sel.id } }])
  return applyOps(g, [{ op: 'X', from: { id: sel.from }, to: { id: sel.to } }])
}

/** 改字：节点改名（旧名字留作别名，模型还认得它），连线改标签（可以清空） */
export function rename(g: Graph, sel: Selection, text: string): Graph {
  const t = text.trim()
  if (sel.kind === 'node') return t ? applyOps(g, [{ op: 'R', target: { id: sel.id }, name: t }]) : g
  const out = cloneGraph(g)
  const e = out.edges.find((x) => x.from === sel.from && x.to === sel.to)
  if (e) e.label = t || undefined
  return out
}

/** 标记：没有就加上，已经有了就去掉 */
export function toggleMark(g: Graph, id: string, mark: Mark): Graph {
  const out = cloneGraph(g)
  const n = out.nodes.find((x) => x.id === id)
  if (!n) return g
  n.marks = n.marks.includes(mark) ? n.marks.filter((m) => m !== mark) : [...n.marks, mark]
  return out
}

/** 选中的东西的名字，用来写进会议记录 */
export function describe(g: Graph, sel: Selection): string {
  const name = (id: string) => g.nodes.find((n) => n.id === id)?.label ?? id
  if (sel.kind === 'node') return `「${name(sel.id)}」`
  const e = g.edges.find((x) => x.from === sel.from && x.to === sel.to)
  return `「${name(sel.from)} → ${name(sel.to)}」${e?.label ? `（${e.label}）` : ''}`
}

/** 选中的东西还在不在（撤销、模型改板书以后可能已经没了） */
export function exists(g: Graph, sel: Selection): boolean {
  if (sel.kind === 'node') return g.nodes.some((n) => n.id === sel.id)
  return g.edges.some((e) => e.from === sel.from && e.to === sel.to)
}

/** 撤销 / 重做：按「一步」记整张图。模型每画完一轮、每次手动修改各算一步 */
export class History {
  private past: Graph[] = []
  private future: Graph[] = []
  private limit: number
  constructor(limit = 100) {
    this.limit = limit
  }

  /** 记下改之前的样子；新的修改会清掉能重做的步骤 */
  record(before: Graph) {
    this.past.push(before)
    if (this.past.length > this.limit) this.past.shift()
    this.future = []
  }
  undo(current: Graph): Graph | null {
    const g = this.past.pop()
    if (!g) return null
    this.future.push(current)
    return g
  }
  redo(current: Graph): Graph | null {
    const g = this.future.pop()
    if (!g) return null
    this.past.push(current)
    return g
  }
  clear() {
    this.past = []
    this.future = []
  }
  get canUndo() {
    return this.past.length > 0
  }
  get canRedo() {
    return this.future.length > 0
  }
}
