import assert from 'node:assert/strict'
import { test } from 'node:test'
import { toMarkdown } from '../board/markdown.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

test('markdown export: topics as numbered lists, flowchart as mermaid, transcript at the end', () => {
  let g = applyOps(
    emptyGraph(),
    parseOps(
      'T 「爬虫」\nL 「爬虫」 「监控任务」\nL 「监控任务」 「定时拉取」\nQ 「爬虫」 「要不要上 MQ」\nM 「监控任务」 星标\nE 「用户」 -> 「网关」 : 「请求」\nG 「入口」 n5\nF 「T_now − T_display」',
    ),
  )
  // 铅笔稿不导出
  g = applyOps(g, parseOps('N 「还没确认」'), 'draft')
  const md = toMarkdown(g, [{ speaker: 'A', text: '先把爬虫理一下' }], '会议板书', new Date(2026, 9, 29, 15, 4))
  assert.match(md, /^# 会议板书\n\n> 2026-10-29 15:04 · boardcast/)
  assert.match(md, /## 爬虫\n\n1\. 监控任务 ★\n   - a\. 定时拉取\n- ？要不要上 MQ/)
  assert.match(md, /```mermaid\nflowchart LR\n {2}subgraph g1\["入口"\]\n {4}n5\["用户"\]\n {2}end\n {2}n6\["网关"\]\n {2}n5 -->\|"请求"\| n6\n```/)
  assert.match(md, /## 待定\n\n- `T_now − T_display`/)
  assert.match(md, /## 会议记录\n\n- \*\*A\*\*：先把爬虫理一下/)
  assert.doesNotMatch(md, /还没确认/)
})
