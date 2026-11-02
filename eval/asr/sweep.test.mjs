import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

test('sweep keeps failed diagnostics, continues after missing reports, and exits with failure', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'boardcast-sweep-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const scripts = join(root, 'eval/asr')
  mkdirSync(scripts, { recursive: true })
  for (const file of ['sweep.mjs', 'compare.mjs', 'catalog.mjs']) copyFileSync(new URL(file, import.meta.url), join(scripts, file))
  writeFileSync(join(scripts, 'run.mjs'), `
    import { mkdirSync, writeFileSync } from 'node:fs'
    import { parseArgs } from 'node:util'
    const { values: { model, suite, output } } = parseArgs({ options: {
      model: { type: 'string' }, suite: { type: 'string' }, output: { type: 'string' },
    } })
    mkdirSync(output, { recursive: true })
    writeFileSync(output + '/input.json', JSON.stringify({ model, suite }))
    if (model === 'reazonspeech') process.exit(1)
    if (model === 'whisper-small') process.exit(0)
    const passed = model === 'sensevoice'
    writeFileSync(output + '/report.json', JSON.stringify({
      model, suite, manifestSha256: 'same', vad: 'on', policy: 'static', denoise: 'none',
      runtime: { passed }, checks: { passed, failures: passed ? [] : ['decoder exception'] },
      details: [{ id: 'meeting', variant: 'original', language: 'en', scoring: 'reference',
        score: { errors: 1, units: 10, rate: .1 }, elapsedMs: 100, duration: 600, rtf: .001 }],
    }))
    process.exit(passed ? 0 : 1)
  `)
  const run = spawnSync(process.execPath, [join(scripts, 'sweep.mjs'), '--suite', 'meetings', '--models', 'paraformer-zh-en,reazonspeech,whisper-small,sensevoice'], { encoding: 'utf8' })
  assert.equal(run.status, 1, run.stderr)
  const reports = join(root, 'eval/reports/asr-sweep')
  const output = join(reports, readdirSync(reports)[0])
  const failures = JSON.parse(readFileSync(join(output, 'failures.json'), 'utf8'))
  assert.deepEqual(failures.map((f) => f.model), ['paraformer-zh-en', 'reazonspeech', 'whisper-small'])
  assert.equal(failures[2].error, 'Missing report.json')
  const comparison = readFileSync(join(output, 'comparison.md'), 'utf8')
  assert.match(comparison, /paraformer-zh-en \| \*\*FAIL\*\*/)
  assert.match(comparison, /sensevoice \| PASS/)
  assert.match(comparison, /Failed candidates.*paraformer-zh-en, reazonspeech, whisper-small/)
  const merged = spawnSync(process.execPath, [join(scripts, 'compare.mjs'), output], { encoding: 'utf8' })
  assert.equal(merged.status, 0, merged.stderr)
  assert.match(merged.stdout, /Failed or incomplete runs — no score[\s\S]*- reazonspeech\n- whisper-small/)
})
