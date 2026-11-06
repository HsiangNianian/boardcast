import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cleanLog, diagnosticErrors, diagnosticJson, errorSummary, makeZip, recentErrors, recordError, scrub, lineTiming, timingReport } from './diagnostics.ts'

test('keys and user names are removed', () => {
  assert.equal(scrub('auth sk-abc123456789 ok'), 'auth [key] ok')
  assert.equal(scrub('Authorization: Bearer abcdef'), 'Authorization: [key]')
  assert.equal(scrub('header Bearer abcdef'), 'header Bearer [key]')
  assert.equal(scrub('{"api_key":"secret"}'), '{"api_key":"[key]"}')
  assert.equal(scrub("loading '/home/alice/models/a.gguf'"), "loading '/home/[user]/models/a.gguf'")
  assert.equal(scrub('C:\\Users\\alice\\x.log'), 'C:\\Users\\[user]\\x.log')
})

test('diagnostic logs retain numeric metrics without exporting free text in any language', () => {
  const input = [
    'llama_context: n_ctx = 4096',
    'prompt: PRIVATE-EN private acquisition plan',
    'PRIVATE-CONTINUATION has no prefix',
    'prompt: PRIVATE-ZH 豆包交给小王',
    '{"messages":[{"content":"PRIVATE-JSON"}]}',
    'slot print_timing: prompt eval time = 12.00 ms / 4 tokens',
    'slot print_timing: eval time = 18.00 ms / 3 tokens',
    'error: out of memory; prompt was PRIVATE-ERROR',
    'Authorization: Bearer private-token-without-a-known-prefix',
  ].join('\n')
  const out = cleanLog(input)
  assert.ok(!out.includes('PRIVATE-') && !out.includes('private-token'))
  assert.ok(!out.includes('豆包') && !out.includes('acquisition'))
  const data = JSON.parse(out)
  assert.deepEqual(data.contextTokens, [4096])
  assert.deepEqual(data.timings, [{ stage: 'prompt eval', ms: 12, tokens: 4 }, { stage: 'eval', ms: 18, tokens: 3 }])
  assert.ok(data.signals.includes('out-of-memory'))
  assert.equal(data.lines, 9)
})

test('exported errors preserve observed codes and source, never free-form response text', () => {
  const raw = 'HTTP 401: PRIVATE-RESPONSE with arbitrary credentials token123; Permission denied (os error 13)'
  assert.deepEqual(errorSummary(raw), { type: 'Error', signals: ['permission-denied'], httpStatus: [401], osError: [13] })
  recordError('voice', new SyntaxError('PRIVATE-SYNTAX input from a meeting'))
  recordError('console', raw)
  const errors = diagnosticErrors().slice(-2)
  assert.equal(errors[0].source, 'voice')
  assert.equal(errors[0].type, 'SyntaxError')
  assert.equal(errors[1].source, 'console')
  assert.deepEqual(errors[1].httpStatus, [401])
  assert.ok(!JSON.stringify(errors).includes('PRIVATE-') && !JSON.stringify(errors).includes('token123'))
  assert.ok(recentErrors().at(-1)!.includes('PRIVATE-RESPONSE'), 'local error details remain available separately from the exported summary')
})

test('storage errors distinguish full disks, denied access and browser quota', () => {
  for (const text of ["Couldn't save the image: No space left on device (os error 28)", 'ENOSPC', 'There is not enough space on the disk. (os error 112)']) {
    assert.deepEqual(errorSummary(text).signals, ['disk-full'])
  }
  for (const text of ['Permission denied (os error 13)', 'Access is denied. (os error 5)', 'EACCES']) {
    assert.deepEqual(errorSummary(text).signals, ['permission-denied'])
  }
  for (const text of ['QuotaExceededError', 'The quota has been exceeded.', 'Storage quota exceeded']) {
    assert.deepEqual(errorSummary(text).signals, ['storage-quota'])
  }
  for (const text of ['This meeting changed in another window.', 'ASSET_HASH_MISMATCH', 'HTTP 503: unavailable']) {
    assert.deepEqual(errorSummary(text).signals, [])
  }
})

test('diagnostic JSON summarizes free-form status errors and scrubs paths before escaping', () => {
  const text = diagnosticJson({ os: 'windows', voice: { error: 'HTTP 503 PRIVATE-VOICE', listening: false }, model: { message: 'PRIVATE-STATUS Permission denied', path: 'C:\\Users\\alice\\model.gguf' }, speechModels: { appleError: 'TimeoutError PRIVATE-APPLE' }, root: '/home/alice/models', api_key: 'private-credential-without-prefix', authorization: 'Bearer another-private-key' })
  const data = JSON.parse(text)
  assert.ok(!text.includes('PRIVATE-') && !text.includes('alice') && !text.includes('sk-private'))
  assert.deepEqual(data.voice.error.httpStatus, [503])
  assert.deepEqual(data.model.message.signals, ['permission-denied'])
  assert.equal(data.speechModels.appleError.type, 'TimeoutError')
  assert.equal(data.model.path, 'C:\\Users\\[user]\\model.gguf')
  assert.equal(data.voice.listening, false)
  assert.equal(data.api_key, '[key]')
  assert.equal(data.authorization, '[key]')
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
