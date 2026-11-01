import assert from 'node:assert/strict'
import { test } from 'node:test'
import { blocks } from '../board/blocks.ts'

test('text pieces join into lines, lines into paragraphs; a heading, a second column and a far paragraph stay apart', () => {
  const H = 14
  const out = blocks([
    // 标题，大字
    { text: '系统设计', x: 60, y: 40, w: 120, h: 28 },
    // 第一段两行，一行被切成两片
    { text: '用户通过小程序', x: 60, y: 100, w: 98, h: H },
    { text: '下单，订单服务', x: 160, y: 100, w: 98, h: H },
    { text: '校验库存。', x: 60, y: 118, w: 70, h: H },
    // 右边一栏，和第一段同一高度
    { text: 'Order', x: 420, y: 100, w: 40, h: H },
    { text: 'service', x: 463, y: 100, w: 50, h: H },
    // 隔得很远的第二段
    { text: '支付成功后回调。', x: 60, y: 200, w: 110, h: H },
    // 空白片段不要
    { text: '  ', x: 0, y: 0, w: 5, h: H },
  ])
  assert.deepEqual(
    out.map((b) => b.text),
    ['系统设计', '用户通过小程序下单，订单服务校验库存。', 'Order service', '支付成功后回调。'],
  )
  const para = out[1]
  assert.deepEqual([para.x, para.y, para.w, para.h], [60, 100, 198, 32])
})
