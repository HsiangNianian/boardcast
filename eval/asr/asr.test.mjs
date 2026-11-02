import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import test from 'node:test'
import { corpus, corpusFor, listeningPage, validateCorpus } from './download.mjs'
import { download } from './files.mjs'
import { errorRate, keywordRecall, tokens, recognitionFailures } from './score.mjs'
import { nativeErrors } from './runtime.mjs'

test('CER/WER/MER preserve language units, edits and empty references', () => {
  assert.deepEqual(tokens('ＡＰＩ、更新。', 'ja'), ['a', 'p', 'i', '更', '新'])
  assert.deepEqual(tokens("Don't split P_H_D_", 'en'), ['dont', 'split', 'phd'])
  assert.deepEqual(tokens('使用 Python 3。', 'zh-en'), ['使', '用', 'python', '3'])
  assert.equal(errorRate('我们测试。', '我们测试', 'zh').rate, 0)
  assert.deepEqual(errorRate('one two three', 'one four three', 'en'), { errors: 1, units: 3, rate: 1 / 3 })
  assert.equal(errorRate('one two three', 'one three', 'en').errors, 1)
  assert.equal(errorRate('one', 'one two three', 'en').rate, 2)
  assert.deepEqual(errorRate('', 'invented words', 'en'), { errors: 2, units: 0, rate: null })
  assert.equal(errorRate('日本語', '', 'ja').rate, 1)
})

test('technical terms match complete token sequences', () => {
  assert.deepEqual(keywordRecall(['python', 'computer languages'], 'Pythonic computer languages', 'zh-en'), {
    hits: ['computer languages'], total: 2, rate: 0.5,
  })
})

test('speech/silence checks catch one missing transcript even when other speech succeeds', () => {
  const details = [
    { id: 'heard', scoring: 'reference', hypothesis: 'words' },
    { id: 'quiet', scoring: 'provisional', hypothesis: '  ' },
    { id: 'silence', scoring: 'silence', hypothesis: '' },
  ]
  assert.deepEqual(recognitionFailures(details), ['quiet: no speech recognized'])
  details[1].hypothesis = 'quiet words'
  assert.deepEqual(recognitionFailures(details), [])
  details[2].hypothesis = 'invented words'
  assert.deepEqual(recognitionFailures(details), ['silence: speech hallucinated on silence'])
})

test('native exceptions cannot become a passing eval just because other segments recognized speech', () => {
  assert.deepEqual(nativeErrors('Finished test profile\n[W:onnxruntime:, graph] optional provider warning'), [])
  const failures = nativeErrors('\u001b[1;31m[E:onnxruntime:, sequential_executor] tensor shape failed\nCaught exception:\nReturn an empty result')
  assert.equal(failures.length, 1)
  assert.match(failures[0], /2 error markers/)
})

test('corpus rejects traversal, duplicate IDs and invalid reference bounds', () => {
  validateCorpus(corpus)
  for (const mutate of [
    (c) => { c.samples[0].file = '../outside.wav' },
    (c) => { c.samples.push(c.samples[0]) },
    (c) => { c.samples[0].reference[0].end = c.samples[0].duration + 1 },
    (c) => { c.samples[0].source = 'missing' },
  ]) {
    const changed = structuredClone(corpus); mutate(changed)
    assert.throws(() => validateCorpus(changed))
  }
  for (const language of ['zh', 'ja', 'en', 'zh-en']) assert(corpus.samples.some((s) => s.language === language && s.quick))
  assert(!JSON.stringify(corpus).includes('MagicData'))
})

test('listening page works without fetching JSON and escapes source transcripts', () => {
  const changed = structuredClone(corpus)
  changed.samples[0].reference[0].text = '<script>alert("bad")</script>'
  const html = listeningPage(changed)
  assert(!html.includes('<script>'))
  assert(html.includes('&lt;script&gt;'))
  assert.equal((html.match(/<audio /g) || []).length, corpus.samples.length)
  assert(html.includes('preload="none"'))
})

test('meeting corpus has continuous long clips, bounded references and explicit provenance', () => {
  const meetings = corpusFor('meetings')
  validateCorpus(meetings)
  assert.equal(corpusFor('full'), corpus)
  assert.throws(() => corpusFor('unknown'), /suite/)
  const speech = meetings.samples.filter((s) => s.scoring !== 'silence')
  for (const language of ['zh', 'ja', 'en']) assert(speech.some((s) => s.language === language))
  assert(new Set(speech.map((s) => s.industry)).size >= 5)
  for (const s of speech) {
    assert(s.duration >= 540 && s.duration <= 660, s.id)
    assert(s.kind && s.industry && s.note && s.referenceSource.url, s.id)
    assert.match(s.referenceSource.sha256, /^[a-f0-9]{64}$/)
    assert(Math.abs(s.sourceInterval[1] - s.sourceInterval[0] - s.duration) < .001, s.id)
    assert(!corpus.samples.some((old) => old.id === s.id))
    if (s.original) {
      assert.match(s.original.sha256, /^[a-f0-9]{64}$/)
      assert(Number.isSafeInteger(s.original.bytes) && s.original.bytes > 0)
      assert.equal(s.original.offset, s.sourceInterval[0])
    }
  }
  const edited = meetings.samples.find((s) => s.kind === 'public-committee')
  assert.equal(edited.scoring, 'provisional')
  const changed = structuredClone(meetings)
  changed.samples.find((s) => s.original).original.offset = -1
  assert.throws(() => validateCorpus(changed), /original audio/)
})

test('downloads verify caches, reject corrupted bytes and remove partial files', async (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'boardcast-asr-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const source = join(dir, 'source'), target = join(dir, 'target')
  const bytes = Buffer.from('known audio fixture')
  const file = { bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex'), url: pathToFileURL(source).href }
  writeFileSync(source, bytes)
  await download(file, target)
  assert.deepEqual(readFileSync(target), bytes)
  rmSync(source)
  await download(file, target) // A valid cache works offline.
  writeFileSync(source, bytes)
  writeFileSync(target, Buffer.alloc(bytes.length))
  await download(file, target) // Same-size corruption is detected and repaired.
  assert.deepEqual(readFileSync(target), bytes)
  writeFileSync(source, 'bad source')
  rmSync(target)
  await assert.rejects(download(file, target), /mismatch/)
  assert(!existsSync(target))
  assert(!existsSync(`${target}.${process.pid}.part`))
})
