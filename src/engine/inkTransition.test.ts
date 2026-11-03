import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Piece } from '../ink/compose.ts'
import { changedInk, eraseSweep } from '../ink/transition.ts'

const piece = (key: string, x = 0): Piece => ({ key, color: '#123', opacity: 1, strokes: [{ d: `M${x} 0L${x + 100} 0`, center: `M${x} 0 L${x + 100} 0`, length: 100 }] })

test('only removed or geometrically changed ink is erased; highlights and recoloring stay still', () => {
  const a = piece('n1|text'), b = piece('n2|text'), c = piece('n3|old')
  const hot = { ...piece('n1|hot'), under: true }
  assert.deepEqual(changedInk([a, b, c, hot], [{ ...a, color: '#fff', opacity: .4 }, piece(b.key, 200), piece('n3|new')]), [b, c])
  assert.deepEqual(changedInk([], [a]), [])
  assert.deepEqual(changedInk([a], [a]), [])
})

test('the erasing path covers negative coordinates and multiple rows without a visible eraser', () => {
  const p = piece('n1|text', -150)
  p.strokes.push({ d: 'M-140 100L-30 100', center: 'M-140 100 L-30 100', length: 110 })
  const sweep = eraseSweep(p)!
  assert.ok(sweep.x < -150 && sweep.y < 0)
  assert.ok(sweep.x + sweep.w > -30 && sweep.y + sweep.h > 100)
  assert.ok(sweep.length > sweep.w * 4)
  assert.equal(eraseSweep({ ...p, strokes: [] }), null)
})
