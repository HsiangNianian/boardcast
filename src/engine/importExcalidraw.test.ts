import assert from 'node:assert/strict'
import { test } from 'node:test'
import { importExcalidraw } from '../board/importExcalidraw.ts'

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
  assert.throws(() => importExcalidraw(file([{ id: 'a', type: 'rectangle' }])), /没有能转成板书的东西/)
})
