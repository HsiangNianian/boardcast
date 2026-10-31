import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cleanLog, makeZip, recentErrors, recordError, scrub } from './diagnostics.ts'

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
