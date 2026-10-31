import assert from 'node:assert/strict'
import { test } from 'node:test'
import { makePdf } from '../board/pdf.ts'

// 一张假 JPEG（只要字节原样塞进去就行，PDF 结构本身不解码它）
const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9])

test('the PDF has one page per image and a cross-reference table that points at every object', () => {
  const bytes = makePdf(
    [
      { jpeg, width: 1600, height: 900 },
      { jpeg, width: 1190, height: 842 },
    ],
    undefined,
    '周会 · 订单系统',
  )
  const text = new TextDecoder('latin1').decode(bytes)
  assert.ok(text.startsWith('%PDF-1.4'))
  assert.ok(text.trimEnd().endsWith('%%EOF'))
  assert.match(text, /\/Type \/Pages \/Count 2 \/Kids \[4 0 R 7 0 R\]/)
  assert.equal(text.match(/\/Subtype \/Image/g)?.length, 2)
  // 交叉引用表里每个对象的偏移都对得上「n 0 obj」
  const start = Number(/startxref\n(\d+)/.exec(text)![1])
  assert.ok(text.slice(start).startsWith('xref'))
  const rows = text.slice(start).split('\n').slice(3, 3 + 9)
  rows.forEach((row, i) => {
    const off = Number(row.slice(0, 10))
    assert.ok(text.startsWith(`${i + 1} 0 obj`, off), `对象 ${i + 1} 的偏移不对`)
  })
  // 标题是 UTF-16 十六进制（「周」是 5468）
  assert.match(text, /\/Title <FEFF5468/)
  // 宽图按比例放进 A4 横向：宽铺满
  assert.match(text, /q 842\.00 0 0 473\.63 0\.00 60\.69 cm \/Im0 Do Q/)
})
