import assert from 'node:assert/strict'
import { test } from 'node:test'
import { msg } from '../i18n/index.ts'

test('desktop warm-up, board, summary and vision use native requests even when WebView fetch is blocked', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window')
  const fetch = globalThis.fetch
  const calls: { command: string; args?: Record<string, unknown> }[] = []
  let failure: unknown
  let next = 0
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: async (command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args })
      if (command === 'remote_prepare') return String(++next)
      if (command === 'remote_cancel') return
      assert.equal(command, 'llm_chat')
      if (failure) throw failure
      return { choices: [{ message: { content: 'Native reply' } }] }
    } },
  } })
  globalThis.fetch = async () => { throw new TypeError('Failed to fetch') }
  try {
    const { localChat } = await import('../desktop.ts')
    const { chat, proposeOps } = await import('./llm.ts')
    const { emptyGraph } = await import('./graph.ts')
    const { llamaReader } = await import('../board/importImage.ts')
    const baseUrl = 'http://127.0.0.1:63788'
    const cfg = { baseUrl, noThink: true }
    await proposeOps(cfg, emptyGraph(), [], { text: '大家好。' })
    await proposeOps({ ...cfg, send: (body, signal) => localChat(baseUrl, body, signal) }, emptyGraph(), [], { text: '模型连接测试。' })
    assert.equal((await chat(cfg, [{ role: 'user', content: 'Summarize synthetic notes.' }], 20)).text, 'Native reply')
    assert.equal(await llamaReader(baseUrl)('data:image/png;base64,synthetic'), 'Native reply')
    const requests = calls.filter(call => call.command === 'llm_chat')
    assert.equal(requests.length, 4)
    assert.ok(requests.every(call => call.args?.baseUrl === baseUrl && call.args.requestId))
    assert.ok((requests[0].args?.body as Record<string, unknown>).grammar)
    assert.match(JSON.stringify(requests[3].args?.body), /data:image\/png;base64,synthetic/)
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(calls.filter(call => call.command === 'remote_cancel').map(call => call.args?.requestId), ['1', '2', '3', '4'])

    const count = calls.length
    const reason = new Error('Canceled before sending')
    await assert.rejects(localChat(baseUrl, {}, AbortSignal.abort(reason)), error => error === reason)
    await assert.rejects(localChat('https://external.invalid', {}))
    assert.equal(calls.length, count, 'aborted and non-local requests never reach the shell')
    failure = 'LOCAL_MODEL_UNAVAILABLE'
    await assert.rejects(localChat(baseUrl, {}), error => error instanceof Error && error.message === msg().app.builtinUnavailable)
    failure = 'HTTP 503: GPU inference failed'
    await assert.rejects(localChat(baseUrl, {}), error => error === failure)
    failure = 'HTTP 400: missing mmproj vision component'
    await assert.rejects(llamaReader(baseUrl)('data:image/png;base64,synthetic'), error => error instanceof Error && error.message === msg().errors.needVision)
  } finally {
    globalThis.fetch = fetch
    if (previous) Object.defineProperty(globalThis, 'window', previous)
    else Reflect.deleteProperty(globalThis, 'window')
  }
})
