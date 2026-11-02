import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { models, supportedSamples } from './catalog.mjs'
import { compare } from './compare.mjs'
import { matrix } from './matrix.mjs'

test('every candidate has pinned assets and required paths, including external ONNX data and tokenizers', () => {
  const files = JSON.parse(readFileSync(new URL('./models.json', import.meta.url), 'utf8'))
  for (const [name, model] of Object.entries(models)) {
    if (name === 'sensevoice') continue
    const pack = files[model.package]
    assert(pack?.length, name)
    for (const f of pack) {
      assert(!f.file.startsWith('/') && !f.file.split('/').includes('..'), f.file)
      assert.match(f.url, /\/resolve\/[a-f0-9]{40}\//)
      assert.match(f.sha256, /^[a-f0-9]{64}$/)
      assert(Number.isSafeInteger(f.bytes) && f.bytes > 0)
    }
    for (const [key, path] of Object.entries(model.config)) {
      if (['family', 'language'].includes(key)) continue
      assert(pack.some((f) => key === 'tokenizer' || model.backend ? f.file.startsWith(path + '/') : f.file === path), `${name}/${path}`)
    }
  }
  assert(files['cohere-transcribe'].some((f) => f.file.endsWith('.onnx.data')))
})

test('language specialists keep silence but never count unsupported speech as a successful empty transcript', () => {
  const samples = ['zh', 'ja', 'en', 'zh-en'].map((language) => ({ id: language, language, scoring: 'reference' }))
  samples.push({ id: 'silence', scoring: 'silence' })
  assert.deepEqual(supportedSamples(samples, 'reazonspeech').map((s) => s.id), ['ja', 'silence'])
  assert.deepEqual(supportedSamples(samples, 'parakeet-v3').map((s) => s.id), ['en', 'silence'])
  assert.equal(models['cohere-ja'].languageMode, 'fixed:ja')
  assert.equal(supportedSamples(samples, 'sensevoice').length, samples.length)
  assert.throws(() => supportedSamples(samples, 'missing'), /Unknown model/)
})

test('CI keeps cheap push controls and includes every candidate once in the expanded matrix', () => {
  assert.equal(matrix('quick').include.length, 3)
  assert.equal(matrix('full').include.length, 9)
  const sweep = matrix('sweep').include
  assert.equal(sweep.length, Object.keys(models).length + 7)
  for (const name of Object.keys(models)) assert.equal(sweep.filter((m) => m.model === name && m.denoise === 'none' && m.vad === 'on' && m.policy === 'static').length, 1)
})

test('comparison refuses changed corpora, mismatched subsets and duplicate runs', () => {
  const report = { model: 'sensevoice', manifestSha256: 'a', suite: 'quick', vad: 'on', policy: 'static', denoise: 'none',
    languageMode: 'auto', checks: { passed: true, failures: [] }, details: [{ id: 'a', variant: 'original', language: 'en', scoring: 'reference', score: { errors: 2, units: 10 }, elapsedMs: 10, duration: 2 }] }
  assert.match(compare([report]), /20\.00%/)
  assert(!compare([report]).includes('| 0.00%'))
  assert.throws(() => compare([]), /No ASR reports/)
  assert.throws(() => compare([report, { ...report, model: 'other', manifestSha256: 'b' }]), /manifestSha256/)
  assert.throws(() => compare([report, { ...report, model: 'other', details: [{ ...report.details[0], id: 'b' }] }]), /Different en samples/)
  assert.throws(() => compare([report, report]), /Duplicate configuration/)
})
