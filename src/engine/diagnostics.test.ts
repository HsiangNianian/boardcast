import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cleanLog, makeZip, recentErrors, recordError, scrub, lineTiming, timingReport } from './diagnostics.ts'

test('keys and user names are removed', () => {
  assert.equal(scrub('auth sk-abc123456789 ok'), 'auth [key] ok')
  assert.equal(scrub('Authorization: Bearer abcdef'), 'Authorization: [key]')
  assert.equal(scrub('header Bearer abcdef'), 'header Bearer [key]')
  assert.equal(scrub('{"api_key":"secret"}'), '{"api_key":"[key]"}')
  assert.equal(scrub("loading '/home/alice/models/a.gguf'"), "loading '/home/[user]/models/a.gguf'")
  assert.equal(scrub('C:\\Users\\alice\\x.log'), 'C:\\Users\\[user]\\x.log')
})

test('log lines with meeting text are dropped', () => {
  const out = cleanLog('srv ready\nprompt: 豆包交给小王\nslot done')
  assert.equal(out, 'srv ready\nslot done')
})

test('recent errors are scrubbed and bounded', () => {
  for (let i = 0; i < 70; i++) recordError('t', `boom ${i} sk-abcdefghijkl`)
  const e = recentErrors()
  assert.equal(e.length, 60)
  assert.ok(e.at(-1)!.includes('boom 69 [key]'))
})

test('the zip has valid structure and the right crc', () => {
  const z = makeZip([{ name: 'a.txt', text: 'hello' }])
  const v = new DataView(z.buffer)
  assert.equal(v.getUint32(0, true), 0x04034b50)
  // 「hello」的 CRC32 是公认值
  assert.equal(v.getUint32(14, true), 0x3610a686)
  assert.equal(v.getUint32(z.length - 22, true), 0x06054b50)
  assert.equal(v.getUint16(z.length - 12, true), 1)
})

test('timing report: each stage and end of speech to board, numbers only', () => {
  const heard = (waitMs: number, asrMs: number, soft = false) => ({ waitMs, asrMs, voiceprintMs: 20, soft })
  const lines = [
    { text: 'secret one', speaker: 'A', ms: 800, queueMs: 0, heard: heard(450, 60) },
    { text: 'secret two', speaker: 'B', ms: 900, queueMs: 700, heard: heard(130, 80, true) },
    { text: 'typed', ms: 1000, queueMs: 0 },
    { text: 'moved a box', manual: true, ms: 0 },
    { text: 'still drawing' },
  ]
  const r = timingReport(lines, 'Built-in')
  assert.equal(r.lines, 3)
  assert.equal(r.spoken, 2)
  assert.equal(r.softBreaks, 1)
  assert.deepEqual(r.waitMs, { n: 2, p50: 450, p95: 450, max: 450 })
  assert.equal(r.modelMs?.n, 3)
  // 450 + 60 + 20 + 0 + 800 and 130 + 80 + 20 + 700 + 900
  assert.deepEqual(r.totalMs, { n: 2, p50: 1830, p95: 1830, max: 1830 })
  assert.ok(!JSON.stringify(r).includes('secret'))
  assert.deepEqual(lineTiming(lines[0]), { wait: 450, asr: 60, voiceprint: 20, queue: 0, model: 800 })
})
