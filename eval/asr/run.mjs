import { execFileSync } from 'node:child_process'
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { corpus, directory, prepareAudio } from './download.mjs'
import { download, sha256 } from './files.mjs'
import { errorRate, keywordRecall } from './score.mjs'

const { values: args } = parseArgs({ options: {
  suite: { type: 'string', default: 'quick' }, model: { type: 'string', default: 'sensevoice' },
  denoise: { type: 'string', default: 'none' }, only: { type: 'string' },
} })
if (!['quick', 'full'].includes(args.suite) || !['sensevoice', 'whisper-base'].includes(args.model) || !['none', 'gtcrn'].includes(args.denoise)) throw new Error('Use --suite quick|full --model sensevoice|whisper-base --denoise none|gtcrn')
const selected = corpus.samples.filter((s) => (args.suite === 'full' || s.quick) && (!args.only || args.only.split(',').includes(s.id)))
if (!selected.length) throw new Error('No matching audio samples')
if (args.only && args.only.split(',').some((id) => !selected.some((s) => s.id === id))) throw new Error('Unknown sample or sample outside the selected suite')
const root = fileURLToPath(new URL('../../', import.meta.url))
process.chdir(root)
await prepareAudio()
execFileSync(process.execPath, ['scripts/fetch-voice.mjs'], { stdio: 'inherit' })
const extraModels = fileURLToPath(new URL('./.models/', import.meta.url))
const catalog = JSON.parse(readFileSync(new URL('./models.json', import.meta.url), 'utf8'))
const modelFiles = [...(catalog[args.model] || []), ...(catalog[args.denoise] || [])]
for (const model of modelFiles) await download(model, join(extraModels, model.file))
const label = `${args.model}-${args.denoise}-${args.suite}`
const reportDir = resolve('eval/reports/asr', `${new Date().toISOString().replace(/[:.]/g, '-')}-${label}`)
mkdirSync(reportDir, { recursive: true })
const output = join(reportDir, 'hypotheses.json'), input = join(reportDir, 'input.json')
writeFileSync(input, JSON.stringify({
  models: resolve('src-tauri'), extra_models: extraModels, model: args.model, denoise: args.denoise,
  samples: selected.map((s) => ({ id: s.id, path: join(directory, s.file) })), output,
}))
execFileSync('cargo', ['test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'voice::eval::corpus', '--', '--ignored', '--exact', '--nocapture'], {
  stdio: 'inherit', env: { ...process.env, BOARDCAST_ASR_INPUT: input },
})
const result = JSON.parse(readFileSync(output, 'utf8'))
if (result.samples.length !== selected.length || new Set(result.samples.map((s) => s.id)).size !== selected.length) throw new Error('Missing or duplicate inference results')
const details = selected.map((sample) => {
  const actual = result.samples.find((r) => r.id === sample.id)
  if (!actual || Math.abs(actual.duration - sample.duration) > 0.001) throw new Error(`Incorrect result: ${sample.id}`)
  const hypothesis = actual.segments.map((s) => s.text).join(' ')
  const reference = sample.reference.map((s) => s.text).join(' ')
  const score = ['reference', 'provisional'].includes(sample.scoring) ? errorRate(reference, hypothesis, sample.language) : null
  return { id: sample.id, language: sample.language, scoring: sample.scoring, reference, hypothesis, score,
    keywords: keywordRecall(sample.keywords || [], hypothesis, sample.language),
    duration: actual.duration, elapsedMs: actual.elapsedMs, rtf: actual.elapsedMs / 1000 / actual.duration }
})
const aggregates = ['zh', 'ja', 'en', 'zh-en'].map((language) => {
  const rows = details.filter((r) => r.language === language && r.scoring === 'reference')
  const errors = rows.reduce((sum, r) => sum + r.score.errors, 0), units = rows.reduce((sum, r) => sum + r.score.units, 0)
  return { language, samples: rows.length, errors, units, rate: units ? errors / units : null }
})
const voiceHashes = {}
for (const file of ['voice/sense-voice/model.int8.onnx', 'voice/sense-voice/tokens.txt', 'voice/silero_vad.onnx', 'voice/speaker/campplus_zh_en.onnx']) voiceHashes[file] = await sha256(join(root, 'src-tauri', file))
const report = { corpus: corpus.version, manifestSha256: await sha256(new URL('./corpus.json', import.meta.url)),
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
  dirty: Boolean(execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { encoding: 'utf8' }).trim()),
  model: args.model, denoise: args.denoise, suite: args.suite, only: args.only || null,
  models: { ...voiceHashes, ...Object.fromEntries(modelFiles.map((f) => [f.file, f.sha256])) },
  engineSha256: await sha256(join(root, 'src-tauri/src/voice.rs')), loadMs: result.loadMs, aggregates, details }
writeFileSync(join(reportDir, 'report.json'), JSON.stringify(report, null, 2) + '\n')
const pct = (rate) => rate === null ? '—' : `${(rate * 100).toFixed(2)}%`
const metric = (language) => language === 'en' ? 'WER' : language === 'zh-en' ? 'MER' : 'CER'
let markdown = `# ASR · ${label}\n\nCorpus: ${corpus.version}. Commit: ${report.commit}${report.dirty ? ' (working tree modified)' : ''}.\n\n`
markdown += 'Same desktop VAD/soft breaks/voiceprints; audio is fed faster than real time. RTF is processing time / audio duration, not microphone-to-screen latency. GTCRN is an offline experiment, not an app setting.\n\n'
markdown += '| Language | Metric | Reference samples | Errors / units | Rate |\n|---|---|---:|---:|---:|\n'
for (const a of aggregates) markdown += `| ${a.language} | ${metric(a.language)} | ${a.samples} | ${a.errors} / ${a.units} | ${pct(a.rate)} |\n`
markdown += '\nProvisional Chinese references and overlapping long meetings are excluded from these aggregates. Missing scores are not zero errors. Japanese is read speech, not a meeting. No accuracy threshold is enforced yet.\n\n| Sample | Reference policy | Error rate | Keyword recall | RTF |\n|---|---|---:|---:|---:|\n'
for (const d of details) markdown += `| ${d.id} | ${d.scoring} | ${pct(d.score?.rate ?? null)} | ${pct(d.keywords.rate)} | ${d.rtf.toFixed(3)} |\n`
for (const d of details) markdown += `\n## ${d.id}\n\nReference (${d.scoring}):\n\n${d.reference || '(silence)'}\n\nRecognized:\n\n${d.hypothesis || '(empty)'}\n`
writeFileSync(join(reportDir, 'report.md'), markdown)
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown)
console.log(`Report: ${join(reportDir, 'report.md')}`)
if (details.some((d) => d.scoring !== 'silence') && !details.some((d) => d.scoring !== 'silence' && d.hypothesis.trim())) throw new Error('No speech was recognized in any speech sample')
if (details.some((d) => d.scoring === 'silence' && d.hypothesis.trim())) throw new Error('Speech hallucinated on the silence control')
