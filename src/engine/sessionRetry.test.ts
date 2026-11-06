import assert from 'node:assert/strict'
import { test } from 'node:test'
import { BoardModelUnavailable, Session } from './session.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

const response = (content: string) => ({ choices: [{ message: { content } }] })
const tick = () => new Promise((yes) => setTimeout(yes, 0))
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('failed → retry keeps the same utterance, pointing and a single graph result', async () => {
  let calls = 0
  const batches: unknown[] = []
  const session = new Session({ baseUrl: 'test', send: async () => {
    if (++calls === 1) throw new Error('offline')
    return response('N 「Recovered」')
  } }, { llm: (r) => batches.push(r.batch) })
  const line = { id: 'speech-1', revision: 0, text: 'A statement to recover', pointing: ['n7'], page: 2 }
  session.say(line)
  await session.drain()
  assert.equal(session.retry(line), true)
  await session.drain()
  assert.equal(calls, 2)
  assert.deepEqual(session.history, [line])
  assert.deepEqual(batches, [[line]])
  assert.deepEqual(session.graph.nodes.map((n) => n.label), ['Recovered'])
  assert.equal(session.retry({ ...line, failed: true }), false, 'a stale failed UI record cannot replay a successful revision')
  assert.equal(calls, 2)
})

test('rapid retry clicks enqueue once; another failure remains retryable', async () => {
  const attempt = deferred<ReturnType<typeof response>>()
  let calls = 0
  const session = new Session({ baseUrl: 'test', send: async () => {
    calls++
    if (calls === 1) throw new Error('first failure')
    if (calls === 2) return attempt.promise
    return response('N 「Recovered once」')
  } })
  const line = { id: 'speech-2', text: 'Please retain this line' }
  session.say(line)
  await session.drain()
  assert.equal(session.retry(line), true)
  assert.equal(session.retry(line), false)
  assert.equal(session.retry({ ...line, failed: true }), false)
  await tick()
  assert.equal(calls, 2)
  attempt.reject(new Error('second failure'))
  await session.drain()
  assert.equal(session.retry(line), true)
  await session.drain()
  assert.equal(calls, 3)
  assert.equal(session.history.length, 1)
  assert.equal(session.graph.nodes.length, 1)
})

test('a saved failure retries on its original page without touching the displayed page', async () => {
  const pages = [applyOps(emptyGraph(), parseOps('N 「Source」')), applyOps(emptyGraph(), parseOps('N 「Other page」'))]
  const other = structuredClone(pages[1])
  const line = { id: 'saved-failure', text: 'Annotate this original node', page: 0, pointing: ['n1'], failed: true }
  const background = new Session({ baseUrl: 'test', send: async () => response('K n1 「Recovered note」') }, { graph: (graph) => { pages[0] = graph } })
  background.graph = pages[0]
  background.history = [line]
  assert.equal(background.retry(line), true)
  await background.drain()
  assert.deepEqual(pages[1], other)
  assert.equal(pages[0].nodes.filter((n) => n.label === 'Recovered note').length, 1)
  assert.equal(background.history.length, 1)
  assert.deepEqual(background.history[0].pointing, ['n1'])
})

test('no model settles immediately without fetch, removes drafts, and can recover later', async () => {
  const errors: unknown[] = []
  let calls = 0
  const original = globalThis.fetch
  globalThis.fetch = async () => { calls++; throw new Error('empty URL must not be requested') }
  try {
    const session = new Session({ baseUrl: '' }, { error: (error) => errors.push(error) })
    const line = { id: 'unconfigured', text: '先说用户，再到数据库' }
    session.say(line)
    await session.drain()
    assert.equal(calls, 0)
    assert.ok(errors[0] instanceof BoardModelUnavailable)
    assert.equal(session.graph.nodes.length, 0)
    session.llm = { baseUrl: 'test', send: async () => response('N 「Available now」') }
    assert.equal(session.retry(line), true)
    await session.drain()
    assert.equal(session.history.length, 1)
    assert.deepEqual(session.graph.nodes.map((n) => n.label), ['Available now'])
  } finally { globalThis.fetch = original }
})

test('cancel fences a late retry result and its finally from a new meeting', async () => {
  const late = deferred<ReturnType<typeof response>>()
  let calls = 0
  const completed: string[] = []
  const session = new Session({ baseUrl: 'test', send: async () => {
    if (++calls === 1) throw new Error('offline')
    if (calls === 2) return late.promise
    return response('N 「New meeting」')
  } }, { llm: (r) => completed.push(r.said.text) })
  const line = { id: 'old', text: 'Old retry' }
  session.say(line); await session.drain()
  assert.equal(session.retry(line), true)
  await tick()
  session.cancel()
  session.graph = emptyGraph(); session.history = []
  session.say({ id: 'new', text: 'New statement' })
  late.resolve(response('N 「Wrong old board」'))
  await session.drain()
  assert.deepEqual(completed, ['New statement'])
  assert.deepEqual(session.graph.nodes.map((n) => n.label), ['New meeting'])
})
