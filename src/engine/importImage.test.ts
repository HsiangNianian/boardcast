import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cleanRead, meaningful } from '../board/importImage.ts'
import { segment } from '../board/segment.ts'

test('what the vision model reads is cleaned up, and arrowheads are not text', () => {
  assert.equal(cleanRead('「订单服务」'), '订单服务')
  assert.equal(cleanRead('```\nAPI 网关\n```'), 'API 网关')
  assert.equal(cleanRead('<think>嗯</think>MySQL'), 'MySQL')
  for (const none of ['无', '无。', '没有文字', 'None']) assert.equal(cleanRead(none), '', none)
  assert.ok(meaningful('写库') && meaningful('HTTPS') && meaningful('2026'))
  assert.ok(!meaningful('1>1') && !meaningful('->') && !meaningful(''))
})

/** 白底上画黑块当「字」，画细长的线当框和箭头 */
function canvas(w: number, h: number, draw: (fill: (x: number, y: number, w: number, h: number) => void) => void) {
  const data = new Uint8ClampedArray(w * h * 4).fill(255)
  draw((x0, y0, fw, fh) => {
    for (let y = y0; y < y0 + fh; y++)
      for (let x = x0; x < x0 + fw; x++) {
        const i = (y * w + x) * 4
        data[i] = data[i + 1] = data[i + 2] = 20
      }
  })
  return { width: w, height: h, data }
}

test('lines of text are found with tight boxes; box outlines and arrows are not text', () => {
  const img = canvas(600, 300, (fill) => {
    // 两行字组成一段：每行几个「字」，字之间有小空隙
    for (const y of [20, 42]) for (let x = 20; x < 300; x += 18) fill(x, y, 14, 14)
    // 一个框，框里两个字
    fill(380, 120, 160, 2)
    fill(380, 200, 160, 2)
    fill(380, 120, 2, 82)
    fill(538, 120, 2, 82)
    fill(440, 154, 14, 14)
    fill(458, 154, 14, 14)
    // 一根箭头连到框上
    fill(200, 160, 180, 2)
    // 远处的另一行
    for (let x = 20; x < 120; x += 18) fill(x, 260, 14, 14)
  })
  const out = segment(img)
  const near = (a: number, b: number) => Math.abs(a - b) <= 4
  const has = (x: number, y: number, w: number, h: number) => out.some((b) => near(b.x, x) && near(b.y, y) && near(b.x + b.w, x + w) && near(b.y + b.h, y + h))
  assert.equal(out.length, 3, JSON.stringify(out))
  assert.ok(has(20, 20, 284, 36), '两行拼成一段')
  assert.ok(has(440, 154, 32, 14), '框里的字，不连着框和箭头')
  assert.ok(has(20, 260, 104, 14), '远处那一行单独一块')
})
