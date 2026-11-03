import assert from 'node:assert/strict'
import { test } from 'node:test'
import { evaluatePhonetics } from '../../eval/phonetic.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { bindSpeechOps, resolveTargets, speechPlan } from './phonetic.ts'
import { correctUtterance, parseGlossary } from './glossary.ts'
import { Session } from './session.ts'
import { quickOps } from './rules.ts'

const board = (text: string) => applyOps(emptyGraph(), parseOps(text))

test('multilingual phonetic regression: matches, ambiguity, negation, aliases and new nouns', async () => {
  const results = await evaluatePhonetics()
  assert.deepEqual(results.flatMap((r) => r.failures.map((f) => `${r.id}: ${f}`)), [])
})

test('real command path marks existing nodes, preserving raw speech and aliases, without calling the model', async () => {
  let calls = 0
  const s = new Session({ baseUrl: 'http://unused', send: async () => { calls++; throw new Error('must not call model') } })
  s.graph = board('N 「cache」')
  s.say({ id: 'audio:1', text: 'circle cash' })
  await s.drain()
  assert.equal(calls, 0)
  assert.deepEqual(s.graph.nodes[0].marks, ['圈出'])
  assert.deepEqual(s.graph.nodes[0].aliases, [])
  assert.equal(s.history[0].text, 'circle cash')
})

test('agent receives candidates and original text; wrong reference reuses the existing node', async () => {
  let sent = ''
  const s = new Session({ baseUrl: 'http://unused', send: async (body) => {
    sent = (body.messages as { content: string }[]).at(-1)!.content
    return { choices: [{ message: { content: 'K 「健全」 「检查权限」' } }] }
  } })
  s.graph = board('N 「鉴权」')
  s.say({ text: '这里的健全要检查权限' })
  await s.drain()
  assert.match(sent, /"健全" → n1 "鉴权"/)
  assert.match(sent, /新的一句：这里的健全要检查权限/)
  assert.equal(s.graph.nodes.find((n) => n.label === '检查权限')?.parent, 'n1')
  assert.equal(s.graph.nodes.some((n) => n.label === '健全'), false)
})

test('new nouns and explicitly declared labels are never merged by sound', async () => {
  const g = board('N 「cache」\nN 「API」')
  for (const text of ['Introduce a new service called cash, connected to API', 'cash connects to API']) {
    const plan = await speechPlan(g, [], { text }, [])
    const original = parseOps('N 「cash」\nE 「cash」 -> n2')
    const ops = bindSpeechOps(original, plan, { text }, g)
    assert.deepEqual(ops, original)
    assert.equal(applyOps(g, ops).nodes.length, 3)
  }
  const next = { text: 'Introduce a new service called cash, connected to API' }
  const plan = await speechPlan(g, [], next, [])
  const implicit = parseOps('E 「cash」 -> n2')
  assert.deepEqual(bindSpeechOps(implicit, plan, next, g), implicit)
})

test('draft references can be repaired; another page and permanent aliases are untouched', async () => {
  const g = applyOps(board('N 「鉴权」\nN 「API」'), parseOps('E 「健全」 -> n2'), 'draft')
  const next = { text: '健全调用API' }
  const plan = await speechPlan(g, [], next, [])
  assert.deepEqual(bindSpeechOps(parseOps('E n3 -> n2'), plan, next, g), parseOps('E n1 -> n2'))
  assert.deepEqual(bindSpeechOps(parseOps('A n1 「健全」\nR n1 「新名字」'), plan, next, g), parseOps('A n1 「健全」\nR n1 「新名字」'))
  const other = await resolveTargets(emptyGraph(), ['健全'], 'zh')
  assert.equal(other[0].selected, undefined)
})

test('negation does not even create a temporary quick draft', () => {
  assert.deepEqual(quickOps('不要画一下 A 到 B'), [])
  assert.deepEqual(quickOps("don't draw A -> B"), [])
  assert.deepEqual(quickOps('A -> B を描かないで'), [])
})

test('glossary originals survive same-text revisions and stale phonetic actions are discarded', async () => {
  const terms = parseGlossary('cache = cash, kashe')
  const first = correctUtterance({ id: 'audio:1', revision: 0, text: 'circle cash' }, terms)
  const next = correctUtterance({ id: 'audio:1', revision: 1, text: 'circle kashe' }, terms)
  assert.equal(first.text, 'circle cache')
  assert.equal(first.rawText, 'circle cash')
  const s = new Session({ baseUrl: 'http://unused', send: async () => ({ choices: [{ message: { content: '-' } }] }) })
  s.graph = board('N 「cache」')
  s.say(first)
  await s.drain()
  s.say(next)
  assert.equal(s.history[0].revision, 1)
  assert.equal(s.history[0].rawText, 'circle kashe')
  const revised = new Session(s.llm)
  revised.graph = board('N 「cache」')
  revised.say({ id: 'audio:2', revision: 0, text: 'circle cash' })
  revised.say({ id: 'audio:2', revision: 1, text: "don't circle cash" })
  await revised.drain()
  assert.deepEqual(revised.graph.nodes[0].marks, [])
  assert.equal(revised.history.length, 1)
})
