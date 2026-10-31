// 导入 .excalidraw：它本来就是结构化的，不用看图——
// 带字的框 / 圆 / 菱形 → 流程图节点；两头都绑在东西上的箭头 → 连线（箭头上的字是标签）；
// frame → 分组；没绑在任何东西上的散字 → 一个「白板上的文字」话题下的条目。
// 转成板书操作再走一遍 applyOps，编号、去重、校验都和模型画的一样。

import { type Graph, emptyGraph } from '../engine/graph.ts'
import { applyOps, parseOps } from '../engine/ops.ts'
import { msg } from '../i18n/index.ts'

interface El {
  id: string
  type: string
  isDeleted?: boolean
  x?: number
  y?: number
  text?: string
  originalText?: string
  containerId?: string | null
  frameId?: string | null
  name?: string | null
  startBinding?: { elementId: string } | null
  endBinding?: { elementId: string } | null
}

export interface Imported {
  graph: Graph
  nodes: number
  edges: number
  groups: number
  notes: number
}

const SHAPES = new Set(['rectangle', 'ellipse', 'diamond'])

/** 操作里的名字不能带「」，换行也压成一行 */
const clean = (s: string) =>
  s
    .replace(/[「」]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 60)

export function importExcalidraw(json: string): Imported {
  let data: { type?: string; elements?: El[] }
  try {
    data = JSON.parse(json)
  } catch {
    throw new Error(msg().errors.notJson)
  }
  if (!Array.isArray(data.elements)) throw new Error(msg().errors.noElements)
  const els = data.elements.filter((e) => e && !e.isDeleted)
  const textOf = (e: El) => clean(e.originalText ?? e.text ?? '')
  // 绑在框或箭头上的字
  const bound = new Map<string, string>()
  for (const e of els) if (e.type === 'text' && e.containerId) bound.set(e.containerId, textOf(e))

  // 箭头：两头都绑着东西才算连线
  const arrows = els.filter(
    (e) => (e.type === 'arrow' || e.type === 'line') && e.startBinding?.elementId && e.endBinding?.elementId,
  )
  const linked = new Set(arrows.flatMap((a) => [a.startBinding!.elementId, a.endBinding!.elementId]))

  // 节点：带字的形状、连着箭头的形状，和被箭头连着的散字；从上到下、从左到右排，编号稳定
  const pos = (e: El) => (e.y ?? 0) * 10000 + (e.x ?? 0)
  const nodeEls = els
    .filter((e) => (SHAPES.has(e.type) && (bound.has(e.id) || linked.has(e.id))) || (e.type === 'text' && !e.containerId && linked.has(e.id)))
    .sort((a, b) => pos(a) - pos(b))
  const label = new Map<string, string>()
  const used = new Map<string, number>()
  let blank = 0
  for (const e of nodeEls) {
    let name = e.type === 'text' ? textOf(e) : (bound.get(e.id) ?? '')
    if (!name) name = msg().errors.blankBox(++blank)
    // 同名的两个框不能合成一个
    const n = (used.get(name) ?? 0) + 1
    used.set(name, n)
    label.set(e.id, n > 1 ? `${name}（${n}）` : name)
  }

  const ops: string[] = []
  for (const e of nodeEls) ops.push(`N 「${label.get(e.id)}」`)
  let edges = 0
  for (const a of arrows) {
    const from = label.get(a.startBinding!.elementId)
    const to = label.get(a.endBinding!.elementId)
    if (!from || !to || from === to) continue
    const text = bound.get(a.id)
    ops.push(`E 「${from}」 -> 「${to}」${text ? ` : 「${text}」` : ''}`)
    edges++
  }
  // frame → 分组
  let groups = 0
  for (const f of els.filter((e) => e.type === 'frame' || e.type === 'magicframe')) {
    const members = nodeEls.filter((e) => e.frameId === f.id).map((e) => `「${label.get(e.id)}」`)
    if (!members.length) continue
    ops.push(`G 「${clean(f.name ?? '') || msg().errors.group(groups + 1)}」 ${members.join(' ')}`)
    groups++
  }
  // 散字（标题、便签之类）
  const loose = els
    .filter((e) => e.type === 'text' && !e.containerId && !linked.has(e.id) && textOf(e))
    .sort((a, b) => pos(a) - pos(b))
  if (loose.length) {
    ops.push(`T 「${msg().errors.looseTopic}」`)
    for (const t of loose) ops.push(`L 「${msg().errors.looseTopic}」 「${textOf(t)}」`)
  }
  if (!nodeEls.length && !loose.length) throw new Error(msg().errors.nothingToImport)

  return { graph: applyOps(emptyGraph(), parseOps(ops.join('\n')), 'ink'), nodes: nodeEls.length, edges, groups, notes: loose.length }
}
