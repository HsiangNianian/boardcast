import assert from 'node:assert/strict'
import { test } from 'node:test'
import { hostOf, isLoopback, needsConsent, rememberConsent } from '../privacy.ts'

test('local addresses are not cloud', () => {
  for (const u of ['http://127.0.0.1:1234/v1', 'http://127.1:1234', 'http://127.255.255.255:1234', 'http://localhost:8000', 'http://[::1]:8080/v1', 'http://lm.localhost/v1'])
    assert.ok(isLoopback(u), u)
  for (const u of ['https://api.deepseek.com/v1', 'http://192.168.1.10:8000/v1', 'http://127.boardcast.test/v1', 'http://127.0.0.1.example/v1', 'http://localhost.example/v1', 'ftp://localhost/v1', 'file://localhost/v1', 'not a url']) assert.ok(!isLoopback(u), u)
})

test('consent is asked once per host', () => {
  const store = new Map<string, string>()
  globalThis.localStorage = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
  } as Storage
  assert.equal(hostOf('https://API.deepseek.com/v1/chat/completions'), 'api.deepseek.com')
  assert.ok(needsConsent('https://api.deepseek.com/v1'))
  rememberConsent('https://api.deepseek.com/v1')
  assert.ok(!needsConsent('https://api.deepseek.com/beta'))
  assert.ok(needsConsent('https://api.openai.com/v1'))
  assert.ok(!needsConsent('http://127.0.0.1:1234/v1'))
  assert.ok(needsConsent('http://127.boardcast.test/v1'))
})
