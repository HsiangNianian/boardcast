import assert from 'node:assert/strict'
import test from 'node:test'
import { comparison } from './dynamic.mjs'

test('paired scoring tracks a revision once, includes cost and rejects mismatched controls', () => {
  const sample = { id: 'en', scoring: 'reference', language: 'en', reference: [{ text: 'hello world' }] }
  const first = { id: 'a', revision: 0, start: 0, end: 1, text: 'hello' }
  const final = { ...first, revision: 1, text: 'hello world', review: { original: 'hello', candidate: 'hello world', decision: 'accepted', asrMs: 20 } }
  const actual = { duration: 2, elapsedMs: 50, segments: [final], trace: [first, final], baseline: { duration: 2, elapsedMs: 30, segments: [first] } }
  const c = comparison(sample, actual)
  assert.equal(c.errorDelta, -1)
  assert.equal(c.attempts, 1)
  assert.equal(c.accepted, 1)
  assert.equal(c.retryMs, 20)
  assert.equal(c.extraMs, 20)
  const next = { id: 'b', revision: 0, start: 1, end: 2, text: 'next' }
  const interleaved = comparison(sample, { ...actual, segments: [final, next], trace: [first, next, final],
    baseline: { ...actual.baseline, segments: [first, next] } })
  assert.equal(interleaved.attempts, 1) // A delayed review must not reorder the primary control.
  assert.throws(() => comparison(sample, { ...actual, segments: [final, final] }), /duplicate/)
  assert.throws(() => comparison(sample, { ...actual, trace: [first, final, final] }), /budget/)
  assert.throws(() => comparison(sample, { ...actual, baseline: { ...actual.baseline, segments: [{ ...first, text: 'different' }] } }), /differs/)
  assert.throws(() => comparison(sample, { ...actual, segments: [{ ...final, review: { ...final.review, decision: 'pending' } }] }), /incomplete/)
})
