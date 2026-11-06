import assert from 'node:assert/strict'
import { test } from 'node:test'
import { importExcalidraw } from '../board/importExcalidraw.ts'
import { setLangChoice } from '../i18n/index.ts'
import { mergeScene, ownerOf } from '../board/freeScene.ts'
import type { ExcalidrawArrowElement, ExcalidrawTextElement } from '@excalidraw/excalidraw/element/types'

// These tests check the Chinese wording
setLangChoice('zh')

const file = (elements: object[]) => JSON.stringify({ type: 'excalidraw', version: 2, elements })

test('boxes with text become nodes, bound arrows become labelled edges, frames become groups', () => {
  const r = importExcalidraw(
    file([
      { id: 'f', type: 'frame', name: '入口层', x: 0, y: 0 },
      { id: 'a', type: 'rectangle', x: 0, y: 0, frameId: 'f' },
      { id: 'ta', type: 'text', text: '用户', containerId: 'a' },
      { id: 'b', type: 'ellipse', x: 300, y: 0, frameId: 'f' },
      { id: 'tb', type: 'text', text: 'API\n网关', containerId: 'b' },
      { id: 'c', type: 'diamond', x: 600, y: 0 },
      { id: 'tc', type: 'text', text: '鉴权「通过」', containerId: 'c' },
      { id: 'ab', type: 'arrow', startBinding: { elementId: 'a' }, endBinding: { elementId: 'b' } },
      { id: 'tab', type: 'text', text: '请求', containerId: 'ab' },
      { id: 'bc', type: 'arrow', startBinding: { elementId: 'b' }, endBinding: { elementId: 'c' } },
      // 只有一头绑着的箭头不算
      { id: 'x', type: 'arrow', startBinding: { elementId: 'c' }, endBinding: null },
      // 删掉的东西不要
      { id: 'd', type: 'rectangle', x: 0, y: 500, isDeleted: true },
      { id: 'td', type: 'text', text: '已删除', containerId: 'd', isDeleted: true },
      // 散字
      { id: 'n', type: 'text', text: '下周上线', x: 0, y: 800 },
    ]),
  )
  const name = (id: string) => r.graph.nodes.find((n) => n.id === id)?.label
  assert.deepEqual(
    r.graph.nodes.filter((n) => n.kind === 'node').map((n) => n.label),
    ['用户', 'API 网关', '鉴权通过'],
  )
  assert.deepEqual(
    r.graph.edges.map((e) => [name(e.from), name(e.to), e.label]),
    [
      ['用户', 'API 网关', '请求'],
      ['API 网关', '鉴权通过', undefined],
    ],
  )
  assert.deepEqual(
    r.graph.groups.map((g) => g.label),
    ['入口层'],
  )
  assert.equal(r.graph.nodes.filter((n) => n.group === r.graph.groups[0].id).length, 2)
  assert.ok(r.graph.nodes.some((n) => n.kind === 'topic' && n.label === '白板上的文字'))
  assert.ok(r.graph.nodes.some((n) => n.kind === 'item' && n.label === '下周上线'))
  assert.deepEqual([r.nodes, r.edges, r.groups, r.notes], [3, 2, 1, 1])
  assert.ok(r.graph.nodes.every((n) => n.ink === 'ink'))
})

test('two boxes with the same text stay two nodes; connected boxes without text get a name', () => {
  const r = importExcalidraw(
    file([
      { id: 'a', type: 'rectangle', x: 0, y: 0 },
      { id: 'ta', type: 'text', text: '服务', containerId: 'a' },
      { id: 'b', type: 'rectangle', x: 300, y: 0 },
      { id: 'tb', type: 'text', text: '服务', containerId: 'b' },
      { id: 'c', type: 'rectangle', x: 600, y: 0 },
      { id: 'ab', type: 'arrow', startBinding: { elementId: 'a' }, endBinding: { elementId: 'b' } },
      { id: 'bc', type: 'arrow', startBinding: { elementId: 'b' }, endBinding: { elementId: 'c' } },
    ]),
  )
  assert.deepEqual(
    r.graph.nodes.map((n) => n.label),
    ['服务', '服务（2）', '框 1'],
  )
  assert.equal(r.graph.edges.length, 2)
})

test('files that are not excalidraw drawings are rejected with a reason', () => {
  assert.throws(() => importExcalidraw('not json'), /不是 JSON/)
  assert.throws(() => importExcalidraw('{"foo":1}'), /没有 elements/)
  assert.throws(() => importExcalidraw(file([])), /没有能转成板书的东西/)
})


test('unlabelled shapes, freehand strokes and image assets remain editable without graph nodes', () => {
  const files = { image1: { id: 'image1', mimeType: 'image/png', dataURL: 'data:image/png;base64,YWJj', created: 1 } }
  const r = importExcalidraw(JSON.stringify({ elements: [
    { id: 'box', type: 'rectangle', x: 10, y: 20, width: 80, height: 40 },
    { id: 'ink', type: 'freedraw', x: 0, y: 0, width: 10, height: 20, points: [[0, 0], [10, 20]], pressures: [0.5, 0.5] },
    { id: 'image', type: 'image', x: 30, y: 40, width: 50, height: 50, fileId: 'image1' },
  ], files }))
  assert.equal(r.graph.nodes.length, 0)
  assert.equal(r.scene.elements.length, 3)
  assert.deepEqual(r.scene.files, files)
  assert.equal(r.scene.pendingImport, true)
})

test('Boardcast drawing metadata restores the exact graph and user overrides', () => {
  const original = importExcalidraw(file([{ id: 'text', type: 'text', text: '自由文字', x: 2, y: 3 }]))
  const scene = original.scene
  const modelCandidates = { n2: { kind: 'node', id: 'n2', label: '未采用的模型文字' } }
  const restored = importExcalidraw(JSON.stringify({ elements: scene.elements, files: scene.files, boardcast: { version: 2, graph: original.graph, generated: scene.generated, overrides: { 'import:text': { strokeColor: '#f00' } }, modelCandidates } }))
  assert.deepEqual(restored.graph, JSON.parse(JSON.stringify(original.graph)))
  assert.deepEqual([restored.nodes, restored.notes], [original.nodes, original.notes])
  assert.deepEqual(restored.scene.elements, JSON.parse(JSON.stringify(scene.elements)))
  assert.equal(restored.scene.overrides?.['import:text'].strokeColor, '#f00')
  assert.deepEqual(restored.scene.modelCandidates, modelCandidates)
})

test('portable Excalidraw underlays restore once as page data without losing ordinary images', () => {
  const original = importExcalidraw(file([{ id: 'shape', type: 'rectangle', x: 10, y: 20, width: 50, height: 50 }]))
  const underlay = 'data:image/png;base64,YWJj'
  const helper = { id: 'background', type: 'image', x: 0, y: 0, width: 500, height: 300, fileId: 'background', customData: { boardcastUnderlay: true } }
  const source = { elements: [helper, ...original.scene.elements], files: { background: { id: 'background', dataURL: underlay, mimeType: 'image/png', created: 0 } }, boardcast: { version: 2, graph: { ...original.graph, underlay: { w: 500, h: 300 } }, underlay, generated: [] } }
  const restored = importExcalidraw(JSON.stringify(source))
  assert.equal(restored.underlay, underlay)
  assert.equal(restored.scene.elements.length, 1)
  assert.equal(restored.scene.files.background, undefined)
  assert.throws(() => importExcalidraw(JSON.stringify({ ...source, boardcast: { ...source.boardcast, underlay: 'https://example.com/tracker.png' } })), /绘图中的 Boardcast 背景图片无效/)
})

test('repeated notes, normalized labels and literal suffixes keep distinct object identities', () => {
  const r = importExcalidraw(file([
    ...['Service', 'SERVICE', 'Service（2）'].flatMap((text, i) => [
      { id: `box${i}`, type: 'rectangle', x: i * 200, y: 0 },
      { id: `label${i}`, type: 'text', text, containerId: `box${i}` },
    ]),
    ...['Review', 'Review', 'RE VIEW', 'Service', '白板上的文字'].map((text, i) => ({ id: `note${i}`, type: 'text', text, x: i * 150, y: 200 })),
  ]))
  assert.equal(r.graph.nodes.filter((n) => n.kind === 'node').length, 3)
  assert.equal(r.graph.nodes.filter((n) => n.kind === 'item').length, 5)
  assert.equal(r.graph.nodes.filter((n) => n.kind === 'topic').length, 1)
  assert.equal(r.scene.elements.length, 11)
  assert.equal(new Set(r.scene.elements.map((e) => e.id)).size, 11)
  const notes = r.scene.elements.filter((e) => e.type === 'text' && !e.containerId)
  assert.equal(new Set(notes.map((e) => ownerOf(e)?.id)).size, 5)
  assert.deepEqual(notes.map((e) => e.type === 'text' && e.text), ['Review', 'Review', 'RE VIEW', 'Service', '白板上的文字'])
})

test('repeated and unnamed frames retain separate memberships and mapped frame references', () => {
  const r = importExcalidraw(file(['Team', 'TE AM', null].flatMap((name, i) => [
    { id: `frame${i}`, type: 'frame', name, x: i * 200, y: 0 },
    { id: `box${i}`, type: 'rectangle', frameId: `frame${i}`, x: i * 200, y: 40 },
    { id: `label${i}`, type: 'text', text: `Task ${i}`, containerId: `box${i}` },
  ])))
  assert.equal(r.graph.groups.length, 3)
  assert.equal(new Set(r.graph.nodes.map((n) => n.group)).size, 3)
  for (const frame of r.scene.elements.filter((e) => e.type === 'frame')) {
    assert.equal(ownerOf(frame)?.kind, 'group')
    const member = r.scene.elements.find((e) => e.frameId === frame.id)
    assert.ok(member)
    assert.equal(r.graph.nodes.find((n) => n.id === ownerOf(member)?.id)?.group, ownerOf(frame)?.id)
  }
})

test('parallel arrows keep every label, binding and path without duplicating semantic edge IDs', () => {
  const r = importExcalidraw(file([
    ...['a', 'b'].flatMap((id, i) => [{ id, type: 'rectangle', x: i * 300, y: 0 }, { id: id + '-label', type: 'text', text: id, containerId: id }]),
    ...['first', 'second'].flatMap((text, i) => [
      { id: `arrow${i}`, type: 'arrow', x: 20, y: i * 80, points: [[0, 0], [100, 40], [300, 0]], startBinding: { elementId: 'a' }, endBinding: { elementId: 'b' }, boundElements: [{ id: `arrow-label${i}`, type: 'text' }] },
      { id: `arrow-label${i}`, type: 'text', text, containerId: `arrow${i}` },
    ]),
  ]))
  assert.equal(r.graph.edges.length, 1)
  assert.equal(r.graph.edges[0].label, 'first')
  const arrows = r.scene.elements.filter((e): e is ExcalidrawArrowElement => e.type === 'arrow')
  assert.equal(arrows.length, 2)
  assert.equal(arrows.filter((e) => ownerOf(e)?.kind === 'edge').length, 1)
  assert.deepEqual(arrows.map((e) => e.points), [[[0, 0], [100, 40], [300, 0]], [[0, 0], [100, 40], [300, 0]]])
  assert.deepEqual(arrows.map((e) => r.scene.elements.find((t): t is ExcalidrawTextElement => t.id === e.boundElements?.[0].id && t.type === 'text')?.containerId), arrows.map((e) => e.id))
  assert.ok(arrows.every((e) => e.startBinding?.elementId === r.graph.edges[0].from && e.endBinding?.elementId === r.graph.edges[0].to))
})

test('import ownership preserves custom data and rough stroke seeds through the first generated projection', () => {
  const customData = { plugin: { version: 7, note: 'keep this extension data' }, boardcast: { kind: 'node', id: 'untrusted', role: 'body' } }
  const r = importExcalidraw(file([
    { id: 'a', type: 'rectangle', customData, seed: 223, roughness: 2 },
    { id: 'label', type: 'text', text: 'Task', containerId: 'a', customData: { locale: 'ja' } },
    { id: 'free', type: 'rectangle', customData: { pinned: true } },
  ]))
  const generated = r.scene.elements.filter((e) => ownerOf(e)).map((e) => ({ ...e, seed: e.seed + 11, customData: { boardcast: e.customData?.boardcast } }))
  const projected = mergeScene(r.scene, generated)
  assert.equal(projected.elements.find((e) => e.id === 'n1')?.seed, 223)
  assert.deepEqual(projected.elements.find((e) => e.id === 'n1')?.customData?.plugin, customData.plugin)
  assert.equal(projected.elements.find((e) => e.type === 'text')?.customData?.locale, 'ja')
  assert.equal(projected.elements.find((e) => e.id === 'import:free')?.customData?.pinned, true)
  assert.equal(ownerOf(projected.elements.find((e) => e.id === 'n1'))?.id, 'n1')
})
