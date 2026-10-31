import assert from 'node:assert/strict'
import { test } from 'node:test'
import { spokenLang } from './lang.ts'
import { boardLang, grammar, userMessage } from './prompt.ts'
import { emptyGraph } from './graph.ts'
import { summaryMessages, tidySummary, transcript } from './summary.ts'

test('meeting language: English only when clearly English', () => {
  assert.equal(spokenLang(['The login service calls the user database.']), 'en')
  assert.equal(spokenLang(["Let's ask 小王 about the API rate limits."]), 'en')
  assert.equal(spokenLang(['graphRAG 这块要接 ClickHouse']), 'zh')
  assert.equal(spokenLang(['用户先打开小程序，小程序去调登录接口。']), 'zh')
  assert.equal(spokenLang([]), 'zh')
  assert.equal(spokenLang(['123 456']), 'zh')
})

test('English meetings ask for an English board; Chinese prompts are unchanged', () => {
  const g = emptyGraph()
  const en = userMessage(g, [], { text: 'First the user opens the app, then it calls the login API.' })
  assert.match(en, /板上的名字、条目、问题、备注都用英文写/)
  const zh = userMessage(g, [], { text: '用户先打开小程序，小程序去调登录接口。' })
  assert.doesNotMatch(zh, /英文/)
  assert.equal(boardLang([{ text: 'We need three things.' }], { text: '好' }), 'en')
  assert.match(grammar(g, 8, 'en'), /\{1,48\} "」"$/)
  assert.match(grammar(g), /\{1,20\} "」"$/)
})

test('English summaries use English instructions and headings', () => {
  const [system, user] = summaryMessages([emptyGraph()], 'Transcript:\nA: ship on Friday', false, 'en')
  assert.match(system.content, /Write in English/)
  assert.match(system.content, /### Conclusions\n### Decisions\n### To-dos\n### Open questions$/)
  assert.match(user.content, /^On the whiteboard:/)
  assert.equal(transcript([{ speaker: 'A', text: 'hi', page: 0 }, { text: 'yo', page: 1 }], 'en'), '[Board 1]\nA: hi\n[Board 2]\nyo')
})

test('English summary headings are tidied and missing ones filled with None', () => {
  const t = tidySummary('Sure! Here it is:\n**Conclusions:**\n- ship Friday\n## Action items\n- [ ] write docs (owner: Ana)', 'en')
  assert.equal(t, '### Conclusions\n- ship Friday\n### To-dos\n- [ ] write docs (owner: Ana)\n\n### Decisions\n- None\n\n### Open questions\n- None')
})
