import assert from 'node:assert/strict'
import { test } from 'node:test'
import { langChoice, setLangChoice } from '../i18n/index.ts'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('native transport errors give localized recovery advice without changing unknown failures', async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, 'window')
  const previousLanguage = langChoice()
  let failure: unknown
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: async () => {
      if (failure !== undefined) throw failure
      return { ms: 1, reply: 'OK' }
    } },
  } })
  try {
    const { remote } = await import('../remote.ts')
    for (const language of ['en', 'zh'] as const) {
      setLangChoice(language, false)
      for (const [code, english, chinese] of [
        ['REMOTE_TLS_CERTIFICATE', 'certificate', '证书'],
        ['REMOTE_PROXY_AUTH', 'proxy username and password', '代理用户名和密码'],
        ['REMOTE_PROXY_CONNECT', 'proxy settings', '代理设置'],
      ]) {
        failure = code
        await assert.rejects(remote.test(), error => {
          assert.ok(error instanceof Error)
          assert.ok(error.message.includes(language === 'en' ? english : chinese), error.message)
          assert.ok(!error.message.includes(code))
          return true
        })
      }
    }
    for (const unknown of ['Save failed: Permission denied', new Error('Unknown transport failure')]) {
      failure = unknown
      await assert.rejects(remote.test(), error => error === unknown)
    }
    failure = undefined
    assert.deepEqual(await remote.test(), { ms: 1, reply: 'OK' })
  } finally {
    setLangChoice(previousLanguage, false)
    if (previousWindow) Object.defineProperty(globalThis, 'window', previousWindow)
    else Reflect.deleteProperty(globalThis, 'window')
  }
})

test('native cancellation covers registration races and only cancels the requested chat', async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, 'window')
  let invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (command: string, args?: Record<string, unknown>) => invoke(command, args) },
  } })
  try {
    const { remote } = await import('../remote.ts')
    const reason = new Error('User canceled this request')
    invoke = async () => { assert.fail('a pre-aborted request must not invoke native code') }
    await assert.rejects(remote.send({}, AbortSignal.abort(reason)), error => error === reason)

    const early = new AbortController()
    const registration = deferred<string>()
    const canceled: unknown[] = []
    invoke = async (command, args) => {
      if (command === 'remote_prepare') return registration.promise
      assert.equal(command, 'remote_cancel', 'abort during registration must not send chat content')
      canceled.push(args?.requestId)
    }
    const beforeStart = remote.send({}, early.signal)
    early.abort(reason)
    registration.resolve('prepared')
    await assert.rejects(beforeStart, error => error === reason)
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(canceled, ['prepared'])

    let next = 0
    const pending = new Map<string, ReturnType<typeof deferred<unknown>>>()
    invoke = async (command, args) => {
      if (command === 'remote_prepare') return String(++next)
      const id = String(args?.requestId)
      if (command === 'remote_cancel') { pending.get(id)?.reject('Request canceled'); pending.delete(id); return }
      assert.equal(command, 'remote_chat')
      const request = deferred<unknown>()
      pending.set(id, request)
      return request.promise
    }
    const controller = new AbortController()
    const first = remote.send({ messages: ['summary'] }, controller.signal)
    const second = remote.send({ messages: ['board'] })
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(pending.size, 2)
    controller.abort(reason)
    await assert.rejects(first, error => error === reason)
    assert.ok(pending.has('2'), 'canceling summary must leave the board request running')
    const reply = { choices: [{ message: { content: 'kept' } }] }
    pending.get('2')!.resolve(reply)
    assert.deepEqual(await second, reply)
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(pending.size, 0)

    const late = new AbortController()
    invoke = async command => {
      if (command === 'remote_prepare') return 'late'
      if (command === 'remote_cancel') return
      late.abort(reason)
      return reply
    }
    await assert.rejects(remote.send({}, late.signal), error => error === reason)
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, 'window', previousWindow)
    else Reflect.deleteProperty(globalThis, 'window')
  }
})
