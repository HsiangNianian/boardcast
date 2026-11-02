import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { models } from './catalog.mjs'
import { compare } from './compare.mjs'

const { values: args } = parseArgs({ options: {
  suite: { type: 'string', default: 'quick' }, models: { type: 'string', default: Object.keys(models).join(',') },
} })
const selected = args.models.split(',')
if (!['quick', 'full', 'meetings'].includes(args.suite) || new Set(selected).size !== selected.length || selected.some((m) => !Object.hasOwn(models, m))) throw new Error('Use --suite quick|full|meetings --models comma-separated-candidate-names')
const root = fileURLToPath(new URL('../../', import.meta.url))
process.chdir(root)
const dir = resolve('eval/reports/asr-sweep', new Date().toISOString().replace(/[:.]/g, '-'))
mkdirSync(dir, { recursive: true })
const reports = [], failures = []
for (const model of selected) {
  const output = resolve(dir, model)
  const log = spawnSync(process.execPath, ['eval/asr/run.mjs', '--model', model, '--suite', args.suite, '--output', output], { stdio: 'inherit' })
  const report = resolve(output, 'report.json')
  const hasReport = existsSync(report)
  if (log.error || log.status !== 0 || !hasReport) {
    failures.push({ model, status: log.status, signal: log.signal, error: log.error?.message || (!hasReport ? 'Missing report.json' : undefined) })
  }
  if (hasReport) reports.push(JSON.parse(readFileSync(report, 'utf8')))
}
writeFileSync(resolve(dir, 'failures.json'), JSON.stringify(failures, null, 2) + '\n')
if (reports.length) writeFileSync(resolve(dir, 'comparison.md'), compare(reports)
  + (failures.length ? `\nFailed candidates (see their logs; missing scores are not passes): ${failures.map((f) => f.model).join(', ')}.\n` : ''))
console.log(`Sweep results: ${dir}`)
if (failures.length) throw new Error(`Failed candidates: ${failures.map((f) => f.model).join(', ')}; see failures.json`)
