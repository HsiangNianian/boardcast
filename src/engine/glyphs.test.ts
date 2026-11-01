import assert from 'node:assert/strict'
import { test } from 'node:test'
import { glyph } from '../ink/glyphs.ts'

test('handwritten ASCII has drawable strokes and finite metrics', async () => {
  for (let code = 33; code < 127; code++) {
    const ch = String.fromCharCode(code)
    const g = await glyph(ch)
    assert.ok(g && g.advance > 0 && Number.isFinite(g.advance), ch)
    assert.ok(g.strokes.length > 0, ch)
    for (const stroke of g.strokes) {
      assert.ok(stroke.length >= 2, ch)
      for (const point of stroke) {
        assert.equal(point.length, 2, ch)
        assert.ok(point.every(Number.isFinite), ch)
      }
    }
  }
  assert.deepEqual((await glyph(' '))?.strokes, [])
})

test('full-width Latin uses the same handwriting; cursive remains available', async () => {
  assert.deepEqual(await glyph('Ａ'), await glyph('A'))
  assert.deepEqual(await glyph('１'), await glyph('1'))
  assert.notDeepEqual((await glyph('a'))?.strokes, (await glyph('a', 'script'))?.strokes)
})
