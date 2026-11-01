import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import test from 'node:test'
import { corpus, listeningPage, validateCorpus } from './download.mjs'
import { download } from './files.mjs'
import { errorRate, keywordRecall, tokens } from './score.mjs'

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
