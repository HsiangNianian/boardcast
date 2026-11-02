import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Session } from './session.ts'
import { applyOps, parseOps } from './ops.ts'
import { emptyGraph } from './graph.ts'
import { settle } from './pending.ts'
import type { Utterance } from './prompt.ts'

test('an in-flight audio revision discards stale operations and does not duplicate speech', async () => {
  let release = () => {}
  const gate = new Promise<void>((r) => { release = r })
  const sent: string[] = [], applied: Utterance[] = []
  const s = new Session({ baseUrl: 'http://fake', send: async (body) => {
    sent.push((body.messages as { content: string }[]).at(-1)!.content)
    const first = sent.length === 1
    if (first) await gate
    return { choices: [{ message: { content: first ? 'N 「错误」' : 'N 「正确」' } }] }
  } }, { llm: (r) => applied.push(...r.batch) })
  s.say({ id: 'capture:1', revision: 0, speaker: 'A', text: '初始转写' })
  s.say({ id: 'capture:1', revision: 1, speaker: 'A', text: '正确转写' })
  s.say({ id: 'capture:1', revision: 0, text: '过时消息' })
  release()
  await s.drain()
  assert.equal(s.history.length, 1)
  assert.equal(s.history[0].text, '正确转写')
  assert.deepEqual(applied.map((u) => u.revision), [1])
  assert.deepEqual(s.graph.nodes.map((n) => n.label), ['正确'])
  assert.equal(sent.length, 2)
})

test('queued revisions replace the pending line and keep adjacent speech', async () => {
  let release = () => {}
  const gate = new Promise<void>((r) => { release = r })
  let calls = 0
  const s = new Session({ baseUrl: 'http://fake', send: async () => {
    if (++calls === 1) await gate
    return { choices: [{ message: { content: '-' } }] }
  } })
  s.say({ text: 'blocking line' })
  s.say({ id: '1', revision: 0, text: 'wrong' })
  s.say({ id: '1', revision: 1, text: 'right' })
  s.say({ id: '2', revision: 0, text: 'next line' })
  release()
  await s.drain()
  assert.deepEqual(s.history.map((u) => u.text), ['blocking line', 'right', 'next line'])
})

test('an already drawn revision instructs the agent to correct the same board and preserves manual content', async () => {
  const sent: string[] = []
  const s = new Session({ baseUrl: 'http://fake', send: async (body) => {
    sent.push((body.messages as { content: string }[]).at(-1)!.content)
    return { choices: [{ message: { content: '-' } }] }
  } })
  s.graph = applyOps(emptyGraph(), parseOps('N 「手动内容」'))
  s.history = [{ id: '1', revision: 0, speaker: 'A', text: '旧转写' }]
  s.say({ id: '1', revision: 1, speaker: 'A', text: '新转写' })
  await s.drain()
  assert.match(sent[0], /同一句话，不是新发言/)
  assert.match(sent[0], /原转写：旧转写/)
  assert.match(sent[0], /正确转写：A：新转写/)
  assert.equal(s.history.length, 1)
  assert.equal(s.history[0].replaces, undefined)
  assert.equal(s.history[0].text, '新转写')
  assert.equal(s.graph.nodes[0].label, '手动内容')
  s.say({ id: '1', revision: 1, text: 'duplicate event' })
  await s.drain()
  assert.equal(sent.length, 1)
  s.say({ id: '1', revision: 3, text: '新转写' })
  s.say({ id: '1', revision: 2, text: 'late older revision' })
  await s.drain()
  assert.equal(s.history[0].revision, 3)
  assert.equal(s.history[0].text, '新转写')
  assert.equal(sent.length, 1)
})

test('settlement matches segment and revision, even with identical spoken text', () => {
  const lines: (Utterance & { ms?: number })[] = [{ id: '1', revision: 1, text: 'okay' }, { id: '2', revision: 0, text: 'okay' }]
  const old = settle(lines, [{ id: '1', revision: 0, text: 'okay' }], () => ({ ms: 10 }))
  assert.deepEqual(old, lines)
  const current = settle(lines, [{ id: '2', revision: 0, text: 'okay' }], () => ({ ms: 20 }))
  assert.deepEqual(current, [lines[0], { ...lines[1], ms: 20 }])
})
