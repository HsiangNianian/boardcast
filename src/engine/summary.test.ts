import assert from 'node:assert/strict'
import { test } from 'node:test'
import { toMarkdown } from '../board/markdown.ts'
import { emptyGraph } from './graph.ts'
import type { ChatResponse, LlmConfig } from './llm.ts'
import { applyOps, parseOps } from './ops.ts'
import { pageGraph } from './regions.ts'
import { boardsText, chunks, plainText, summarize, summaryMessages, tidySummary, transcript } from './summary.ts'
import { setLangChoice } from '../i18n/index.ts'

// These tests check the Chinese wording
setLangChoice('zh')

test('long transcripts are cut between sentences', () => {
  const lines = Array.from({ length: 10 }, (_, i) => ({ text: `${i}`.repeat(40) }))
  const parts = chunks(lines, 100)
  assert.equal(parts.flat().length, 10)
  assert.ok(parts.length > 1 && parts.every((p) => p.length >= 1 && p.length <= 3))
})

test('summary output is cleaned up and missing sections say none', () => {
  const t = tidySummary('好的，下面是摘要：\n```markdown\n### 结论\n- 先做登录\n### 待办\n- [ ] 写接口（负责人：小王）\n```')
  assert.match(t, /^### 结论\n- 先做登录/)
  assert.match(t, /### 决定\n- 无/)
  assert.match(t, /### 待讨论\n- 无/)
  assert.doesNotMatch(t, /```|好的/)
})

test('long meetings are summarized piece by piece, then merged with the board', async () => {
  const seen: string[] = []
  const done = '### 结论\n- 好\n### 决定\n- 无\n### 待办\n- 无\n### 待讨论\n- 无'
  const cfg: LlmConfig = {
    baseUrl: 'x',
    send: async (b): Promise<ChatResponse> => {
      const msgs = b.messages as { content: string }[]
      seen.push(msgs[1].content)
      const merge = msgs[1].content.startsWith('白板上的内容')
      return { choices: [{ message: { content: merge ? done : '- 要点' } }] }
    },
  }
  const g = applyOps(emptyGraph(), parseOps('T 「登录」'))
  const lines = Array.from({ length: 200 }, (_, i) => ({ text: `第${i}句，我们讨论一下登录接口的细节和时间安排` }))
  const r = await summarize(cfg, g, lines)
  assert.ok(r.calls >= 3)
  assert.match(seen.at(-1)!, /白板上的内容：\n.*登录/)
  assert.match(seen.at(-1)!, /逐段记下的要点/)
  assert.match(r.text, /^### 结论\n- 好/)
})

test('headings written other ways are recognized, and unrecognized text is kept as is', () => {
  const t = tidySummary('**结论**\n- 用 Go\n## 决定：\n- 上 Redis\n【待办事项】\n- [ ] 写爬虫（负责人：A）\n待讨论：\n- 热度公式')
  assert.equal(t, '### 结论\n- 用 Go\n### 决定\n- 上 Redis\n### 待办\n- [ ] 写爬虫（负责人：A）\n### 待讨论\n- 热度公式')
  assert.equal(tidySummary('这场会主要聊了爬虫。'), '这场会主要聊了爬虫。')
  assert.equal(tidySummary(''), '')
})

test('an empty answer from a thinking model is retried with more room, then reported', async () => {
  const asked: number[] = []
  const reply = (content: string | null): ChatResponse => ({
    choices: [{ message: { content, reasoning_content: '想了很久' }, finish_reason: 'length' }],
  })
  const g = emptyGraph()
  const lines = [{ text: '先做爬虫' }]
  const flaky: LlmConfig = {
    baseUrl: 'x',
    flavor: 'openai',
    send: async (b) => {
      asked.push(b.max_tokens as number)
      return reply(asked.length === 1 ? null : '### 结论\n- 先做爬虫')
    },
  }
  const r = await summarize(flaky, g, lines)
  assert.deepEqual(asked, [4096, 8192])
  assert.match(r.text, /先做爬虫/)

  const mute: LlmConfig = { baseUrl: 'x', flavor: 'openai', send: async () => reply('') }
  await assert.rejects(summarize(mute, g, lines), /token 都用在思考上/)
})

test('copying as plain text drops the markdown', () => {
  assert.equal(plainText('### 结论\n- **用 Go**\n\n\n### 待办\n- [ ] 写爬虫\n- [x] 定方案'), '结论\n• 用 Go\n\n待办\n☐ 写爬虫\n☑ 定方案')
  assert.equal(plainText('### 结论\n- 好\n### 决定\n- 无'), '结论\n• 好\n\n决定\n• 无')
})

test('a summary sees every board, marks where the boards change, and skips untouched source text', () => {
  const a = applyOps(emptyGraph(), parseOps('T 「爬虫」\nL 「爬虫」 「监控任务」'))
  const pdf = applyOps(
    pageGraph({ w: 800, h: 1000 }, [
      { text: '库存先锁后扣', x: 0, y: 0, w: 100, h: 20 },
      { text: '没人讨论的一段', x: 0, y: 40, w: 100, h: 20 },
    ]),
    parseOps('K n1 「超时要可配置」'),
  )
  const text = boardsText([a, pdf])
  assert.match(text, /^第 1 张板：\n.*爬虫[\s\S]*第 2 张板：\n[\s\S]*库存先锁后扣/)
  assert.match(text, /超时要可配置/)
  assert.doesNotMatch(text, /没人讨论的一段/)
  // 只有一张板时不分节
  assert.doesNotMatch(boardsText([a]), /张板/)
  const said = transcript([
    { speaker: 'A', text: '先说爬虫', page: 0 },
    { speaker: 'B', text: '再看文档', page: 1 },
  ])
  assert.equal(said, '【第 1 张板】\nA：先说爬虫\n【第 2 张板】\nB：再看文档')
  assert.equal(transcript([{ text: '一句' }]), '一句')
  const user = summaryMessages([a, pdf], '会议记录：…')[1].content
  assert.match(user, /第 2 张板/)
})

test('markdown export puts the summary right under the title', () => {
  const md = toMarkdown(emptyGraph(), [], '周会', new Date(2026, 9, 30, 9, 0), '### 结论\n- 好')
  assert.match(md, /^# 周会\n\n> 2026-10-30 09:00 · boardcast\n\n## 摘要\n\n### 结论\n- 好\n/)
})
