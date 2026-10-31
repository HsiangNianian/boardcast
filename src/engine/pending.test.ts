import assert from 'node:assert/strict'
import { test } from 'node:test'
import { settle } from './pending.ts'

type L = { text: string; speaker?: string; ms?: number; ops?: string; queueMs?: number; failed?: boolean }

test('a result goes to the oldest waiting lines it was made from, not the newest', () => {
  // 第 2 句在画；第 3、4 句是模型忙时进来的，还在排队
  const lines: L[] = [
    { text: '订单服务调库存服务', ms: 57 },
    { text: '中间加一个消息队列' },
    { text: '库存要不要拆分' },
    { text: '待办里加一个压测' },
  ]
  const a = settle(lines, [{ text: '中间加一个消息队列' }], () => ({ ms: 2500, ops: 'N 「消息队列」' }))
  assert.deepEqual(
    a.map((l) => l.ms),
    [57, 2500, undefined, undefined],
  )
  const b = settle(a, [{ text: '库存要不要拆分' }, { text: '待办里加一个压测' }], (k) => ({ ms: 40, queueMs: [2100, 1900][k], ops: k === 1 ? 'Q …' : '' }))
  assert.deepEqual(
    b.map((l) => [l.ms, l.queueMs, l.ops]),
    [
      [57, undefined, undefined],
      [2500, undefined, 'N 「消息队列」'],
      [40, 2100, ''],
      [40, 1900, 'Q …'],
    ],
  )
  assert.equal(lines[1].ms, undefined, '不改原数组')
})

test('repeated words are matched in order, finished lines and unknown text are left alone', () => {
  const lines: L[] = [{ text: '好的', ms: 20 }, { text: '好的' }, { text: '好的' }, { text: '先这样' }]
  const out = settle(lines, [{ text: '好的' }, { text: '不在记录里' }], () => ({ ms: 1, failed: true }))
  assert.deepEqual(
    out.map((l) => [l.ms, l.failed]),
    [
      [20, undefined],
      [1, true],
      [undefined, undefined],
      [undefined, undefined],
    ],
  )
})
