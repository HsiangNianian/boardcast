import assert from 'node:assert/strict'
import { test } from 'node:test'
import { toMarkdown } from '../board/markdown.ts'
import { emptyGraph } from './graph.ts'
import type { ChatResponse, LlmConfig } from './llm.ts'
import { applyOps, parseOps } from './ops.ts'
import { chunks, summarize, tidySummary } from './summary.ts'

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

test('markdown export puts the summary right under the title', () => {
  const md = toMarkdown(emptyGraph(), [], '周会', new Date(2026, 9, 30, 9, 0), '### 结论\n- 好')
  assert.match(md, /^# 周会\n\n> 2026-10-30 09:00 · boardcast\n\n## 摘要\n\n### 结论\n- 好\n/)
})
