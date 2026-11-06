// 导入 .excalidraw：它本来就是结构化的，不用看图——
// 带字的框 / 圆 / 菱形 → 流程图节点；两头都绑在东西上的箭头 → 连线（箭头上的字是标签）；
// frame → 分组；没绑在任何东西上的散字 → 一个「白板上的文字」话题下的条目。
// 转成板书操作再走一遍 applyOps，编号、去重、校验都和模型画的一样。

import { type Graph, emptyGraph, norm } from '../engine/graph.ts'
import { applyOps, parseOps } from '../engine/ops.ts'
import { msg } from '../i18n/index.ts'
import { type FreeScene, type SceneOwner, imageSvg, validateScene, withoutPageUnderlay } from './freeScene.ts'
import type { LayoutMemory } from './layout.ts'
import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types'

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
  customData?: Record<string, unknown>
}

export interface Imported {
  graph: Graph
  nodes: number
  edges: number
  groups: number
  notes: number
  scene: FreeScene
  layout?: LayoutMemory
  underlay?: string
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
  if (json.length > 80 * 1024 * 1024) throw new Error(msg().errors.drawingTooLarge)
  let data: { type?: string; elements?: El[]; files?: FreeScene['files']; boardcast?: { version: number; graph: Graph; layout?: LayoutMemory; underlay?: string; generated: FreeScene['generated']; overrides?: FreeScene['overrides']; hiddenGenerated?: string[]; modelCandidates?: FreeScene['modelCandidates'] } }
  try {
    data = JSON.parse(json)
  } catch {
    throw new Error(msg().errors.notJson)
  }
  if (!Array.isArray(data.elements)) throw new Error(msg().errors.noElements)
  if (data.boardcast) {
    const meta = data.boardcast
    if (meta.version !== 2) throw new Error(msg().errors.drawingNewer)
    if (!meta.graph || !Array.isArray(meta.graph.nodes) || !Array.isArray(meta.graph.edges) || !Array.isArray(meta.graph.groups) || !Number.isFinite(meta.graph.nextNode) || !Number.isFinite(meta.graph.nextGroup)) throw new Error(msg().errors.invalidDrawingGraph)
    if (meta.underlay && (!meta.graph.underlay || !imageSvg(meta.underlay, 0, 0, meta.graph.underlay.w, meta.graph.underlay.h))) throw new Error(msg().errors.invalidDrawingBackground)
    const drawing = data.elements as ExcalidrawElement[]
    const state = withoutPageUnderlay({ elements: drawing, generated: meta.generated, files: data.files ?? {}, overrides: meta.overrides, hiddenGenerated: meta.hiddenGenerated, modelCandidates: meta.modelCandidates })
    validateScene(state)
    return { graph: meta.graph, layout: meta.layout, underlay: meta.underlay, scene: state, nodes: meta.graph.nodes.filter((n) => n.kind === 'node').length, edges: meta.graph.edges.length, groups: meta.graph.groups.length, notes: meta.graph.nodes.filter((n) => n.kind === 'item').length }
  }
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
  const used = new Set<string>()
  const unique = (name: string) => {
    let candidate = name, n = 2
    while (used.has(norm(candidate))) candidate = `${name}（${n++}）`
    used.add(norm(candidate))
    return candidate
  }
  let blank = 0
  for (const e of nodeEls) {
    let name = e.type === 'text' ? textOf(e) : (bound.get(e.id) ?? '')
    if (!name) name = msg().errors.blankBox(++blank)
    // 同名的两个框不能合成一个
    label.set(e.id, unique(name))
  }

  const ops: string[] = []
  for (const e of nodeEls) ops.push(`N 「${label.get(e.id)}」`)
  let edges = 0
  const edgePairs = new Set<string>()
  const semanticArrows: El[] = []
  for (const a of arrows) {
    const from = label.get(a.startBinding!.elementId)
    const to = label.get(a.endBinding!.elementId)
    if (!from || !to || from === to) continue
    const pair = JSON.stringify([from, to])
    // The graph has one edge per pair; parallel paths stay editable free objects.
    if (edgePairs.has(pair)) continue
    edgePairs.add(pair)
    semanticArrows.push(a)
    const text = bound.get(a.id)
    ops.push(`E 「${from}」 -> 「${to}」${text ? ` : 「${text}」` : ''}`)
    edges++
  }
  // frame → 分组
  let groups = 0
  for (const f of els.filter((e) => e.type === 'frame' || e.type === 'magicframe')) {
    const members = nodeEls.filter((e) => e.frameId === f.id).map((e) => `「${label.get(e.id)}」`)
    if (!members.length) continue
    label.set(f.id, unique(clean(f.name ?? '') || msg().errors.group(groups + 1)))
    ops.push(`G 「${label.get(f.id)}」 ${members.join(' ')}`)
    groups++
  }
  // 散字（标题、便签之类）
  const loose = els
    .filter((e) => e.type === 'text' && !e.containerId && !linked.has(e.id) && textOf(e))
    .sort((a, b) => pos(a) - pos(b))
  if (loose.length) {
    const topic = unique(msg().errors.looseTopic)
    ops.push(`T 「${topic}」`)
    for (const t of loose) {
      label.set(t.id, unique(textOf(t)))
      ops.push(`L 「${topic}」 「${label.get(t.id)}」`)
    }
  }
  if (!els.length) throw new Error(msg().errors.nothingToImport)
  const graph = applyOps(emptyGraph(), parseOps(ops.join('\n')), 'ink')
  const ids = new Map<string, string>()
  const owners = new Map<string, Omit<SceneOwner, 'role'>>()
  for (const e of nodeEls) {
    const n = graph.nodes.find((n) => n.label === label.get(e.id))
    if (!n) continue
    ids.set(e.id, e.type === 'text' ? `${n.id}:label` : n.id)
    owners.set(e.id, { kind: 'node', id: n.id })
  }
  for (const e of loose) {
    const n = graph.nodes.find((n) => n.kind === 'item' && n.label === label.get(e.id))
    if (n) { ids.set(e.id, `${n.id}:t`); owners.set(e.id, { kind: 'node', id: n.id }) }
  }
  for (const a of semanticArrows) {
    const from = owners.get(a.startBinding!.elementId)?.id, to = owners.get(a.endBinding!.elementId)?.id
    if (from && to) { ids.set(a.id, `${from}>${to}`); owners.set(a.id, { kind: 'edge', id: `${from}>${to}`, from, to }) }
  }
  for (const f of els.filter((e) => e.type === 'frame' || e.type === 'magicframe')) {
    const group = graph.groups.find((g) => g.label === label.get(f.id))
    if (group) { ids.set(f.id, group.id); owners.set(f.id, { kind: 'group', id: group.id }) }
  }
  for (const e of els) if (e.type === 'text' && e.containerId && owners.has(e.containerId)) {
    ids.set(e.id, `${ids.get(e.containerId)}:label`)
    owners.set(e.id, owners.get(e.containerId)!)
  }
  const id = (old: string) => ids.get(old) ?? `import:${old}`
  const elements = els.map((e) => {
    const raw = e as unknown as Record<string, unknown>
    const owner = owners.get(e.id)
    return {
      x: 0, y: 0, width: 100, height: 40, angle: 0, strokeColor: '#1b1b1f', backgroundColor: 'transparent', fillStyle: 'solid', strokeWidth: 2, strokeStyle: 'solid', roughness: 1, opacity: 100, roundness: null, index: null, groupIds: [], seed: 1, version: 1, versionNonce: 1, updated: 0, isDeleted: false, boundElements: null, link: null, locked: false,
      ...(e.type === 'text' ? { fontSize: 20, fontFamily: 5, textAlign: 'left', verticalAlign: 'top', lineHeight: 1.25, autoResize: true, text: e.text ?? e.originalText ?? '', originalText: e.originalText ?? e.text ?? '' } : {}),
      ...raw, id: id(e.id),
      ...(e.containerId ? { containerId: id(e.containerId) } : {}),
      ...(e.frameId ? { frameId: id(e.frameId) } : {}),
      ...(e.startBinding ? { startBinding: { ...e.startBinding, elementId: id(e.startBinding.elementId) } } : {}),
      ...(e.endBinding ? { endBinding: { ...e.endBinding, elementId: id(e.endBinding.elementId) } } : {}),
      ...(Array.isArray(raw.boundElements) ? { boundElements: raw.boundElements.map((b: { id: string }) => ({ ...b, id: id(b.id) })) } : {}),
      customData: { ...e.customData, boardcast: owner ? { ...owner, role: e.type === 'text' ? 'label' : 'body' } : undefined },
    } as unknown as ExcalidrawElement
  })
  const state: FreeScene = { elements, generated: [], files: data.files ?? {}, pendingImport: true }
  validateScene(state)
  return { graph, scene: state, nodes: nodeEls.length, edges, groups, notes: loose.length }
}
