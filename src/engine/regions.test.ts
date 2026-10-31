import assert from 'node:assert/strict'
import { test } from 'node:test'
import { layout } from '../board/layout.ts'
import { toMarkdown } from '../board/markdown.ts'
import { describe } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { pageGraph } from './regions.ts'
import { diffGraph, replayTo } from './steps.ts'

const page = () =>
  pageGraph({ w: 800, h: 1000 }, [
    { text: '第一章  系统设计', x: 60, y: 40, w: 400, h: 40 },
    { text: '用户通过小程序下单，订单服务校验库存后调用支付服务。支付成功后回调订单服务。', x: 60, y: 120, w: 680, h: 80 },
    { text: '用户通过小程序下单，订单服务校验库存后调用支付服务。支付成功后回调订单服务。', x: 60, y: 220, w: 680, h: 80 },
    { text: '   ', x: 0, y: 0, w: 1, h: 1 },
  ])

test('a page becomes pinned source regions the model can see', () => {
  const g = page()
  assert.deepEqual(g.underlay, { w: 800, h: 1000 })
  assert.equal(g.nodes.length, 3)
  assert.ok(g.nodes.every((n) => n.kind === 'region' && n.at))
  assert.ok(g.nodes[1].label.endsWith('…') && g.nodes[2].label.endsWith('（2）'))
  assert.match(describe(g), /n1 \[原文\] 第一章 系统设计/)
})

test('the model can mark, connect and annotate source regions but not rename, delete or move them', () => {
  const g = applyOps(page(), parseOps('M n2 圈出\nE n1 -> n2 : 「展开」\nK n2 「这里少了退款」\nR n1 「改掉」\nD n3\nL n2 「一个条目」\nG 「分组」 n1'))
  const n1 = g.nodes.find((n) => n.id === 'n1')!
  assert.equal(n1.label, '第一章 系统设计')
  assert.equal(n1.group, undefined)
  assert.ok(g.nodes.some((n) => n.id === 'n3'), '原文删不掉')
  assert.deepEqual(g.nodes.find((n) => n.id === 'n2')!.marks, ['圈出'])
  assert.equal(g.edges.length, 1)
  const kids = g.nodes.filter((n) => n.parent === 'n2')
  assert.equal(kids.length, 2)
  assert.ok(kids.every((n) => n.kind === 'note'), '挂在原文上的只能是备注')
})

test('the page takes the top-left corner, regions keep their spot, notes go to the margin', async () => {
  const g = applyOps(page(), parseOps('K n2 「这里少了退款」\nE n1 -> n2\nT 「待办」\nL 「待办」 「补退款流程」'))
  const l = await layout(g)
  assert.deepEqual(l.page, { id: '__page', x: 0, y: 0, w: 800 + 320, h: 1000 })
  assert.deepEqual(l.anchors.get('n2'), { id: 'n2', x: 60, y: 120, w: 680, h: 80 })
  // 批注在页面右边的边栏，和它说的那块原文齐平
  const note = g.nodes.find((n) => n.parent === 'n2')!
  const at = l.anchors.get(note.id)!
  assert.ok(at.x > 800 && at.x < 800 + 320 && at.y === 120)
  // 原文之间的连线照常画
  assert.deepEqual(l.links, [{ from: 'n1', to: 'n2', ink: 'ink' }].map((e) => g.edges.find((x) => x.from === e.from && x.to === e.to)))
  // 新的话题不压在页面上
  const topic = l.regions.find((r) => r.head?.label === '待办')!
  assert.ok(topic.x >= 800 + 320 || topic.y >= 1000)
})

test('markdown export lists the marked and annotated source text', () => {
  const g = applyOps(page(), parseOps('M n1 星标\nK n2 「这里少了退款」'))
  const md = toMarkdown(g, [], '评审')
  assert.match(md, /## 原文标注\n\n- 第一章 系统设计 ★\n- 用户通过小程序下单.*…\n {3}- 备注：这里少了退款/)
  assert.doesNotMatch(md, /（2）/, '没标注的原文不导出')
})

test('the page size survives replay', () => {
  const g = page()
  const d = diffGraph({ ...g, nodes: [], underlay: undefined, nextNode: 1 }, g)!
  assert.deepEqual(d.underlay, { w: 800, h: 1000 })
  assert.deepEqual(replayTo([{ t: 0, line: 0, d }], 1).underlay, { w: 800, h: 1000 })
})

test('the model sees the whole paragraph, not just the start of the label', () => {
  const words = 'Computational chemistry has become an indispensable tool for probing, predicting, and rationalizing systems.'
  const g = pageGraph({ w: 800, h: 1000 }, [{ text: words, x: 0, y: 0, w: 700, h: 200 }])
  assert.ok(g.nodes[0].label.length <= 36)
  assert.ok(describe(g).includes('predicting'))
})
