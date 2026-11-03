import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { emptyGraph, findNode } from '../src/engine/graph.ts'
import { correct, parseGlossary } from '../src/engine/glossary.ts'
import { applyOps, formatOps } from '../src/engine/ops.ts'
import { resolveTargets, speechPlan } from '../src/engine/phonetic.ts'

interface Case {
  id: string
  language: 'zh' | 'en' | 'ja'
  labels: string[]
  text: string
  glossary?: string
  pointing?: number[]
  targets: string[]
  resolved: (string | null)[]
  direct: string | null
}

const cases: Case[] = JSON.parse(readFileSync(new URL('./phonetic-cases.json', import.meta.url), 'utf8'))

/** Synthetic transcript errors, tested through the production resolver and command parser. */
export async function evaluatePhonetics() {
  const rows = []
  for (const c of cases) {
    const graph = applyOps(emptyGraph(), c.labels.map((name) => ({ op: 'N', name })))
    const terms = parseGlossary(c.glossary ?? '')
    const next = { text: c.text, pointing: c.pointing?.map((n) => `n${n}`) }
    const start = performance.now()
    const matches = await resolveTargets(graph, c.targets, c.language === 'ja' ? 'ja' : 'zh', terms, next.pointing)
    const actual = matches.map((m) => graph.nodes.find((n) => n.id === m.selected)?.label ?? null)
    const plan = await speechPlan(graph, [], next, terms)
    const direct = plan.direct === undefined ? null : formatOps(plan.direct) || '-'
    const failures = actual.flatMap((label, i) => label === c.resolved[i] ? [] : [`target ${c.targets[i]}: ${label} != ${c.resolved[i]}`])
    if (direct !== c.direct) failures.push(`action: ${direct} != ${c.direct}`)
    rows.push({ id: c.id, language: c.language, text: c.text, expected: c.resolved, actual,
      baseline: c.targets.map((t) => findNode(graph, correct(t, terms))?.label ?? null),
      expectedAction: c.direct, action: direct, ms: Math.round((performance.now() - start) * 100) / 100, failures })
  }
  return rows
}

if (process.argv[1] && resolve(process.argv[1]) === import.meta.filename) {
  const rows = await evaluatePhonetics()
  const lines = ['## Phonetic label / command regression', '',
    'Synthetic Chinese, Japanese and English transcript perturbations. Offline dictionaries, no ASR or LLM inference; this is not an audio accuracy benchmark.', '',
    '| Language | Targets | Exact baseline correct | With phonetics correct | Wrong matches | Commands / abstentions correct |',
    '| --- | ---: | ---: | ---: | ---: | ---: |']
  for (const language of ['zh', 'en', 'ja']) {
    const group = rows.filter((r) => r.language === language)
    let total = 0, baseline = 0, correct = 0, wrong = 0
    for (const r of group) r.expected.forEach((expected, i) => {
      total++
      baseline += +(r.baseline[i] === expected)
      correct += +(r.actual[i] === expected)
      wrong += +(r.actual[i] !== null && r.actual[i] !== expected)
    })
    lines.push(`| ${language} | ${total} | ${baseline} | ${correct} | ${wrong} | ${group.filter((r) => r.action === r.expectedAction).length}/${group.length} |`)
  }
  const failures = rows.flatMap((r) => r.failures.map((f) => `${r.id}: ${f}`))
  lines.push('', `${rows.length - rows.filter((r) => r.failures.length).length}/${rows.length} cases passed.`, '',
    'Exact baseline includes glossary correction. Correct totals include expected abstentions; wrong matches count incorrect non-empty selections. Command checks include required delegation and refusals. LLM decisions and actual ASR errors are evaluated separately.',
    ...failures.map((f) => `- ${f}`), '')
  const markdown = lines.join('\n')
  mkdirSync('eval/reports', { recursive: true })
  writeFileSync('eval/reports/phonetic.json', JSON.stringify({ kind: 'synthetic-transcript-regression', rows }, null, 2))
  writeFileSync('eval/reports/phonetic.md', markdown)
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown)
  console.log(markdown)
  if (failures.length) process.exitCode = 1
}
