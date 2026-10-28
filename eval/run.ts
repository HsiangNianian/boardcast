// 离线评测：把合成会议逐句喂给引擎，比较最终板书和标注，统计每句的延迟。
//
//   node eval/run.ts [--base-url http://127.0.0.1:8080] [--label qwen3.5-2b] [--only login-flow]
//
// 需要先起一个 llama-server（见 README）。结果写到 eval/reports/ 和 GitHub Actions 的 job summary。

import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { parseArgs } from 'node:util'
import { describe } from '../src/engine/graph.ts'
import { formatOps } from '../src/engine/ops.ts'
import type { Utterance } from '../src/engine/prompt.ts'
import { Session } from '../src/engine/session.ts'
import { type Gold, f1, score } from './score.ts'

const { values: args } = parseArgs({
  options: {
    'base-url': { type: 'string', default: 'http://127.0.0.1:8080' },
    label: { type: 'string', default: 'local' },
    only: { type: 'string' },
    verbose: { type: 'boolean', short: 'v', default: false },
  },
})

interface Case {
  id: string
  name: string
  turns: Utterance[]
  gold: Gold
}

const cases = (JSON.parse(readFileSync(new URL('./cases.json', import.meta.url), 'utf8')) as Case[]).filter(
  (c) => !args.only || args.only.split(',').includes(c.id),
)

function pct(xs: number[], p: number): number {
  if (!xs.length) return NaN
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))]
}

const latencies: number[] = []
const prefill: number[] = []
const outTokens: number[] = []
const rows: string[] = []
const details: string[] = []
const metric = { node: [] as number[], edge: [] as number[], topic: [] as number[], item: [] as number[], mark: [] as number[], group: [] as number[] }
let totalViolations = 0

// 先打一枪暖机，把系统提示词放进 KV 缓存，第一句的延迟才有代表性
{
  const warm = new Session({ baseUrl: args['base-url']!, noThink: true })
  warm.say({ text: '大家好。' })
  await warm.drain()
}

for (const c of cases) {
  const turnMs: number[] = []
  const log: string[] = []
  const s = new Session(
    { baseUrl: args['base-url']!, noThink: true },
    {
      llm: (r) => {
        turnMs.push(r.ms)
        latencies.push(r.ms)
        if (r.promptEvaluated !== undefined) prefill.push(r.promptEvaluated)
        if (r.outputTokens !== undefined) outTokens.push(r.outputTokens)
        log.push(
          `> ${r.batch.map((u) => u.text).join(' ')}\n` +
            `${formatOps(r.ops) || '-'}\n` +
            `  (${r.ms.toFixed(0)} ms, 读入 ${r.promptEvaluated ?? '?'}/${r.promptTokens ?? '?'} tok, 输出 ${r.outputTokens ?? '?'} tok)`,
        )
      },
      error: (e) => log.push(`! ${String(e)}`),
    },
  )
  for (const t of c.turns) {
    s.say(t)
    // 评测时逐句等待，测的是单句延迟，不让排队合并掩盖它
    await s.drain()
  }
  const sc = score(s.graph, c.gold)
  const cell = (v: number | null) => (v === null ? '' : v.toFixed(2))
  const nf = c.gold.nodes?.length ? f1(sc.nodeP, sc.nodeR) : null
  const ef = c.gold.edges?.length ? f1(sc.edgeP, sc.edgeR) : null
  const itf = sc.itemP !== null && sc.itemR !== null ? f1(sc.itemP, sc.itemR) : null
  const put = (k: keyof typeof metric, v: number | null) => v !== null && metric[k].push(v)
  put('node', nf)
  put('edge', ef)
  put('topic', sc.topicR)
  put('item', itf)
  put('mark', sc.markAcc)
  put('group', sc.groupAcc)
  totalViolations += sc.violations.length
  rows.push(
    `| ${c.id} | ${cell(nf)} | ${cell(ef)} | ${cell(sc.topicR)} | ${cell(itf)} | ${cell(sc.markAcc)} | ${cell(sc.groupAcc)} | ${sc.violations.length} | ${pct(turnMs, 50).toFixed(0)} |`,
  )
  const miss = [
    sc.missing.length && `缺：${sc.missing.join('、')}`,
    sc.extra.length && `多：${sc.extra.join('、')}`,
    ...sc.violations,
  ].filter(Boolean)
  details.push(
    `### ${c.id} · ${c.name}\n\n${miss.length ? miss.map((m) => `- ${m}`).join('\n') : '- 全对'}\n\n` +
      `<details><summary>逐句操作与最终板书</summary>\n\n\`\`\`\n${log.join('\n')}\n\n${describe(s.graph)}\n\`\`\`\n</details>\n`,
  )
  if (args.verbose) console.error(`${c.id}\n${log.join('\n')}\n${describe(s.graph)}\n`)
  console.error(
    `${c.id}: 节点 ${cell(nf) || '-'}，连线 ${cell(ef) || '-'}，话题 ${cell(sc.topicR) || '-'}，要点 ${cell(itf) || '-'}，标记 ${cell(sc.markAcc) || '-'}，违规 ${sc.violations.length}`,
  )
}

const avg = (xs: number[]) => (xs.length ? (xs.reduce((a, b) => a + b, 0) / xs.length).toFixed(3) + ` (${xs.length} 段)` : '-')
const summary = [
  `## 板书评测 · ${args.label}`,
  '',
  `| 指标 | 值 |`,
  `|---|---|`,
  `| 节点 F1 | ${avg(metric.node)} |`,
  `| 连线 F1 | ${avg(metric.edge)} |`,
  `| 话题召回 | ${avg(metric.topic)} |`,
  `| 要点 F1（条目挂在对的话题下） | ${avg(metric.item)} |`,
  `| 标记命中（划掉、强调、问题、公式） | ${avg(metric.mark)} |`,
  `| 分组正确率 | ${avg(metric.group)} |`,
  `| 违规（该删没删、纠正没生效） | ${totalViolations} |`,
  `| 单句延迟 p50 / p95 | ${pct(latencies, 50).toFixed(0)} / ${pct(latencies, 95).toFixed(0)} ms |`,
  `| 每句实际读入 token p50（扣掉前缀缓存） | ${pct(prefill, 50)} |`,
  `| 每句输出 token p50 | ${pct(outTokens, 50)} |`,
  '',
  '| 用例 | 节点 | 连线 | 话题 | 要点 | 标记 | 分组 | 违规 | 延迟 p50 ms |',
  '|---|---|---|---|---|---|---|---|---|',
  ...rows,
  '',
  ...details,
].join('\n')

mkdirSync(new URL('./reports/', import.meta.url), { recursive: true })
const stamp = new Date().toISOString().replace(/[:.]/g, '-')
writeFileSync(new URL(`./reports/${args.label}-${stamp}.md`, import.meta.url), summary)
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary + '\n')
console.log(summary.split('\n').slice(0, 18 + cases.length).join('\n'))
