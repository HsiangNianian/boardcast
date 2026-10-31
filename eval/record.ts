// Records the sample meetings for the web demo (issue #30): each case in eval/cases.json is played
// through a real model once, and the board steps, transcript and summary are saved, so the demo
// can replay them without any model.
//
//   node eval/record.ts [--base-url http://127.0.0.1:8080] [--only login-flow,en-auth-correction]
//
// Output: public/demos/<id>.json (one Demo each) and public/demos/index.json (the list).

import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { parseArgs } from 'node:util'
import { emptyGraph } from '../src/engine/graph.ts'
import { spokenLang } from '../src/engine/lang.ts'
import { formatOps } from '../src/engine/ops.ts'
import type { Utterance } from '../src/engine/prompt.ts'
import { Session } from '../src/engine/session.ts'
import { type Step, diffGraph, inkOnly } from '../src/engine/steps.ts'
import { summarize } from '../src/engine/summary.ts'
import type { Demo, DemoInfo } from '../src/demo.ts'
import type { MeetingLine } from '../src/meetings.ts'

const { values: args } = parseArgs({
  options: {
    'base-url': { type: 'string', default: 'http://127.0.0.1:8080' },
    only: { type: 'string' },
  },
})

interface Case {
  id: string
  name: string
  turns: Utterance[]
}

/** Pause between two sentences in the recording, on top of the model's own time */
const GAP_MS = 2000

/** Cases not worth showing even when recorded fine; add an id here if the model draws it wrong */
const SKIP = new Set<string>()

const base = args['base-url']!.replace(/\/+$/, '')
const healthy = await fetch(`${base}/health`, { signal: AbortSignal.timeout(5000) })
  .then((r) => r.ok)
  .catch(() => false)
if (!healthy) {
  console.error(`Can't reach a llama-server at ${base} (GET /health failed); start one or pass --base-url.`)
  process.exit(1)
}

const llm = { baseUrl: base, noThink: true }
const cases = (JSON.parse(readFileSync(new URL('./cases.json', import.meta.url), 'utf8')) as Case[]).filter(
  (c) => (!args.only || args.only.split(',').includes(c.id)) && !SKIP.has(c.id),
)
const out = new URL('../public/demos/', import.meta.url)
mkdirSync(out, { recursive: true })

const indexFile = new URL('index.json', out)
let index: DemoInfo[] = []
try {
  index = JSON.parse(readFileSync(indexFile, 'utf8'))
} catch {
  /* first run */
}

for (const c of cases) {
  const lines: MeetingLine[] = []
  const steps: Step[] = []
  let recorded = emptyGraph()
  let t = 0
  let pending: { ms: number; ops: string } | null = null
  const s = new Session(llm, {
    llm: (r) => {
      pending = { ms: r.ms, ops: formatOps(r.ops) || '-' }
    },
    error: (e) => {
      throw e
    },
  })
  for (const turn of c.turns) {
    pending = null
    s.say(turn)
    // One sentence at a time, like the eval, so each step belongs to exactly one line
    await s.drain()
    const done = pending as { ms: number; ops: string } | null
    lines.push({ ...turn, ms: Math.round(done?.ms ?? 0), ops: done?.ops ?? '-', page: 0 })
    t += GAP_MS + (done?.ms ?? 0)
    const ink = inkOnly(s.graph)
    const d = diffGraph(recorded, ink)
    if (d) {
      steps.push({ t: Math.round(t), line: lines.length, d })
      recorded = ink
    }
  }
  const lang = spokenLang(c.turns.map((u) => u.text))
  // Nothing ever lands on the board (e.g. a case that only points at things): nothing to watch.
  // An English meeting whose board still has Chinese on it (the model sometimes labels a line in
  // Chinese) would look broken to the visitors the English demo is for.
  const board = [...s.graph.nodes.map((n) => n.label), ...s.graph.edges.map((e) => e.label ?? ''), ...s.graph.groups.map((x) => x.label)]
  const why = !steps.length ? 'no board changes' : lang === 'en' && board.some((x) => /\p{Script=Han}/u.test(x)) ? 'Chinese on an English board' : ''
  if (why) {
    console.log(`${c.id}: ${why}, skipped`)
    index = index.filter((x) => x.id !== c.id)
    rmSync(new URL(`${c.id}.json`, out), { force: true })
    continue
  }
  const summary = await summarize(llm, [s.graph], lines).then(
    (r) => r.text,
    (e) => {
      console.warn(`  ${c.id}: no summary (${e})`)
      return undefined
    },
  )
  const demo: Demo = { id: c.id, name: c.name, lang, graph: s.graph, history: s.history, lines, steps, summary }
  writeFileSync(new URL(`${c.id}.json`, out), `${JSON.stringify(demo)}\n`)
  index = [...index.filter((x) => x.id !== c.id), { id: c.id, name: c.name, lang }]
  console.log(`${c.id}: ${lines.length} lines, ${steps.length} steps, ${s.graph.nodes.length} nodes${summary ? ', summary' : ''}`)
}

// Keep the order of cases.json, then anything recorded earlier that is no longer there
const order = new Map((JSON.parse(readFileSync(new URL('./cases.json', import.meta.url), 'utf8')) as Case[]).map((c, i) => [c.id, i]))
index.sort((a, b) => (order.get(a.id) ?? 1e9) - (order.get(b.id) ?? 1e9))
writeFileSync(indexFile, `${JSON.stringify(index, null, 2)}\n`)
