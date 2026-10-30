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
