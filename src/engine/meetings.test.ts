import assert from 'node:assert/strict'
import { test } from 'node:test'
import { StableLayout } from '../board/layout.ts'
import { autoTitle, newMeetingId, normalize, type Meeting } from '../meetings.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

test('a restored layout puts everything back where it was', async () => {
  const a = new StableLayout()
  let g = emptyGraph()
  for (const step of ['E 「用户」 「小程序」', 'T 「爬虫」\nL 「爬虫」 「监控任务」', 'E 「小程序」 「网关」']) {
    g = applyOps(g, parseOps(step))
    await a.layout(g)
  }
  const before = await a.layout(g)

  // 存起来、读回来（走一遍 JSON，和真的存文件一样），放进一个全新的排版器
  const saved = JSON.parse(JSON.stringify(a.snapshot()))
  const graph = JSON.parse(JSON.stringify(g))
  const b = new StableLayout()
  b.restore(saved)
  const after = await b.layout(graph)
  for (const [id, box] of before.anchors) assert.deepEqual(after.anchors.get(id), box, `${id} 位置变了`)

  // 接着加东西，旧的照样不动
  const more = await b.layout(applyOps(graph, parseOps('E 「网关」 「登录接口」')))
  for (const [id, box] of before.anchors)
    if (id.startsWith('n')) assert.deepEqual([more.anchors.get(id)?.x, more.anchors.get(id)?.y], [box.x, box.y], `${id} 挪了`)
})

test('dragging moves a flowchart node or a whole topic and leaves the rest alone', async () => {
  const b = new StableLayout()
  const g = applyOps(emptyGraph(), parseOps('E 「用户」 「网关」\nT 「爬虫」\nL n3 「监控任务」'))
  const l0 = await b.layout(g)
  assert.ok(b.movable('n2') && b.movable('n3'))
  assert.ok(!b.movable('n4'), '条目跟着话题走，不能单独拖')
  assert.ok(b.move('n2', 120, 40))
  assert.ok(b.move('n3', -30, 200))
  assert.ok(!b.move('n4', 10, 10))
  const l1 = await b.layout(g)
  const at = (l: typeof l0, id: string) => [l.anchors.get(id)!.x, l.anchors.get(id)!.y]
  assert.deepEqual(at(l1, 'n2'), [at(l0, 'n2')[0] + 120, at(l0, 'n2')[1] + 40])
  assert.deepEqual(at(l1, 'n1'), at(l0, 'n1'))
  // 话题整块挪，条目跟着走
  assert.deepEqual(at(l1, 'n4'), [at(l0, 'n4')[0] - 30, at(l0, 'n4')[1] + 200])
  // 连到挪过的节点的线重新走线，终点跟着到了新位置
  const route = l1.diagram!.edges.find((e) => e.to === 'n2')!.points
  const end = route[route.length - 1]
  const box = l1.anchors.get('n2')!
  assert.ok(end[0] >= box.x - 1 && end[0] <= box.x + box.w + 1 && end[1] >= box.y - 1 && end[1] <= box.y + box.h + 1)
})

test('meetings get a title from their content until the user names them', () => {
  const g = applyOps(emptyGraph(), parseOps('E 「用户」 「小程序」\nT 「登录流程」'))
  assert.equal(autoTitle(g, []), '登录流程')
  assert.equal(autoTitle(applyOps(emptyGraph(), parseOps('E 「用户」 「小程序」')), []), '用户')
  assert.equal(autoTitle(emptyGraph(), [{ text: '我们先把登录这块理一下吧，然后再看支付和库存' }]), '我们先把登录这块理一下吧，然后再看支付和…')
  assert.equal(autoTitle(emptyGraph(), []), '未命名会议')
})

test('ids are safe file names and sentences cut off by a crash are marked', () => {
  assert.match(newMeetingId(new Date(2026, 9, 30, 10, 5, 7)), /^20261030-100507-[a-z0-9]{1,4}$/)
  const m = normalize({
    id: 'x',
    title: 't',
    createdAt: 0,
    updatedAt: 0,
    graph: emptyGraph(),
    lines: [{ text: '画完了', ms: 120, ops: 'N 「甲」' }, { text: '还在排队' }],
  } as unknown as Meeting)
  assert.deepEqual(m.history, [])
  assert.equal(m.lines[1].ms, 0)
  assert.match(m.lines[1].ops ?? '', /还没画完/)
  assert.equal(m.lines[0].ops, 'N 「甲」')
})
