import assert from 'node:assert/strict'
import { test } from 'node:test'
import '../i18n/index.ts'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('canceling an arXiv download covers registration and late replies without canceling a chat', async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, 'window')
  let invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (command: string, args?: Record<string, unknown>) => invoke(command, args) },
  } })
  try {
    const { arxivPdf } = await import('../desktop.ts')
    const { remote } = await import('../remote.ts')
    const reason = new Error('Import canceled')
    invoke = async () => { assert.fail('pre-aborted import must not reach native code') }
    await assert.rejects(arxivPdf('1706.03762v7', AbortSignal.abort(reason)), error => error === reason)

    const registration = deferred<string>()
    const early = new AbortController()
    const canceled: unknown[] = []
    invoke = async (command, args) => {
      if (command === 'remote_prepare') return registration.promise
      assert.equal(command, 'remote_cancel')
      canceled.push(args?.requestId)
    }
    const beforeStart = arxivPdf('1706.03762v7', early.signal)
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
      assert.ok(command === 'arxiv_pdf' || command === 'remote_chat')
      if (command === 'arxiv_pdf') assert.equal(args?.id, '1706.03762v7')
      const request = deferred<unknown>()
      pending.set(id, request)
      return request.promise
    }
    const controller = new AbortController()
    const download = arxivPdf('1706.03762v7', controller.signal)
    const chat = remote.send({ messages: ['kept board request'] })
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(pending.size, 2)
    controller.abort(reason)
    await assert.rejects(download, error => error === reason)
    assert.ok(pending.has('2'))
    const reply = { choices: [{ message: { content: 'kept' } }] }
    pending.get('2')!.resolve(reply)
    assert.deepEqual(await chat, reply)
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(pending.size, 0)

    const pdf = new Uint8Array([37, 80, 68, 70, 45, 49, 46, 55, 10, 0, 128, 255])
    const late = new AbortController()
    invoke = async command => {
      if (command === 'remote_prepare') return 'late'
      if (command === 'remote_cancel') return
      late.abort(reason)
      return Buffer.from(pdf).toString('base64')
    }
    await assert.rejects(arxivPdf('1706.03762v7', late.signal), error => error === reason)
    invoke = async command => {
      if (command === 'remote_prepare') return 'retry'
      if (command === 'remote_cancel') return
      return Buffer.from(pdf).toString('base64')
    }
    assert.deepEqual(await arxivPdf('1706.03762v7'), pdf)
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, 'window', previousWindow)
    else Reflect.deleteProperty(globalThis, 'window')
  }
})
