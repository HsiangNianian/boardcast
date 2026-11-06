import { msg } from '../i18n/index.ts'
import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types'
import type { BinaryFiles } from '@excalidraw/excalidraw/types'
import { remove, rename, type Selection } from '../engine/edit.ts'
import { cloneGraph, type Edge, type Graph, type Group, type Node } from '../engine/graph.ts'
import { inherited, OLD_INK, oldTextOpacity } from '../engine/base.ts'

export interface SceneOwner {
  kind: 'node' | 'edge' | 'group'
  id: string
  from?: string
  to?: string
  role: 'body' | 'label' | 'decoration'
}

export interface SceneOverlay {
  url: string
  x: number
  y: number
  width: number
  height: number
  /** Generated ink replaced by user-styled objects in this overlay. */
  hidden: string[]
}

export type ModelCandidate =
  | { kind: 'node'; id: string; label?: string; restore?: Node }
  | { kind: 'edge'; id: string; from: string; to: string; label?: string | null; restore?: Edge }
  | { kind: 'group'; id: string; label?: string; restore?: Group }

/** The generated baseline makes field ownership explicit across model updates. */
export interface FreeScene {
  elements: readonly ExcalidrawElement[]
  generated: readonly ExcalidrawElement[]
  files: BinaryFiles
  overlay?: SceneOverlay
  /** Explicit ownership survives a semantic edit being reflected back into the graph. */
  overrides?: Record<string, Record<string, unknown>>
  /** External Excalidraw imports establish their graph projection on first layout. */
  pendingImport?: boolean
  hiddenGenerated?: string[]
  /** Latest actual proposal rejected by a user-owned field, not a revision log. */
  modelCandidates?: Record<string, ModelCandidate>
}

const transient = new Set(['version', 'versionNonce', 'updated', 'seed', 'index'])
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)
export const ownerOf = (e: ExcalidrawElement | undefined): SceneOwner | undefined => {
  const owner = e?.customData?.boardcast as SceneOwner | undefined
  return owner && ['node', 'edge', 'group'].includes(owner.kind) && typeof owner.id === 'string' && ['body', 'label', 'decoration'].includes(owner.role)
    && (owner.kind !== 'edge' || typeof owner.from === 'string' && typeof owner.to === 'string') ? owner : undefined
}

export const freeObjectName = (e: ExcalidrawElement): string =>
  typeof e.customData?.boardcastName === 'string' ? e.customData.boardcastName.slice(0, 120) : ''

export function renameSceneSpeaker(state: FreeScene | undefined, from: string, to: string): FreeScene | undefined {
  if (!state?.modelCandidates || !Object.values(state.modelCandidates).some((c) => c.kind !== 'group' && c.restore?.by === from)) return state
  const modelCandidates = structuredClone(state.modelCandidates)
  for (const c of Object.values(modelCandidates)) if (c.kind !== 'group' && c.restore?.by === from) c.restore.by = to
  return { ...state, modelCandidates }
}

/** Names describe free objects without changing their visible text or inventing image descriptions. */
export function editFreeObject(state: FreeScene, id: string, name: string | null): FreeScene {
  const target = state.elements.find((e) => e.id === id && !e.isDeleted && !ownerOf(e))
  if (!target || name !== null && freeObjectName(target) === name.trim().slice(0, 120)) return state
  const removed = new Set(name === null ? [id, ...state.elements.filter((e) => e.type === 'text' && e.containerId === id && !ownerOf(e)).map((e) => e.id)] : [])
  const elements = state.elements.map((e) => {
    if (name !== null) return e.id === id ? { ...e, customData: { ...e.customData, boardcastName: name.trim().slice(0, 120) }, version: e.version + 1 } : e
    if (removed.has(e.id)) return { ...e, isDeleted: true, version: e.version + 1 }
    const patch: Record<string, unknown> = {}
    if (e.frameId && removed.has(e.frameId)) patch.frameId = null
    if (e.boundElements?.some((b) => removed.has(b.id))) patch.boundElements = e.boundElements.filter((b) => !removed.has(b.id))
    if (e.type === 'arrow') {
      if (e.startBinding && removed.has(e.startBinding.elementId)) patch.startBinding = null
      if (e.endBinding && removed.has(e.endBinding.elementId)) patch.endBinding = null
    }
    return Object.keys(patch).length ? { ...e, ...patch, version: e.version + 1 } : e
  })
  return { ...state, elements }
}

export function validateScene(value: unknown): asserts value is FreeScene {
  const s = value as FreeScene | null
  if (!s || !Array.isArray(s.elements) || !Array.isArray(s.generated) || !s.files || typeof s.files !== 'object') throw new Error(msg().errors.invalidScene)
  const kinds = new Set(['text', 'rectangle', 'diamond', 'ellipse', 'line', 'arrow', 'freedraw', 'image', 'frame', 'magicframe', 'embeddable', 'iframe'])
  for (const list of [s.elements, s.generated]) {
    const ids = new Set<string>()
    for (const e of list) {
      if (!e || typeof e.id !== 'string' || ids.has(e.id) || !kinds.has(e.type) || ![e.x, e.y, e.width, e.height].every(Number.isFinite)) throw new Error(msg().errors.invalidElement)
      if (e.link && !/^(https?:|mailto:|tel:)/i.test(e.link)) throw new Error(msg().errors.unsafeBoardLink)
      ids.add(e.id)
    }
  }
  const safeImage = (url: unknown) => typeof url === 'string' && (/^asset:[a-f0-9]{16,64}\.(png|jpg|webp|gif|svg)$/i.test(url) || /^data:image\/(?:png|jpeg|gif|webp|svg\+xml);base64,[a-zA-Z0-9+/=\s]+$/.test(url))
  for (const file of Object.values(s.files)) if (!file || !safeImage(file.dataURL)) throw new Error(msg().errors.invalidBoardImage)
  if (s.overlay && (s.overlay.url && !safeImage(s.overlay.url) || !Array.isArray(s.overlay.hidden) || !s.overlay.hidden.every((id) => typeof id === 'string') || ![s.overlay.x, s.overlay.y, s.overlay.width, s.overlay.height].every(Number.isFinite))) throw new Error(msg().errors.invalidProjection)
  if (s.modelCandidates) {
    if (typeof s.modelCandidates !== 'object' || Array.isArray(s.modelCandidates)) throw new Error(msg().errors.invalidCandidates)
    for (const [id, c] of Object.entries(s.modelCandidates)) {
      if (!c || c.id !== id || !['node', 'edge', 'group'].includes(c.kind) || !('label' in c) && !c.restore) throw new Error(msg().errors.invalidCandidate)
      if ('label' in c && typeof c.label !== 'string' && !(c.kind === 'edge' && c.label === null)) throw new Error(msg().errors.invalidCandidateLabel)
      if (c.kind === 'edge' && (typeof c.from !== 'string' || typeof c.to !== 'string' || id !== `${c.from}>${c.to}`)) throw new Error(msg().errors.invalidCandidateEdge)
      if (c.restore) {
        if (c.kind === 'node' && (c.restore.id !== id || typeof c.restore.label !== 'string' || !Array.isArray(c.restore.aliases) || !c.restore.aliases.every((x) => typeof x === 'string') || !Array.isArray(c.restore.marks) || !['ink', 'draft'].includes(c.restore.ink) || !['node', 'topic', 'item', 'question', 'formula', 'note', 'region'].includes(c.restore.kind))) throw new Error(msg().errors.invalidRestoredNode)
        if (c.kind === 'edge' && (c.restore.from !== c.from || c.restore.to !== c.to || !['ink', 'draft'].includes(c.restore.ink))) throw new Error(msg().errors.invalidRestoredEdge)
        if (c.kind === 'group' && (c.restore.id !== id || typeof c.restore.label !== 'string')) throw new Error(msg().errors.invalidRestoredGroup)
      }
    }
  }
}

function overrides(before: ExcalidrawElement, current: ExcalidrawElement): Record<string, unknown> {
  const a = before as unknown as Record<string, unknown>
  return Object.fromEntries(Object.entries(current).filter(([key, value]) => !transient.has(key) && !same(value, a[key])))
}

/** Fading is a view projection: original user styles and opacity remain the saved truth. */
export function withInheritedInk(state: FreeScene, graph: Graph, base?: Graph, original?: FreeScene | null, editable = false): FreeScene {
  if (!base) return state
  const old = inherited(base)
  const generated = new Map(state.generated.map((e) => [e.id, e]))
  const originals = new Map(original?.elements.map((e) => [e.id, e]))
  const marks = { strike: '划掉', strike2: '划掉', underline: '下划线', circle: '圈出', star: '星标', highlight: '高亮' } as const
  return { ...state, elements: state.elements.map((e) => {
    const owner = ownerOf(e), projection = generated.get(e.id)
    let unchanged = false
    if (!projection || Object.keys(state.overrides?.[e.id] ?? {}).length || Object.keys(overrides(projection, e)).length) {
      const source = originals.get(e.id)
      unchanged = !!source && !Object.keys(overrides(source, e)).length
    } else if (owner?.kind === 'node') {
      const n = graph.nodes.find((n) => n.id === owner.id)
      if (n) {
        const span = n.spans?.find((s) => e.id.startsWith(`${n.id}:span:${s.mark}:${s.boxes.map((b) => `${b.x},${b.y}`).join(';')}:`))
        const mark = marks[e.id.split(':mark:')[1] as keyof typeof marks]
        unchanged = span ? old.span(n, span) : mark ? old.mark(n, mark) : e.id.includes(':icon:') ? old.icon(n) : owner.role === 'label' ? old.label(n) : old.node(n)
      }
    } else if (owner?.kind === 'edge') {
      const edge = graph.edges.find((x) => x.from === owner.from && x.to === owner.to)
      unchanged = !!edge && old.edge(edge, owner.role === 'label')
    } else if (owner?.kind === 'group') {
      const group = graph.groups.find((x) => x.id === owner.id)
      unchanged = !!group && old.group(group)
    }
    if (!unchanged) return e
    const opacity = Math.round(e.type === 'text' ? oldTextOpacity(e.strokeColor, '#fbfbf9', e.opacity / 100) * 100 : e.opacity * OLD_INK)
    return { ...e, opacity, ...(editable ? { customData: { ...e.customData, boardcastInheritedOpacity: [e.opacity, opacity] } } : {}) }
  }) }
}

/** Keep explicit opacity edits, but remove the display-only fade before merging a gesture. */
export function withoutInheritedInk(state: FreeScene): FreeScene {
  return { ...state, elements: state.elements.map((e) => {
    const value = e.customData?.boardcastInheritedOpacity
    if (!Array.isArray(value) || value.length !== 2 || !value.every((n) => typeof n === 'number' && Number.isFinite(n) && n >= 0 && n <= 100)) return e
    const customData: Record<string, unknown> = { ...e.customData }
    delete customData.boardcastInheritedOpacity
    return { ...e, opacity: e.opacity === value[1] ? value[0] : e.opacity, customData: Object.keys(customData).length ? customData : undefined }
  }) }
}

export function sceneChanged(a: FreeScene | undefined, b: FreeScene | undefined): boolean {
  const content = (s: FreeScene | undefined) => s && {
    elements: s.elements.map((e) => Object.fromEntries(Object.entries(e).filter(([k]) => !transient.has(k)))),
    files: s.files,
    modelCandidates: s.modelCandidates,
  }
  return !same(content(a), content(b))
}

/** Three-way merge: refreshed graph fields never overwrite user-owned fields. */
export function mergeScene(saved: FreeScene | undefined, generated: readonly ExcalidrawElement[]): FreeScene {
  if (!saved) return { elements: structuredClone(generated), generated: structuredClone(generated), files: {} }
  if (saved.pendingImport) {
    const present = new Set(saved.elements.map((e) => e.id))
    // Imported seeds determine the original drawing's rough strokes.
    const owned = Object.fromEntries(saved.elements.filter((e) => ownerOf(e)).map((e) => [e.id, Object.fromEntries(Object.entries(e).filter(([key]) => (key === 'seed' || !transient.has(key)) && key !== 'id' && key !== 'customData'))]))
    return mergeScene({ ...saved, overrides: owned, pendingImport: undefined, generated: structuredClone(generated), hiddenGenerated: generated.filter((e) => !present.has(e.id)).map((e) => e.id) }, generated)
  }
  const base = new Map(saved.generated.map((e) => [e.id, e]))
  const current = new Map(saved.elements.map((e) => [e.id, e]))
  const fresh = new Map(generated.map((e) => [e.id, e]))
  const owned: NonNullable<FreeScene['overrides']> = { ...saved.overrides }
  const elements: ExcalidrawElement[] = []
  // Keep the user's order, including free objects interleaved with generated ones.
  for (const e of saved.elements) {
    const old = base.get(e.id)
    const next = fresh.get(e.id)
    if (!old) elements.push(e)
    else if (next) {
      const patch = { ...owned[e.id], ...overrides(old, e) }
      for (const key of Object.keys(patch)) patch[key] = (e as unknown as Record<string, unknown>)[key]
      if (Object.keys(patch).length) owned[e.id] = patch
      elements.push({ ...next, ...patch, version: Math.max(e.version, next.version) + 1 } as ExcalidrawElement)
    }
    else if (Object.keys(overrides(old, e)).length && !e.isDeleted) {
      // A model deletion must not erase a user's customized object.
      const customData: Record<string, unknown> = { ...e.customData }
      delete customData.boardcast
      elements.push({ ...e, customData } as ExcalidrawElement)
    }
  }
  for (const e of generated) {
    if (current.has(e.id)) continue
    // Missing old elements are user deletions, not an invitation to recreate them.
    elements.push(base.has(e.id) ? { ...e, isDeleted: true } as ExcalidrawElement : e)
  }
  return { ...saved, elements, generated: structuredClone(generated), overrides: owned }
}

/** Keep the semantic model in agreement with fields explicitly owned by the user. */
export function protectGraphFields(graph: Graph, state: FreeScene | undefined): Graph {
  if (!state) return graph
  let next = graph
  for (const e of state.elements) {
    const owner = ownerOf(e)
    const patch = state.overrides?.[e.id]
    if (!owner || !patch || state.hiddenGenerated?.includes(e.id)) continue
    if (owner.kind === 'group') {
      if (patch.isDeleted && owner.role !== 'decoration') next = { ...next, groups: next.groups.filter((g) => g.id !== owner.id), nodes: next.nodes.map((n) => n.group === owner.id ? { ...n, group: undefined } : n) }
      else if (e.type === 'text' && ('text' in patch || 'originalText' in patch)) next = { ...next, groups: next.groups.map((g) => g.id === owner.id ? { ...g, label: e.originalText || e.text } : g) }
      continue
    }
    const selection: Selection = owner.kind === 'edge' ? { kind: 'edge', from: owner.from!, to: owner.to! } : { kind: 'node', id: owner.id }
    if (patch.isDeleted && owner.role !== 'decoration') next = remove(next, selection)
    else if (e.type === 'text' && ('text' in patch || 'originalText' in patch)) {
      const current = owner.kind === 'node' ? next.nodes.find((n) => n.id === owner.id) : next.edges.find((edge) => edge.from === owner.from && edge.to === owner.to)
      const text = e.originalText || e.text
      if (current && (current.label ?? '') !== text) next = rename(next, selection, text)
    }
  }
  return next
}

/** Call only for a completed model result, before protectGraphFields discards conflicts. */
export function captureModelCandidates(graph: Graph, state: FreeScene | undefined): FreeScene | undefined {
  if (!state) return state
  const candidates = { ...state.modelCandidates }
  for (const e of state.elements) {
    const owner = ownerOf(e), patch = state.overrides?.[e.id]
    if (!owner || !patch || owner.role === 'decoration' || state.hiddenGenerated?.includes(e.id)) continue
    const entity = owner.kind === 'node' ? graph.nodes.find((n) => n.id === owner.id) : owner.kind === 'group' ? graph.groups.find((g) => g.id === owner.id) : graph.edges.find((edge) => edge.from === owner.from && edge.to === owner.to)
    if (!entity || 'ink' in entity && entity.ink !== 'ink') continue
    const candidate = { kind: owner.kind, id: owner.id, ...(owner.kind === 'edge' ? { from: owner.from!, to: owner.to! } : {}) }
    if (patch.isDeleted) candidates[owner.id] = { ...candidate, restore: structuredClone(entity) } as ModelCandidate
    else if (e.type === 'text' && ('text' in patch || 'originalText' in patch) && (entity.label ?? '') !== (e.originalText || e.text)) {
      candidates[owner.id] = { ...candidate, label: entity.label ?? null } as ModelCandidate
    }
  }
  return same(candidates, state.modelCandidates ?? {}) ? state : { ...state, modelCandidates: candidates }
}

const selectionId = (selection: Selection) => selection.kind === 'node' ? selection.id : `${selection.from}>${selection.to}`
const semanticFields = new Set(['text', 'originalText', 'isDeleted'])

export function sceneOwnerAction(state: FreeScene | undefined, selection: Selection): 'model' | 'style' | undefined {
  if (!state) return undefined
  const id = selectionId(selection)
  const candidate = state.modelCandidates?.[id]
  const elements = state.elements.filter((e) => ownerOf(e)?.id === id)
  if (candidate?.restore || candidate && 'label' in candidate && elements.some((e) => e.type === 'text' && ownerOf(e)?.role === 'label' && (e.originalText || e.text) !== (candidate.label ?? ''))) return 'model'
  return elements.some((e) => Object.keys(state.overrides?.[e.id] ?? {}).some((key) => !semanticFields.has(key))) ? 'style' : undefined
}

/** Restore only the selected owner; the caller records graph/layout/scene as one undo batch. */
export function adoptSceneOwner(graph: Graph, state: FreeScene | undefined, selection: Selection): {
  graph: Graph; scene: FreeScene | undefined; kind: 'model' | 'style' | undefined; changed: boolean
} {
  const kind = sceneOwnerAction(state, selection)
  if (!state || !kind) return { graph, scene: state, kind, changed: false }
  const id = selectionId(selection), candidate = state.modelCandidates?.[id]
  let next = graph
  if (kind === 'model' && candidate) {
    if (candidate.restore) {
      next = cloneGraph(graph)
      if (candidate.kind === 'node') {
        const node = structuredClone(candidate.restore)
        if (node.parent && !next.nodes.some((n) => n.id === node.parent)) throw new Error(msg().errors.missingCandidateParent)
        if (node.group && !next.groups.some((g) => g.id === node.group)) node.group = undefined
        next.nodes = [...next.nodes.filter((n) => n.id !== id), node]
        next.nextNode = Math.max(next.nextNode, Number(/^n(\d+)$/.exec(id)?.[1] ?? 0) + 1)
      } else if (candidate.kind === 'edge') {
        if (![candidate.from, candidate.to].every((key) => next.nodes.some((n) => n.id === key))) throw new Error(msg().errors.missingCandidateEndpoints)
        next.edges = [...next.edges.filter((e) => e.from !== candidate.from || e.to !== candidate.to), structuredClone(candidate.restore)]
      } else next.groups = [...next.groups.filter((g) => g.id !== id), structuredClone(candidate.restore)]
    } else if ('label' in candidate) next = rename(graph, selection, candidate.label ?? '')
  }
  const owned = { ...state.overrides }, candidates = { ...state.modelCandidates }
  delete candidates[id]
  const generated = new Map(state.generated.map((e) => [e.id, e]))
  const elements = state.elements.map((e) => {
    if (ownerOf(e)?.id !== id) return e
    const semantic = kind === 'style' ? Object.fromEntries(Object.entries(owned[e.id] ?? {}).filter(([key]) => semanticFields.has(key))) : {}
    delete owned[e.id]
    if (Object.keys(semantic).length) owned[e.id] = semantic
    return { ...(generated.get(e.id) ?? e), ...semantic, version: e.version + 1 } as ExcalidrawElement
  })
  return { graph: next, scene: { ...state, elements, overrides: owned, modelCandidates: candidates, overlay: undefined, hiddenGenerated: kind === 'model' ? state.hiddenGenerated?.filter((key) => ownerOf(generated.get(key))?.id !== id) : state.hiddenGenerated }, kind, changed: true }
}

/** A newer explicit PAD edit replaces earlier PC overrides for the same semantic field. */
export function applySemanticEdits(before: Graph, after: Graph, state: FreeScene | undefined): FreeScene | undefined {
  if (!state) return state
  const owned = { ...state.overrides }
  const find = (g: Graph, owner: SceneOwner) => owner.kind === 'node' ? g.nodes.find((n) => n.id === owner.id)
    : owner.kind === 'group' ? g.groups.find((group) => group.id === owner.id)
    : g.edges.find((edge) => edge.from === owner.from && edge.to === owner.to)
  const elements = state.elements.map((e) => {
    const owner = ownerOf(e)
    if (!owner) return e
    const old = find(before, owner), next = find(after, owner)
    let patch: Record<string, unknown> | undefined
    if (old && !next) patch = { isDeleted: true }
    else if (old && next && old.label !== next.label && e.type === 'text' && owner.role === 'label') {
      if (owner.kind === 'edge' && !next.label) {
        // Remove the label without leaving an override that deletes its later replacement.
        delete owned[e.id]
        return { ...e, isDeleted: true, version: e.version + 1 }
      }
      patch = { text: next.label ?? '', originalText: next.label ?? '', ...(owner.kind === 'edge' ? { isDeleted: false } : {}) }
    }
    if (!patch) return e
    owned[e.id] = { ...owned[e.id], ...patch }
    return { ...e, ...patch, version: e.version + 1 } as ExcalidrawElement
  })
  return { ...state, elements, overrides: owned }
}

export function moveSceneOwner(state: FreeScene | undefined, id: string, dx: number, dy: number): FreeScene | undefined {
  if (!state || !Number.isFinite(dx) || !Number.isFinite(dy)) return state
  const owned = { ...state.overrides }
  const elements = state.elements.map((e) => {
    if (e.isDeleted || ownerOf(e)?.id !== id) return e
    const patch = { x: e.x + dx, y: e.y + dy }
    owned[e.id] = { ...owned[e.id], ...patch }
    return { ...e, ...patch, version: e.version + 1 } as ExcalidrawElement
  })
  return { ...state, elements, overrides: owned }
}

export function sceneHasContent(s: FreeScene | undefined): boolean {
  return !!s?.elements.some((e) => !e.isDeleted)
}

/** Free objects and customized owners are projected into PAD as an SVG image. */
export function overlayElements(s: FreeScene): { elements: ExcalidrawElement[]; hidden: string[] } {
  const baseline = new Map(s.generated.map((e) => [e.id, e]))
  const hidden = new Set<string>()
  for (const e of s.elements) {
    const old = baseline.get(e.id)
    const owner = ownerOf(e)
    if (old && owner && (Object.keys(overrides(old, e)).length || Object.keys(s.overrides?.[e.id] ?? {}).length)) hidden.add(owner.id)
  }
  return {
    elements: s.elements.filter((e) => !e.isDeleted && (!baseline.has(e.id) || hidden.has(ownerOf(e)?.id ?? ''))),
    hidden: [...hidden],
  }
}

export function hidesInk(key: string, overlay: SceneOverlay | undefined): boolean {
  const canonical = key.replace('~', '>')
  return !!overlay?.hidden.some((id) => canonical === id || canonical.startsWith(`${id}|`) || canonical.startsWith(`${id}t|`) || (id.includes('>') && (canonical === `${id}l` || canonical.startsWith(`${id}l|`))))
}

export function sceneBounds(b: { x0: number; y0: number; x1: number; y1: number }, overlay?: SceneOverlay) {
  if (!overlay?.width || !overlay.height) return b
  return { x0: Math.min(b.x0, overlay.x), y0: Math.min(b.y0, overlay.y), x1: Math.max(b.x1, overlay.x + overlay.width), y1: Math.max(b.y1, overlay.y + overlay.height) }
}

/** Escaped data images stay inert even if a meeting came from another computer. */
export function imageSvg(url: string, x: number, y: number, width: number, height: number): string {
  if (!/^data:image\/(?:png|jpeg|gif|webp|svg\+xml);base64,[a-zA-Z0-9+/=\s]+$/.test(url)) return ''
  if (![x, y, width, height].every(Number.isFinite) || width <= 0 || height <= 0) return ''
  return `<image href="${url}" x="${x}" y="${y}" width="${width}" height="${height}" preserveAspectRatio="none"/>`
}

export const overlaySvg = (o?: SceneOverlay): string => o ? imageSvg(o.url, o.x, o.y, o.width, o.height) : ''

/** The original page is a locked display/export image, not a second editable copy. */
export function withPageUnderlay(state: FreeScene, graph: Graph, underlay?: string): FreeScene {
  const u = graph.underlay
  if (!u || !underlay) return state
  if (!imageSvg(underlay, 0, 0, u.w, u.h)) throw new Error(msg().errors.invalidExportBackground)
  let id = '__boardcast_underlay__'
  while (state.elements.some((e) => e.id === id) || state.files[id]) id += '_'
  const element = { id, type: 'image', x: 0, y: 0, width: u.w, height: u.h, angle: 0, strokeColor: 'transparent', backgroundColor: 'transparent', fillStyle: 'solid', strokeWidth: 1, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, index: null, seed: 1, version: 1, versionNonce: 1, updated: 0, isDeleted: false, boundElements: null, link: null, locked: true, fileId: id, status: 'saved', scale: [1, 1], crop: null, customData: { boardcastUnderlay: true } } as unknown as ExcalidrawElement
  const files = { ...state.files, [id]: { id, dataURL: underlay, mimeType: /^data:([^;]+)/.exec(underlay)![1], created: 0 } } as BinaryFiles
  return { ...state, elements: [element, ...state.elements], files }
}

export function withoutPageUnderlay(state: FreeScene): FreeScene {
  const images = new Set(state.elements.filter((e) => e.type === 'image' && e.customData?.boardcastUnderlay).map((e) => (e as ExcalidrawElement & { fileId: string }).fileId))
  if (!images.size) return state
  return { ...state, elements: state.elements.filter((e) => !e.customData?.boardcastUnderlay), files: Object.fromEntries(Object.entries(state.files).filter(([id]) => !images.has(id))) }
}

/** Only edits relative to the last displayed scene are semantic user actions. */
export function sceneEdits(graph: Graph, before: readonly ExcalidrawElement[], after: readonly ExcalidrawElement[], committed: Graph = graph): {
  graph: Graph
  moves: { id: string; dx: number; dy: number }[]
} {
  let next = graph
  const prior = new Map(before.map((e) => [e.id, e]))
  const current = new Map(after.map((e) => [e.id, e]))
  const moved = new Set<string>()
  const removed = new Set<string>()
  const moves: { id: string; dx: number; dy: number }[] = []
  for (const old of before) {
    const owner = ownerOf(old)
    if (!owner || owner.role === 'decoration' || old.isDeleted) continue
    const e = current.get(old.id)
    if (owner.kind === 'group') {
      if (!e || e.isDeleted) {
        next = { ...next, groups: next.groups.filter((g) => g.id !== owner.id), nodes: next.nodes.map((n) => n.group === owner.id ? { ...n, group: undefined } : n) }
      } else if (e.type === 'text' && old.type === 'text' && e.text !== old.text) {
        next = { ...next, groups: next.groups.map((g) => g.id === owner.id ? { ...g, label: e.originalText || e.text } : g) }
      }
      continue
    }
    const selection: Selection = owner.kind === 'edge'
      ? { kind: 'edge', from: owner.from!, to: owner.to! }
      : { kind: 'node', id: owner.id }
    if ((!e || e.isDeleted) && !removed.has(owner.id)) {
      next = remove(next, selection)
      removed.add(owner.id)
      continue
    }
    if (!e || e.isDeleted) continue
    if (e.type === 'text' && old.type === 'text' && e.text !== old.text) {
      // Keep real prior names as aliases, not each keystroke in the current edit.
      const original = owner.kind === 'node' && committed !== graph && committed.nodes.find((n) => n.id === owner.id)
      if (original) next = { ...next, nodes: next.nodes.map((n) => n.id === owner.id ? { ...n, label: original.label, aliases: original.aliases } : n) }
      next = rename(next, selection, e.originalText || e.text)
    }
    if (owner.kind === 'node' && owner.role === 'body' && !moved.has(owner.id) && (e.x !== old.x || e.y !== old.y)) {
      moves.push({ id: owner.id, dx: e.x - old.x, dy: e.y - old.y })
      moved.add(owner.id)
    }
  }
  // A text-only node's label is also its movable body.
  for (const e of after) {
    const owner = ownerOf(e)
    const old = prior.get(e.id)
    if (!owner || owner.kind !== 'node' || moved.has(owner.id) || !old || e.isDeleted) continue
    if (e.x !== old.x || e.y !== old.y) {
      moves.push({ id: owner.id, dx: e.x - old.x, dy: e.y - old.y })
      moved.add(owner.id)
    }
  }
  return { graph: next, moves }
}
