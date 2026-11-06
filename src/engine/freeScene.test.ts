import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types'
import { type FreeScene, withInheritedInk, withoutInheritedInk, editFreeObject, freeObjectName, adoptSceneOwner, captureModelCandidates, sceneOwnerAction, applySemanticEdits, moveSceneOwner, hidesInk, mergeScene, overlayElements, protectGraphFields, sceneChanged, sceneEdits, sceneHasContent, validateScene } from '../board/freeScene.ts'
import { emptyGraph, findNode } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { diffGraph, replaySceneTo, replayTo, type Step } from './steps.ts'
import { History } from './edit.ts'

const element = (id: string, extra: Record<string, unknown> = {}): ExcalidrawElement => ({
  id, type: 'text', text: 'Original', originalText: 'Original', x: 10, y: 20, width: 100, height: 24,
  version: 1, versionNonce: 1, updated: 1, seed: 1, isDeleted: false, strokeColor: '#000',
  ...extra,
} as ExcalidrawElement)
const label = element('n1:t', { customData: { boardcast: { kind: 'node', id: 'n1', role: 'label' } } })

test('one text edit retains committed names without promoting intermediate keystrokes to aliases', () => {
  const committed = applyOps(emptyGraph(), parseOps('N 「Original」\nA n1 「Former name」\nN 「Unrelated」'))
  const snapshot = structuredClone(committed)
  let graph = applyOps(committed, parseOps('M n2 星标'))
  let previous = label
  for (const text of ['R', 'Re', 'Renamed']) {
    const current = element(label.id, { ...label, text, originalText: text, x: previous.x + 5 })
    const edit = sceneEdits(graph, [previous], [current], committed)
    assert.deepEqual(edit.moves, [{ id: 'n1', dx: 5, dy: 0 }])
    graph = edit.graph
    previous = current
  }
  assert.equal(graph.nodes[0].label, 'Renamed')
  assert.deepEqual(graph.nodes[0].aliases, ['Former name', 'Original'])
  assert.equal(findNode(graph, 'Re'), undefined)
  assert.equal(findNode(graph, 'Former name')?.id, 'n1')
  assert.deepEqual(graph.nodes[1].marks, ['星标'])
  assert.deepEqual(committed, snapshot)
})

test('text undo and separate editing sessions preserve real prior names without abandoned drafts', () => {
  const initial = applyOps(emptyGraph(), parseOps('N 「Original」\nA n1 「Former name」'))
  let graph = initial
  let previous = label
  for (const text of ['未', '未确认', 'Original']) {
    const current = element(label.id, { ...label, text, originalText: text })
    graph = sceneEdits(graph, [previous], [current], initial).graph
    previous = current
  }
  assert.deepEqual(graph, initial)
  for (const text of ['新名称', '第三次', 'Former name']) {
    const current = element(label.id, { ...label, text, originalText: text })
    graph = sceneEdits(graph, [previous], [current], graph).graph
    previous = current
  }
  assert.equal(graph.nodes[0].label, 'Former name')
  assert.deepEqual(graph.nodes[0].aliases, ['Original', '新名称', '第三次'])
  assert.equal(findNode(graph, '未确认'), undefined)
})

test('continuation fading preserves original opacity, explicit edits, duplicates and new objects', () => {
  const graph = applyOps(emptyGraph(), parseOps('N 「Original」'), 'ink')
  const text = { ...label, opacity: 100, strokeColor: '#1e1e1e' }
  const box = element('box', { type: 'rectangle', opacity: 80 })
  const source: FreeScene = { elements: [text, box], generated: [text], files: {} }
  const display = withInheritedInk(source, graph, graph, source, true)
  assert.ok(display.elements.every((e, i) => e.opacity < source.elements[i].opacity))
  assert.equal(sceneChanged(withoutInheritedInk(display), source), false)
  const changed = withoutInheritedInk({ ...display, elements: [display.elements[0], { ...display.elements[1], x: 99 }, { ...display.elements[1], id: 'duplicate' }, element('new', { type: 'rectangle', opacity: 100 })] })
  const next = withInheritedInk(changed, graph, graph, source)
  assert.equal(next.elements[1].opacity, 80)
  assert.equal(next.elements[2].opacity, 80)
  assert.equal(next.elements[3].opacity, 100)
  assert.equal(withoutInheritedInk({ ...display, elements: [{ ...display.elements[1], opacity: 35 }] }).elements[0].opacity, 35)
  assert.equal(source.elements[1].opacity, 80)
})

test('continuation retains faded old bodies and marks while renames, new marks and changed edges use full ink', () => {
  const base = applyOps(emptyGraph(), parseOps('N 「Original」\nN 「Second」\nE n1 n2 : 「Before」\nM n1 圈出'), 'ink')
  const graph = applyOps(base, parseOps('R n1 「Changed」\nM n1 星标\nE n1 n2 : 「After」'), 'ink')
  const node = (role: string) => ({ boardcast: { kind: 'node', id: 'n1', role } })
  const generated = [
    element('n1', { type: 'rectangle', opacity: 100, customData: node('body') }),
    element('n1:t', { opacity: 100, text: 'Changed', customData: node('label') }),
    element('n1:mark:circle', { type: 'ellipse', opacity: 100, customData: node('decoration') }),
    element('n1:mark:star', { opacity: 100, customData: node('decoration') }),
    element('n1>n2:label', { opacity: 100, customData: { boardcast: { kind: 'edge', id: 'n1>n2', from: 'n1', to: 'n2', role: 'label' } } }),
  ]
  const state = { elements: generated, generated, files: {} }
  const faded = withInheritedInk(state, graph, base)
  assert.deepEqual(faded.elements.map((e) => e.opacity), [68, 100, 68, 100, 100])
  assert.equal(withInheritedInk(state, graph), state)
})

test('content-list names preserve visible text and deletion unbinds related free objects in one undo step', () => {
  const state: FreeScene = { elements: [label, element('box', { type: 'rectangle', boundElements: [{ id: 'caption', type: 'text' }] }), element('caption', { containerId: 'box' }), element('arrow', { type: 'arrow', startBinding: { elementId: 'box' } })], generated: [label], files: {} }
  const named = editFreeObject(state, 'box', '  Decision  ')
  assert.equal(freeObjectName(named.elements[1]), 'Decision')
  assert.equal(freeObjectName(state.elements[1]), '')
  assert.equal(editFreeObject(named, 'n1:t', null), named)
  const history = new History<FreeScene>()
  history.record(named)
  const deleted = editFreeObject(named, 'box', null)
  assert.deepEqual(deleted.elements.map((e) => e.isDeleted), [false, true, true, false])
  assert.equal((deleted.elements[3] as unknown as { startBinding: unknown }).startBinding, null)
  assert.equal(history.undo(deleted), named)
  assert.equal(named.elements[2].isDeleted, false)
  assert.equal(freeObjectName(editFreeObject(named, 'box', 'x'.repeat(150)).elements[1]).length, 120)
})

test('three-way merge keeps user fields and free objects while refreshing untouched model fields', () => {
  const original = mergeScene(undefined, [label])
  const free = element('free-text', { text: 'free', originalText: 'free' })
  const edited = { ...original, elements: [{ ...label, x: 99, strokeColor: '#f00' }, free] }
  const generated = [{ ...label, text: 'Model update', originalText: 'Model update', y: 30 }]
  const result = mergeScene(edited, generated)
  assert.equal(result.elements[0].x, 99)
  assert.equal(result.elements[0].strokeColor, '#f00')
  assert.equal(result.elements[0].y, 30)
  assert.equal(result.elements[1].id, 'free-text')
  assert.deepEqual(overlayElements(result).hidden, ['n1'])
  assert.ok(hidesInk('n1|Model update', { url: '', x: 0, y: 0, width: 0, height: 0, hidden: ['n1'] }))
  assert.equal(original.elements[0].x, 10)
})

test('owned edge overlays hide generated labels and backgrounds without hiding adjacent edge IDs', () => {
  const overlay = { url: '', x: 0, y: 0, width: 0, height: 0, hidden: ['n1>n2'] }
  for (const separator of ['>', '~']) {
    for (const suffix of ['', 'l', 'l|first', 'l|bg']) assert.equal(hidesInk(`n1${separator}n2${suffix}`, overlay), true)
    assert.equal(hidesInk(`n1${separator}n20l|other`, overlay), false)
  }
})

test('explicit user ownership survives a graph round-trip and later model conflicts', () => {
  const graph = applyOps(emptyGraph(), parseOps('N 「Original」'))
  const user = { ...label, text: 'User label', originalText: 'User label' } as ExcalidrawElement
  const edited = sceneEdits(graph, [label], [user])
  assert.equal(edited.graph.nodes[0].label, 'User label')
  const first = mergeScene({ ...mergeScene(undefined, [label]), elements: [user] }, [user])
  const reopened: FreeScene = JSON.parse(JSON.stringify(first))
  const conflict = mergeScene(reopened, [{ ...label, text: 'Model conflict', originalText: 'Model conflict' } as ExcalidrawElement])
  assert.equal((conflict.elements[0] as typeof label & { text: string }).text, 'User label')
  assert.equal(protectGraphFields(graph, conflict).nodes[0].label, 'User label')
})

test('user deletion remains deleted and model deletion preserves customized objects as free content', () => {
  const initial = mergeScene(undefined, [label])
  const deleted = mergeScene({ ...initial, elements: [{ ...label, isDeleted: true }] }, [label])
  assert.equal(deleted.elements[0].isDeleted, true)
  const customized = mergeScene({ ...initial, elements: [{ ...label, strokeColor: '#f00' }] }, [])
  assert.equal(customized.elements.length, 1)
  assert.equal(customized.elements[0].customData?.boardcast, undefined)
  assert.equal(sceneHasContent(customized), true)
})

test('program echoes, selection changes and version bumps do not create semantic edits or history changes', () => {
  const g = applyOps(emptyGraph(), parseOps('N 「Original」'))
  const initial = mergeScene(undefined, [label])
  const echo = { ...initial, elements: [{ ...label, version: 12, updated: 99, versionNonce: 123, seed: 321 }] }
  assert.equal(sceneChanged(initial, echo), false)
  assert.equal(sceneEdits(g, initial.elements, echo.elements).graph, g)
  assert.deepEqual(sceneEdits(g, initial.elements, echo.elements).moves, [])
  const moved = { ...label, x: 41 }
  assert.deepEqual(sceneEdits(g, [label], [moved]).moves, [{ id: 'n1', dx: 31, dy: 0 }])
  assert.equal(sceneEdits(g, [label], [{ ...label, isDeleted: true }]).graph.nodes.length, 0)
})

test('scene-only steps and semantic steps survive JSON round-trip and replay independently', () => {
  const g = applyOps(emptyGraph(), parseOps('N 「Original」'))
  const free = mergeScene(undefined, [])
  free.elements = [element('drawing', { type: 'freedraw', points: [[0, 0], [10, 20]] })]
  const steps: Step[] = [{ t: 1, line: 0, d: diffGraph(emptyGraph(), g)! }, { t: 2, line: 1, d: {}, scene: free }, { t: 3, line: 2, d: {}, scene: null }]
  const reopened: Step[] = JSON.parse(JSON.stringify(steps))
  assert.equal(diffGraph(replayTo(reopened, 2), g), null)
  assert.equal(sceneHasContent(replaySceneTo(reopened, 2)), true)
  assert.equal(replaySceneTo(reopened, 1), undefined)
  assert.equal(replaySceneTo(reopened, 3), undefined)
})


test('imported scenes reject duplicate identity, active links and external resources', () => {
  const base = mergeScene(undefined, [label])
  assert.doesNotThrow(() => validateScene(base))
  assert.throws(() => validateScene({ ...base, elements: [label, label] }), /Invalid saved board element/)
  assert.throws(() => validateScene({ ...base, elements: [{ ...label, link: 'javascript:alert(1)' }] }), /Unsafe board link/)
  assert.throws(() => validateScene({ ...base, files: { x: { dataURL: 'https://example.com/tracker.png' } } }), /Invalid saved board image/)
  assert.throws(() => validateScene({ ...base, overlay: { url: 'data:text/html;base64,PHNjcmlwdD4=', x: 0, y: 0, width: 10, height: 10, hidden: [] } }), /Invalid saved board projection/)
})

test('newer PAD rename, movement and deletion replace earlier PC overrides', () => {
  const original = applyOps(emptyGraph(), parseOps('N 「Original」'))
  const pcLabel = { ...label, text: 'PC name', originalText: 'PC name', x: 99 } as ExcalidrawElement
  const pc = mergeScene({ ...mergeScene(undefined, [label]), elements: [pcLabel] }, [label])
  const pcGraph = protectGraphFields(original, pc)
  const padGraph = applyOps(pcGraph, parseOps('R n1 「PAD name」'))
  const renamed = applySemanticEdits(pcGraph, padGraph, pc)!
  const refreshed = mergeScene(renamed, [{ ...label, text: 'PAD name', originalText: 'PAD name' } as ExcalidrawElement])
  assert.equal((refreshed.elements[0] as typeof label & { text: string }).text, 'PAD name')
  assert.equal(protectGraphFields(original, refreshed).nodes[0].label, 'PAD name')
  const moved = moveSceneOwner(refreshed, 'n1', 31, -8)!
  assert.equal(mergeScene(moved, [label]).elements[0].x, 130)
  assert.equal(moved.elements[0].y, 12)
  const deletedGraph = applyOps(padGraph, parseOps('D n1'))
  const deleted = mergeScene(applySemanticEdits(padGraph, deletedGraph, moved), [])
  assert.equal(sceneHasContent(deleted), false, 'explicit PAD deletion must not detach and preserve the customized object')
})

test('clearing an edge label removes its scene text and lets a later label return', () => {
  const graph = applyOps(emptyGraph(), parseOps('E 「Client」 -> 「Service」 : 「requests」'))
  const arrow = element('n1>n2', { type: 'arrow', customData: { boardcast: { kind: 'edge', id: 'n1>n2', from: 'n1', to: 'n2', role: 'body' } } })
  const caption = element('n1>n2:label', { text: 'requests', originalText: 'requests', customData: { boardcast: { kind: 'edge', id: 'n1>n2', from: 'n1', to: 'n2', role: 'label' } } })
  const note = element('free-note', { text: 'User note' })
  const state = { ...mergeScene(undefined, [arrow, caption]), elements: [arrow, { ...caption, strokeColor: '#f00' }, note] }
  const clearedGraph = { ...graph, edges: graph.edges.map(e => ({ ...e, label: undefined })) }
  const cleared = applySemanticEdits(graph, clearedGraph, state)!
  const settled = mergeScene(cleared, [arrow])
  assert.deepEqual(settled.elements.filter(e => !e.isDeleted).map(e => e.id), [arrow.id, note.id])
  assert.equal(settled.overrides?.[caption.id], undefined)
  for (const previous of [cleared, settled]) {
    const restored = mergeScene(applySemanticEdits(clearedGraph, graph, previous), [arrow, caption])
    const again = mergeScene(restored, [arrow, caption])
    assert.ok(again.elements.some(e => e.id === caption.id && !e.isDeleted && e.type === 'text' && e.text === 'requests'))
    assert.deepEqual(again.elements.find(e => e.id === note.id), note)
  }
  assert.equal(state.elements[1].isDeleted, false)
})

test('real rejected model labels survive unrelated turns, persistence, adoption and undo/redo', () => {
  const original = applyOps(emptyGraph(), parseOps('N 「Original」'))
  const user = { ...label, text: 'User', originalText: 'User', strokeColor: '#f00' } as ExcalidrawElement
  let state = mergeScene({ ...mergeScene(undefined, [label]), elements: [user] }, [label])
  const raw = applyOps(original, parseOps('R n1 「Model proposal」'))
  state = captureModelCandidates(raw, state)!
  const protectedGraph = protectGraphFields(raw, state)
  assert.equal(protectedGraph.nodes[0].label, 'User')
  assert.equal(state.modelCandidates?.n1.label, 'Model proposal')
  assert.equal(captureModelCandidates(protectedGraph, state), state, 'an unrelated turn must not erase the rejected proposal')
  state = JSON.parse(JSON.stringify(state))
  validateScene(state)
  const before = { graph: protectedGraph, scene: state }
  const history = new History<typeof before>()
  history.record(before)
  const adopted = adoptSceneOwner(protectedGraph, state, { kind: 'node', id: 'n1' })
  assert.equal(adopted.kind, 'model')
  assert.equal(adopted.graph.nodes[0].label, 'Model proposal')
  assert.equal(adopted.scene?.modelCandidates?.n1, undefined)
  const projected = mergeScene(adopted.scene, [{ ...label, text: 'Model proposal', originalText: 'Model proposal' } as ExcalidrawElement])
  assert.equal(projected.elements[0].strokeColor, '#000')
  assert.equal(sceneOwnerAction(projected, { kind: 'node', id: 'n1' }), undefined)
  const after = { graph: adopted.graph, scene: projected }
  assert.deepEqual(history.undo(after), before)
  assert.deepEqual(history.redo(before), after)
  assert.equal(state.modelCandidates?.n1.label, 'Model proposal', 'adoption does not mutate undo state')
})

test('without a captured model conflict, reset styles keeps the user semantic label and free content', () => {
  const graph = applyOps(emptyGraph(), parseOps('N 「User」'))
  const user = { ...label, text: 'User', originalText: 'User', strokeColor: '#f00', x: 40 } as ExcalidrawElement
  const free = element('free')
  const state = mergeScene({ ...mergeScene(undefined, [label]), elements: [user, free] }, [label])
  assert.equal(sceneOwnerAction(state, { kind: 'node', id: 'n1' }), 'style')
  const reset = adoptSceneOwner(graph, state, { kind: 'node', id: 'n1' })
  assert.equal(reset.kind, 'style')
  assert.equal(reset.graph, graph)
  assert.equal((reset.scene!.elements[0] as ExcalidrawElement & { text: string }).text, 'User')
  assert.equal(reset.scene!.elements[0].strokeColor, '#000')
  assert.equal(reset.scene!.elements[0].x, 10)
  assert.equal(reset.scene!.elements[1], free)
  const later = applyOps(graph, parseOps('R n1 「Later model」'))
  assert.equal(protectGraphFields(later, reset.scene).nodes[0].label, 'User')
  assert.equal(sceneOwnerAction(reset.scene, { kind: 'node', id: 'n1' }), undefined)
})

test('new conflicts replace only the matching owner and an edge label can adopt a real empty proposal', () => {
  const graph = applyOps(emptyGraph(), parseOps('E 「From」 -> 「To」 : 「Original edge」'))
  const edge = element('n1>n2:label', { text: 'Original edge', originalText: 'Original edge', customData: { boardcast: { kind: 'edge', id: 'n1>n2', from: 'n1', to: 'n2', role: 'label' } } })
  const user = { ...edge, text: 'User edge', originalText: 'User edge' } as ExcalidrawElement
  let state = mergeScene({ ...mergeScene(undefined, [edge]), elements: [user] }, [edge])
  state = captureModelCandidates({ ...graph, edges: [{ ...graph.edges[0], label: 'Proposal 1' }] }, state)!
  state = captureModelCandidates({ ...graph, edges: [{ ...graph.edges[0], label: undefined }] }, state)!
  assert.equal(state.modelCandidates?.['n1>n2'].label, null)
  assert.equal(protectGraphFields({ ...graph, edges: [{ ...graph.edges[0], label: undefined }] }, state).edges[0].label, 'User edge')
  const result = adoptSceneOwner(graph, state, { kind: 'edge', from: 'n1', to: 'n2' })
  assert.equal(result.kind, 'model')
  assert.equal(result.graph.edges[0].label, undefined)
})

test('a protected user deletion retains the actual proposed entity and malformed candidates are rejected', () => {
  const graph = applyOps(emptyGraph(), parseOps('N 「Proposed node」'))
  const state = mergeScene({ ...mergeScene(undefined, [label]), elements: [{ ...label, isDeleted: true }] }, [label])
  const captured = captureModelCandidates(graph, state)!
  const protectedGraph = protectGraphFields(graph, captured)
  assert.equal(protectedGraph.nodes.length, 0)
  const restored = adoptSceneOwner(protectedGraph, captured, { kind: 'node', id: 'n1' })
  assert.equal(restored.graph.nodes[0].label, 'Proposed node')
  assert.throws(() => validateScene({ ...captured, modelCandidates: { n1: { kind: 'node', id: 'n1', label: 123 } } }), /candidate label/)
  assert.throws(() => validateScene({ ...captured, modelCandidates: { n1: { kind: 'node', id: 'other', label: 'x' } } }), /candidate/)
})
