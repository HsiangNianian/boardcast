import { execFileSync } from 'node:child_process'
import { appendFileSync, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { corpusFor, directory, prepareAudio } from './download.mjs'
import { download, sha256 } from './files.mjs'
import { errorRate, keywordRecall, recognitionFailures } from './score.mjs'
import { comparison } from './dynamic.mjs'
import { models, supportedSamples } from './catalog.mjs'

const { values: args } = parseArgs({ options: {
  policy: { type: 'string', default: 'static' },
  suite: { type: 'string', default: 'quick' }, model: { type: 'string', default: 'sensevoice' },
  denoise: { type: 'string', default: 'none' }, vad: { type: 'string', default: 'on' }, only: { type: 'string' },
  output: { type: 'string' },
} })
const corpus = corpusFor(args.suite)
if (!Object.hasOwn(models, args.model) || !['none', 'gtcrn'].includes(args.denoise) || !['on', 'off'].includes(args.vad)) throw new Error(`Use --suite quick|full|meetings --model ${Object.keys(models).join('|')} --denoise none|gtcrn --vad on|off`)
if (!['static', 'dynamic'].includes(args.policy) || (args.policy === 'dynamic' && (args.vad !== 'on' || args.denoise !== 'none'))) throw new Error('Dynamic policy needs --vad on --denoise none')
if (models[args.model].backend && (args.policy !== 'static' || args.vad !== 'on' || args.denoise !== 'none')) throw new Error('CTC candidates currently require --policy static --vad on --denoise none')
let selected = corpus.samples.filter((s) => (args.suite !== 'quick' || s.quick) && (!args.only || args.only.split(',').includes(s.id)))
const excluded = selected.filter((s) => !supportedSamples([s], args.model).length).map((s) => s.id)
selected = supportedSamples(selected, args.model)
if (!selected.length) throw new Error('No matching audio samples')
if (args.only && args.only.split(',').some((id) => !selected.some((s) => s.id === id))) throw new Error('Unknown sample or sample outside the selected suite/language scope')
selected = selected.map((s) => ({ ...s, variant: 'original' }))
if (args.policy === 'dynamic') {
  // Deterministic stress controls, not additional real meetings or speaker-separation gold.
  const seeds = ['zh', 'ja', 'en', 'zh-en'].map((lang) => selected.find((s) => s.language === lang && s.duration <= 15 && s.scoring !== 'silence')).filter(Boolean)
  const silence = selected.find((s) => s.scoring === 'silence')
  selected.push(...seeds.flatMap((s) => ['quiet', 'noise', 'dual_echo'].map((variant) => ({ ...s, id: `${s.id}@${variant}`, variant }))))
  if (silence) selected.push({ ...silence, id: `${silence.id}@noise`, variant: 'noise' })
}
const root = fileURLToPath(new URL('../../', import.meta.url))
process.chdir(root)
await prepareAudio({ ...corpus, samples: selected.filter((s) => s.variant === 'original') })
const manifestPath = new URL(args.suite === 'meetings' ? './meetings.json' : './corpus.json', import.meta.url)
const manifestSha256 = await sha256(manifestPath)
execFileSync(process.execPath, ['scripts/fetch-voice.mjs'], { stdio: 'inherit' })
const extraModels = fileURLToPath(new URL('./.models/', import.meta.url))
const catalog = JSON.parse(readFileSync(new URL('./models.json', import.meta.url), 'utf8'))
const candidate = models[args.model]
const modelFiles = [...(catalog[candidate.package] || []), ...(catalog[args.denoise] || [])]
for (const model of modelFiles) await download(model, join(extraModels, model.file))
const label = `${args.model}-${args.denoise}-vad-${args.vad}-${args.policy}-${args.suite}`
const reportDir = args.output ? resolve(args.output) : resolve('eval/reports/asr', `${new Date().toISOString().replace(/[:.]/g, '-')}-${label}`)
if (existsSync(join(reportDir, 'input.json'))) throw new Error(`Refusing to overwrite a previous run: ${reportDir}`)
mkdirSync(reportDir, { recursive: true })
const output = join(reportDir, 'hypotheses.json'), input = join(reportDir, 'input.json')
const sources = ['voice.rs', 'voice_policy.rs', 'voice_eval.rs']
const sourceHashes = await Promise.all(sources.map((s) => sha256(join(root, 'src-tauri/src', s))))
const scorerSha256 = await sha256(new URL('./score.mjs', import.meta.url))
const adapterSha256 = candidate.backend ? await sha256(new URL('./ctc.py', import.meta.url)) : null
writeFileSync(input, JSON.stringify({
  models: resolve('src-tauri'), extra_models: extraModels, model: args.model, denoise: args.denoise, vad: args.vad, policy: args.policy,
  recognizer: candidate.backend ? { family: 'sensevoice' } : candidate.config,
  external_model: candidate.backend ? join(extraModels, candidate.config.model) : undefined,
  samples: selected.map((s) => ({ id: s.id, path: join(directory, s.file), variant: s.variant })), output,
}))
execFileSync('cargo', ['test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'voice::eval::corpus', '--', '--ignored', '--exact', '--nocapture'], {
  stdio: 'inherit', env: { ...process.env, BOARDCAST_ASR_INPUT: input },
})
if (candidate.backend) execFileSync('uv', ['run', '--locked', '--script', 'eval/asr/ctc.py', input], { stdio: 'inherit' })
const result = JSON.parse(readFileSync(output, 'utf8'))
if (manifestSha256 !== await sha256(manifestPath)) throw new Error('Corpus changed during inference')
if (scorerSha256 !== await sha256(new URL('./score.mjs', import.meta.url))) throw new Error('Scorer changed during inference')
if (candidate.backend && adapterSha256 !== await sha256(new URL('./ctc.py', import.meta.url))) throw new Error('CTC adapter changed during inference')
for (const [i, source] of sources.entries()) {
  if (sourceHashes[i] !== await sha256(join(root, 'src-tauri/src', source))) throw new Error(`Source changed during inference: ${source}; rerun evaluation`)
}
if (result.model !== args.model || result.denoise !== args.denoise || result.vad !== args.vad || result.policy !== args.policy) throw new Error('Inference configuration does not match the requested evaluation')
if (result.samples.length !== selected.length || new Set(result.samples.map((s) => s.id)).size !== selected.length) throw new Error('Missing or duplicate inference results')
const details = selected.map((sample) => {
  const actual = result.samples.find((r) => r.id === sample.id)
  if (!actual || Math.abs(actual.duration - sample.duration) > 0.001) throw new Error(`Incorrect result: ${sample.id}`)
  const hypothesis = actual.segments.map((s) => s.text).join(' ')
  const reference = sample.reference.map((s) => s.text).join(' ')
  const score = ['reference', 'ordered', 'provisional'].includes(sample.scoring) ? errorRate(reference, hypothesis, sample.language) : null
  return { id: sample.id, variant: sample.variant, comparison: args.policy === 'dynamic' ? comparison(sample, actual) : undefined, language: sample.language, scoring: sample.scoring, reference, hypothesis, score,
    industry: sample.industry, kind: sample.kind, note: sample.note, audioSha256: sample.sha256,
    keywords: keywordRecall(sample.keywords || [], hypothesis, sample.language),
    inputRms: actual.inputRms, inputPeak: actual.inputPeak, vadSegments: actual.vadSegments,
    vadSeconds: actual.vadSeconds, finalSegments: actual.segments.length,
    duration: actual.duration, elapsedMs: actual.elapsedMs, rtf: actual.elapsedMs / 1000 / actual.duration }
})
const aggregates = ['zh', 'ja', 'en', 'zh-en'].map((language) => {
  const rows = details.filter((r) => r.variant === 'original' && r.language === language && r.scoring === 'reference')
  const errors = rows.reduce((sum, r) => sum + r.score.errors, 0), units = rows.reduce((sum, r) => sum + r.score.units, 0)
  return { language, samples: rows.length, errors, units, rate: units ? errors / units : null }
})
const voiceHashes = {}
const failures = recognitionFailures(details.filter((d) => d.variant === 'original'))
if (args.policy === 'dynamic') {
  for (const d of details) {
    if (d.scoring === 'reference' && d.comparison.errorDelta > 0) failures.push(`${d.id}: dynamic recognition regressed by ${d.comparison.errorDelta} edits`)
    if (d.scoring === 'silence' && d.hypothesis.trim()) failures.push(`${d.id}: speech hallucinated on silence/noise`)
  }
  if (args.suite !== 'meetings' && !args.only && !details.some((d) => d.comparison.attempts)) failures.push('Dynamic policy never ran a second pass')
}
const production = args.model === 'sensevoice' && args.denoise === 'none' && args.vad === 'on'
for (const file of ['voice/sense-voice/model.int8.onnx', 'voice/sense-voice/tokens.txt', 'voice/silero_vad.onnx', 'voice/speaker/campplus_zh_en.onnx']) voiceHashes[file] = await sha256(join(root, 'src-tauri', file))
const report = { corpus: corpus.version, manifestSha256,
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
  dirty: Boolean(execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], { encoding: 'utf8' }).trim()),
  model: args.model, denoise: args.denoise, vad: args.vad, policy: args.policy, suite: args.suite, only: args.only || null,
  languageMode: candidate.languageMode, excluded, recognizer: candidate.config,
  timingScope: candidate.backend ? 'recognizer-only' : 'desktop-pipeline',
  modelBytes: candidate.package ? catalog[candidate.package].reduce((sum, f) => sum + f.bytes, 0)
    : ['voice/sense-voice/model.int8.onnx', 'voice/sense-voice/tokens.txt'].reduce((sum, f) => sum + statSync(join(root, 'src-tauri', f)).size, 0),
  models: { ...voiceHashes, ...Object.fromEntries(modelFiles.map((f) => [f.file, f.sha256])) },
  engineSha256: sourceHashes[0], policySha256: sourceHashes[1], harnessSha256: sourceHashes[2], scorerSha256, adapterSha256, loadMs: result.loadMs,
  checks: { enforced: production, passed: !failures.length, failures }, aggregates, details }
writeFileSync(join(reportDir, 'report.json'), JSON.stringify(report, null, 2) + '\n')
const pct = (rate) => rate === null ? '—' : `${(rate * 100).toFixed(2)}%`
const metric = (language) => language === 'en' ? 'WER' : language === 'zh-en' ? 'MER' : 'CER'
let markdown = `# ASR · ${label}\n\nCorpus: ${corpus.version}. Commit: ${report.commit}${report.dirty ? ' (working tree modified)' : ''}.\n\n`
markdown += `Language selection: **${candidate.languageMode}**. Evaluated languages: ${candidate.languages.join(', ')}. Unsupported samples omitted: ${excluded.length ? excluded.join(', ') : 'none'}. No reference text is supplied to the recognizer.\n\n`
if (candidate.backend) markdown += 'Transformers CTC reuses the desktop baseline’s VAD boundaries; it receives only audio, not the baseline transcript. Its RTF measures recognition alone, excluding the separate segmentation pass, and is not directly comparable to native pipeline RTF.\n\n'
markdown += args.vad === 'on'
  ? 'Desktop VAD with bounded level compensation, soft breaks and voiceprints; VAD gain is not applied to recognition or voiceprints.\n\n'
  : 'VAD bypassed: fixed 10-second windows (including silence), same recognizer and voiceprints. Window boundaries may split words. This is an evaluation control, not an app setting.\n\n'
markdown += 'Audio is fed faster than real time. RTF is processing time / audio duration, not microphone-to-screen latency. GTCRN is an offline experiment before segmentation.\n\n'
markdown += `Speech/silence checks: **${failures.length ? 'FAIL' : 'PASS'}** (${production ? 'enforced for the production pipeline' : 'diagnostic only for this experimental control'}).\n\n`
for (const failure of failures) markdown += `- ${failure}\n`
if (failures.length) markdown += '\n'
markdown += '| Language | Metric | Reference samples | Errors / units | Rate |\n|---|---|---:|---:|---:|\n'
for (const a of aggregates) markdown += `| ${a.language} | ${metric(a.language)} | ${a.samples} | ${a.errors} / ${a.units} | ${pct(a.rate)} |\n`
markdown += '\nOnly reference-scored samples enter the language aggregates. Ordered scores concatenate overlapping manual turns by start time and can penalize a different valid ordering; provisional scores use edited minutes or unverified text. Both are diagnostic only. Missing scores are not zero errors. No absolute CER/WER threshold is enforced. Dynamic runs also enforce no added edits on reference-scored paired cases. Input level is measured before enhancement; VAD seconds include segment padding.\n\n| Sample / industry | Reference policy | Error rate | Keyword recall | Input dBFS | VAD seconds | Finals | RTF |\n|---|---|---:|---:|---:|---:|---:|---:|\n'
for (const d of details) markdown += `| ${d.id}${d.industry ? ` / ${d.industry}` : ''} | ${d.scoring} | ${pct(d.score?.rate ?? null)} | ${pct(d.keywords.rate)} | ${d.inputRms ? (20 * Math.log10(d.inputRms)).toFixed(1) : '−∞'} | ${d.vadSeconds?.toFixed(2) ?? '—'} | ${d.finalSegments} | ${d.rtf.toFixed(3)} |\n`
if (args.policy === 'dynamic') {
  markdown += '\n## Paired dynamic policy\n\nStatic and dynamic share the same audio, model, VAD and first pass. Original samples alone enter the language aggregates above. Derived quiet (0.15×), deterministic additive noise and dual-channel same-speech echo stress the policy; they are not separate real meetings or separation accuracy tests. References are never available to the policy. Extra wall time is noisy; retry ms measures the actual second-pass inference.\n\n| Sample | Static errors | Dynamic errors | Delta | Attempts / accepted | Retry ms | Extra ms |\n|---|---:|---:|---:|---:|---:|---:|\n'
  for (const d of details) {
    const c = d.comparison
    markdown += `| ${d.id} | ${c.score?.errors ?? '—'} | ${d.score?.errors ?? '—'} | ${c.errorDelta ?? '—'} | ${c.attempts} / ${c.accepted} | ${c.retryMs} | ${c.extraMs.toFixed(1)} |\n`
  }
}
for (const d of details) {
  markdown += `\n## ${d.id}\n\n${d.note || ''}\n\nReference (${d.scoring}):\n\n${d.reference || '(silence)'}\n\nRecognized:\n\n${d.hypothesis || '(empty)'}\n`
  if (d.comparison) {
    markdown += `\nStatic control: ${d.comparison.hypothesis || '(empty)'}\n`
    for (const r of d.comparison.reviews) markdown += `\n- ${r.start.toFixed(2)}–${r.end.toFixed(2)} s: ${r.reason} → ${r.action} → ${r.decision} (${r.asrMs} ms). Original: ${r.original || '(empty)'}. Candidate: ${r.candidate || '(empty)'}.\n`
  }
}
writeFileSync(join(reportDir, 'report.md'), markdown)
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown)
console.log(`Report: ${join(reportDir, 'report.md')}`)
if (production && failures.length) throw new Error(failures.join('; '))
if (failures.length) console.warn(`${process.env.GITHUB_ACTIONS ? '::warning::' : ''}Experimental control: ${failures.join('; ')}`)
