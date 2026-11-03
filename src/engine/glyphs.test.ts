import assert from 'node:assert/strict'
import { test } from 'node:test'
import { glyph } from '../ink/glyphs.ts'
import { normalizeKana } from '../ink/kana.ts'
import { readFileSync } from 'node:fs'
import { gunzipSync } from 'node:zlib'
import { textWidth } from '../board/layout.ts'

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

test('board headings and timestamp ranges have offline punctuation strokes', async (t) => {
  t.mock.method(globalThis, 'fetch', () => { throw new Error('Punctuation must work offline') })
  for (const ch of ['·', '–']) assert.ok((await glyph(ch))?.strokes.length)
})

test('Japanese kana have ordered, drawable strokes without a network or embedded font', async (t) => {
  t.mock.method(globalThis, 'fetch', () => { throw new Error('Kana must work offline') })
  const chars = [...'ー・゛゜ゝゞヽヾ']
  for (const [a, b] of [[0x3041, 0x3096], [0x30a1, 0x30fa]]) {
    for (let c = a; c <= b; c++) chars.push(String.fromCharCode(c))
  }
  for (const ch of chars) {
    const g = await glyph(ch)
    assert.ok(g && g.advance === 1 && !g.latin, ch)
    assert.ok(g.strokes.length > 0, ch)
    for (const s of g.strokes) {
      assert.ok(s.length >= 2, ch)
      assert.ok(s.every((p) => p.length === 2 && p.every(Number.isFinite)), ch)
    }
  }
  assert.equal((await glyph('デ'))!.strokes.length, 5)
  assert.equal((await glyph('ア'))!.strokes.length, 2)
})

test('half-width and decomposed kana retain voicing without normalizing unrelated symbols', () => {
  assert.equal(normalizeKana('｢ｶﾞﾊﾟﾃﾞｰ｣ か\u3099 は\u309a API ① x²'), '「ガパデー」 が ぱ API ① x²')
  assert.equal(textWidth('ｶﾞﾊﾟﾃﾞｰ', 24), textWidth('ガパデー', 24))
  assert.equal(textWidth('か\u3099', 24), textWidth('が', 24))
})

test('Japanese kanji absent from the Chinese font also render in an offline replay', async (t) => {
  const host = globalThis as typeof globalThis & { __boardcastHanzi?: Record<string, number[][][]> }
  const previous = host.__boardcastHanzi
  host.__boardcastHanzi = JSON.parse(gunzipSync(readFileSync(new URL('../ink/kanji.json.gz', import.meta.url))).toString())
  t.after(() => { host.__boardcastHanzi = previous })
  t.mock.method(globalThis, 'fetch', () => { throw new Error('Replay must work offline') })
  for (const ch of '働込峠辻気広渋沢栄実発桜駅図') {
    const g = await glyph(ch)
    assert.ok(g && g.advance === 1 && !g.latin, ch)
    assert.ok(g.strokes.length > 0, ch)
    assert.ok(g.strokes.every((s) => s.length >= 2 && s.every((p) => p.every(Number.isFinite))), ch)
  }
})
