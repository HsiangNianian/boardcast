import assert from 'node:assert/strict'
import { test } from 'node:test'
import { emptyGraph } from './graph.ts'
import { Session } from './session.ts'
import { applyOps, parseOps } from './ops.ts'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
const response = (text: string) => ({ choices: [{ message: { content: text } }] })

test('a proposal waits for editing before reading names into its model context', async () => {
  const editing = deferred<void>()
  const waiting = deferred<void>()
  const requests: string[] = []
  const session = new Session({baseUrl: 'test', send: async body => {
    requests.push(JSON.stringify(body))
    return response('N 「Model topic」')
  }}, {beforeApply: () => { waiting.resolve(); return editing.promise }})
  session.graph = applyOps(emptyGraph(), parseOps('N 「Uncommitted draft」'))
  session.say({text: 'Add another topic'})
  await waiting.promise
  assert.equal(requests.length, 0)
  session.graph = applyOps(emptyGraph(), parseOps('N 「Committed name」'))
  editing.resolve()
  await session.drain()
  assert.equal(requests.length, 1)
  assert.ok(requests[0].includes('Committed name'))
  assert.ok(!requests[0].includes('Uncommitted draft'))
  assert.deepEqual(session.graph.nodes.map(n => n.label), ['Committed name', 'Model topic'])
})

test('cancelling a proposal during editing prevents a late request for the old meeting', async () => {
  const editing = deferred<void>()
  const waiting = deferred<void>()
  let hold = true
  let signal: AbortSignal | undefined
  const requests: string[] = []
  const session = new Session({baseUrl: 'test', send: async body => {
    requests.push(JSON.stringify(body))
    return response('N 「New topic」')
  }}, {beforeApply: current => {
    if (!hold) return Promise.resolve()
    signal = current; waiting.resolve(); return editing.promise
  }})
  session.say({text: 'Old meeting'})
  await waiting.promise
  session.cancel()
  assert.equal(signal?.aborted, true)
  hold = false
  session.graph = emptyGraph()
  session.say({text: 'New meeting'})
  await session.drain()
  editing.resolve()
  await new Promise(resolve => setTimeout(resolve, 0))
  assert.equal(requests.length, 1)
  assert.ok(!requests[0].includes('Old meeting'))
  assert.deepEqual(session.graph.nodes.map(n => n.label), ['New topic'])
  assert.deepEqual(session.history.map(u => u.text), ['New meeting'])
})

test('a completed model response waits for a direct edit and then applies to the current graph', async () => {
  const editing = deferred<void>()
  const waiting = deferred<void>()
  let updates = 0
  let requested = false
  const session = new Session({baseUrl: 'test', send: async () => { requested = true; return response('N 「Model topic」') }}, {
    beforeApply: () => { if (!requested) return Promise.resolve(); waiting.resolve(); return editing.promise },
    graph: () => { updates++ },
  })
  const line = {id: 'during-edit', text: 'An additional topic'}
  session.say(line)
  await waiting.promise
  assert.equal(updates, 0)
  assert.equal(session.isProcessing(line), true)
  session.graph = applyOps(session.graph, parseOps('N 「Manual topic」'))
  editing.resolve()
  await session.drain()
  assert.deepEqual(session.graph.nodes.map(n => n.label), ['Manual topic', 'Model topic'])
  assert.equal(updates, 1)
  assert.equal(session.isProcessing(line), false)
})

test('cancel while a model response waits for editing aborts the wait and fences a late release', async () => {
  const editing = deferred<void>()
  const waiting = deferred<void>()
  let signal: AbortSignal | undefined
  const completed: string[] = []
  let calls = 0
  let hold = true
  const session = new Session({baseUrl: 'test', send: async () => response(`N 「Topic ${++calls}」`)}, {
    beforeApply: (current) => {
      if (!calls || !hold) return Promise.resolve()
      signal = current; waiting.resolve(); return editing.promise
    },
    llm: r => { completed.push(r.said.text) },
  })
  session.say({text: 'Old meeting'})
  await waiting.promise
  session.cancel()
  assert.equal(signal?.aborted, true)
  hold = false
  session.graph = emptyGraph()
  session.say({text: 'New meeting'})
  await session.drain()
  editing.resolve()
  await new Promise(resolve => setTimeout(resolve, 0))
  assert.deepEqual(session.graph.nodes.map(n => n.label), ['Topic 2'])
  assert.deepEqual(completed, ['New meeting'])
  assert.deepEqual(session.history.map(u => u.text), ['New meeting'])
})

test('processing status follows the active revision, next batch and cancellation', async () => {
  const first = deferred<ReturnType<typeof response>>()
  const second = deferred<ReturnType<typeof response>>()
  const secondStarted = deferred<void>()
  let calls = 0
  const session = new Session({ baseUrl: 'test', send: () => {
    if (++calls === 1) return first.promise
    secondStarted.resolve()
    return second.promise
  } })
  const active = { id: 'active', text: 'First statement' }
  const queued = { id: 'queued', text: 'Next statement' }
  session.say(active)
  session.say(queued)
  assert.equal(session.isProcessing(active), true)
  assert.equal(session.isProcessing(queued), false)
  assert.equal(session.isProcessing({ ...active, revision: 1 }), false)
  assert.equal(session.isProcessing({}), false)
  first.resolve(response('-'))
  await secondStarted.promise
  assert.equal(session.isProcessing(active), false)
  assert.equal(session.isProcessing(queued), true)
  session.cancel()
  assert.equal(session.isProcessing(queued), false)
  second.resolve(response('N 「Late result」'))
  await session.drain()
  assert.equal(session.isProcessing(queued), false)
})

test('speaker rename updates in-flight and queued attribution without mutating caller-owned lines', async () => {
  const first = deferred<ReturnType<typeof response>>()
  const started = deferred<void>()
  const requests: string[] = []
  const batches: string[][] = []
  const session = new Session({ baseUrl: 'http://unused', send: (body) => {
    requests.push(JSON.stringify(body))
    started.resolve()
    return requests.length === 1 ? first.promise : Promise.resolve(response('N 「Second idea」'))
  } }, { llm: (r) => batches.push(r.batch.map((u) => u.speaker!)) })
  const caller = { id: 'first', text: 'First idea', speaker: 'Alice' }
  session.say(caller)
  await started.promise
  session.say({ id: 'second', text: 'Second idea', speaker: 'Alice' })
  session.renameSpeaker('Alice', 'Bob')
  first.resolve(response('N 「First idea」'))
  await session.drain()
  assert.equal(caller.speaker, 'Alice')
  assert.deepEqual(session.history.map((u) => u.speaker), ['Bob', 'Bob'])
  assert.deepEqual(session.graph.nodes.map((node) => node.by), ['Bob', 'Bob'])
  assert.deepEqual(batches, [['Bob'], ['Bob']])
  assert.ok(requests[1].includes('Bob'))
})

test('cancelled native completion cannot change a new graph, history, callbacks, or busy state', async () => {
  const old = deferred<ReturnType<typeof response>>()
  const started = deferred<void>()
  const current = deferred<ReturnType<typeof response>>()
  let calls = 0
  const completed: string[] = []
  const session = new Session({ baseUrl: 'http://unused', send: () => { started.resolve(); return ++calls === 1 ? old.promise : current.promise } }, {
    llm: (r) => completed.push(r.said.text),
  })
  session.say({ text: 'old meeting sentence' })
  await started.promise
  session.say({ text: 'old queued sentence' })
  const cancelled = session.cancel()
  assert.deepEqual(cancelled.map((u) => u.text), ['old meeting sentence', 'old queued sentence'])
  session.graph = emptyGraph()
  session.history = []
  session.say({ text: 'new meeting sentence' })
  old.resolve(response('N 「Wrong meeting」'))
  await new Promise((resolve) => setTimeout(resolve, 0))
  assert.equal(calls, 2)
  assert.equal(session.graph.nodes.length, 0)
  assert.equal(session.history.length, 0)
  assert.deepEqual(completed, [])
  current.resolve(response('N 「Correct meeting」'))
  await session.drain()
  assert.deepEqual(session.graph.nodes.map((n) => n.label), ['Correct meeting'])
  assert.deepEqual(session.history.map((u) => u.text), ['new meeting sentence'])
  assert.deepEqual(completed, ['new meeting sentence'])
})

test('late rejected native request is silent after cancel and does not append old history', async () => {
  const old = deferred<ReturnType<typeof response>>()
  const started = deferred<void>()
  const errors: unknown[] = []
  const session = new Session({ baseUrl: 'http://unused', send: () => { started.resolve(); return old.promise } }, { error: (e) => errors.push(e) })
  session.say({ text: 'old sentence' })
  await started.promise
  session.cancel()
  session.history = [{ text: 'restored meeting' }]
  old.reject(new Error('late native failure'))
  await new Promise((resolve) => setTimeout(resolve, 0))
  await session.drain()
  assert.deepEqual(errors, [])
  assert.deepEqual(session.history, [{ text: 'restored meeting' }])
})
