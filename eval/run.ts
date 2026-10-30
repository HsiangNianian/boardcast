// 离线评测：把合成会议逐句喂给引擎，比较最终板书和标注，统计每句的延迟。
//
//   node eval/run.ts [--base-url http://127.0.0.1:8080] [--label qwen3.5-4b] [--set dev|holdout|all] [--repeat 3] [--only login-flow]
//
// 两套用例：
// - dev（eval/cases.json）：调提示词时用，分数偏乐观；
// - holdout（eval/holdout.json）：不参与调参，只在发版前、换模型时看。看了分数不要回头改提示词去迁就它。
// --repeat N：每段跑 N 遍，报告平均值和波动（标准差），结果差得多的用例标「不稳定」。
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
    set: { type: 'string', default: 'dev' },
    repeat: { type: 'string', default: '1' },
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

const load = (file: string) => JSON.parse(readFileSync(new URL(file, import.meta.url), 'utf8')) as Case[]
const sets: Record<string, string[]> = { dev: ['./cases.json'], holdout: ['./holdout.json'], all: ['./cases.json', './holdout.json'] }
if (!sets[args.set!]) throw new Error(`--set 只能是 ${Object.keys(sets).join(' / ')}`)
const cases = sets[args.set!].flatMap(load).filter((c) => !args.only || args.only.split(',').includes(c.id))
const repeat = Math.max(1, Number(args.repeat) || 1)

function pct(xs: number[], p: number): number {
  if (!xs.length) return NaN
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))]
}
const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length
const sd = (xs: number[]) => (xs.length > 1 ? Math.sqrt(xs.reduce((a, x) => a + (x - mean(xs)) ** 2, 0) / (xs.length - 1)) : 0)

const KEYS = ['node', 'edge', 'topic', 'item', 'mark', 'group'] as const
type Key = (typeof KEYS)[number]
type Metrics = Record<Key, number | null>

const latencies: number[] = []
const prefill: number[] = []
const outTokens: number[] = []
/** 每一遍、每段的分数 */
const runs: { metrics: Map<string, Metrics>; violations: number }[] = []
const details = new Map<string, string>()
const caseMs = new Map<string, number[]>()

// 先打一枪暖机，把系统提示词放进 KV 缓存，第一句的延迟才有代表性
{
  const warm = new Session({ baseUrl: args['base-url']!, noThink: true })
  warm.say({ text: '大家好。' })
  await warm.drain()
}

const cell = (v: number | null | undefined) => (v === null || v === undefined || Number.isNaN(v) ? '' : v.toFixed(2))

for (let r = 0; r < repeat; r++) {
  const run = { metrics: new Map<string, Metrics>(), violations: 0 }
  runs.push(run)
  for (const c of cases) {
    const log: string[] = []
    const s = new Session(
      { baseUrl: args['base-url']!, noThink: true },
      {
        llm: (res) => {
          latencies.push(res.ms)
          caseMs.set(c.id, [...(caseMs.get(c.id) ?? []), res.ms])
          if (res.promptEvaluated !== undefined) prefill.push(res.promptEvaluated)
          if (res.outputTokens !== undefined) outTokens.push(res.outputTokens)
          log.push(
            `> ${res.batch.map((u) => u.text).join(' ')}\n` +
              `${formatOps(res.ops) || '-'}\n` +
              `  (${res.ms.toFixed(0)} ms, 读入 ${res.promptEvaluated ?? '?'}/${res.promptTokens ?? '?'} tok, 输出 ${res.outputTokens ?? '?'} tok)`,
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
    const m: Metrics = {
      node: c.gold.nodes?.length ? f1(sc.nodeP, sc.nodeR) : null,
      edge: c.gold.edges?.length ? f1(sc.edgeP, sc.edgeR) : null,
      topic: sc.topicR,
      item: sc.itemP !== null && sc.itemR !== null ? f1(sc.itemP, sc.itemR) : null,
      mark: sc.markAcc,
      group: sc.groupAcc,
    }
    run.metrics.set(c.id, m)
    run.violations += sc.violations.length
    // 详细记录只留第一遍的
    if (r === 0) {
      const miss = [sc.missing.length && `缺：${sc.missing.join('、')}`, sc.extra.length && `多：${sc.extra.join('、')}`, ...sc.violations].filter(Boolean)
      details.set(
        c.id,
        `### ${c.id} · ${c.name}\n\n${miss.length ? miss.map((x) => `- ${x}`).join('\n') : '- 全对'}\n\n` +
          `<details><summary>逐句操作与最终板书</summary>\n\n\`\`\`\n${log.join('\n')}\n\n${describe(s.graph)}\n\`\`\`\n</details>\n`,
      )
    }
    if (args.verbose) console.error(`${c.id}\n${log.join('\n')}\n${describe(s.graph)}\n`)
    console.error(
      `${repeat > 1 ? `[${r + 1}/${repeat}] ` : ''}${c.id}: 节点 ${cell(m.node) || '-'}，连线 ${cell(m.edge) || '-'}，话题 ${cell(m.topic) || '-'}，要点 ${cell(m.item) || '-'}，标记 ${cell(m.mark) || '-'}，违规 ${sc.violations.length}`,
    )
  }
}

/** 一遍里某个指标在所有用例上的平均 */
const runMean = (run: (typeof runs)[number], k: Key) => {
  const xs = [...run.metrics.values()].map((m) => m[k]).filter((v): v is number => v !== null)
  return xs.length ? mean(xs) : NaN
}
const counted = (k: Key) => [...runs[0].metrics.values()].filter((m) => m[k] !== null).length
/** 指标：各遍平均值的平均 ± 标准差 */
const overall = (k: Key) => {
  const per = runs.map((run) => runMean(run, k)).filter((v) => !Number.isNaN(v))
  if (!per.length) return '-'
  return `${mean(per).toFixed(3)}${repeat > 1 ? ` ± ${sd(per).toFixed(3)}` : ''} (${counted(k)} 段)`
}
/** 某段某个指标在各遍里的平均和极差 */
const perCase = (id: string, k: Key) => {
  const xs = runs.map((run) => run.metrics.get(id)![k]).filter((v): v is number => v !== null)
  return xs.length ? { m: mean(xs), range: Math.max(...xs) - Math.min(...xs) } : null
}
const unstable = (id: string) => KEYS.some((k) => (perCase(id, k)?.range ?? 0) >= 0.3)

const rows = cases.map((c) => {
  const v = (k: Key) => cell(perCase(c.id, k)?.m ?? null)
  return `| ${c.id}${unstable(c.id) ? ' ⚠' : ''} | ${KEYS.map(v).join(' | ')} | ${pct(caseMs.get(c.id) ?? [], 50).toFixed(0)} |`
})
const shaky = cases.filter((c) => unstable(c.id)).map((c) => c.id)

const summary = [
  `## 板书评测 · ${args.label} · ${args.set}${repeat > 1 ? ` · 每段 ${repeat} 遍` : ''}`,
  '',
  `| 指标 | 值${repeat > 1 ? '（平均 ± 各遍之间的标准差）' : ''} |`,
  `|---|---|`,
  `| 节点 F1 | ${overall('node')} |`,
  `| 连线 F1 | ${overall('edge')} |`,
  `| 话题召回 | ${overall('topic')} |`,
  `| 要点 F1（条目挂在对的话题下） | ${overall('item')} |`,
  `| 标记命中（划掉、强调、问题、公式） | ${overall('mark')} |`,
  `| 分组正确率 | ${overall('group')} |`,
  `| 违规（该删没删、纠正没生效），每遍平均 | ${mean(runs.map((r) => r.violations)).toFixed(1)} |`,
  `| 单句延迟 p50 / p95 | ${pct(latencies, 50).toFixed(0)} / ${pct(latencies, 95).toFixed(0)} ms |`,
  `| 每句实际读入 token p50（扣掉前缀缓存） | ${pct(prefill, 50)} |`,
  `| 每句输出 token p50 | ${pct(outTokens, 50)} |`,
  ...(repeat > 1 ? [`| 不稳定的用例（某项分数各遍相差 ≥ 0.3） | ${shaky.length ? shaky.join('、') : '无'} |`] : []),
  '',
  `| 用例 | 节点 | 连线 | 话题 | 要点 | 标记 | 分组 | 延迟 p50 ms |`,
  '|---|---|---|---|---|---|---|---|',
  ...rows,
  '',
  ...cases.map((c) => details.get(c.id)!),
].join('\n')

mkdirSync(new URL('./reports/', import.meta.url), { recursive: true })
const stamp = new Date().toISOString().replace(/[:.]/g, '-')
writeFileSync(new URL(`./reports/${args.label}-${args.set}-${stamp}.md`, import.meta.url), summary)
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary + '\n')
console.log(summary.split('\n').slice(0, 20 + cases.length).join('\n'))
