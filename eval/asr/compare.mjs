import { appendFileSync, existsSync, globSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export function compare(reports) {
  if (!reports.length) throw new Error('No ASR reports; an empty comparison is not a pass')
  for (const key of ['manifestSha256', 'engineSha256', 'policySha256', 'harnessSha256', 'scorerSha256', 'suite']) {
    if (new Set(reports.map((r) => r[key])).size !== 1) throw new Error(`Incompatible reports: ${key}`)
  }
  const seen = new Set()
  let text = '# ASR candidate comparison\n\nLower CER/WER/MER is better. Language-selected controls are labelled explicitly; this does not test a language router. RTF includes desktop segmentation and voiceprints, excludes loading, and is not live latency. Compare timings only on the same hardware.\n\n'
  for (const language of ['zh', 'ja', 'en', 'zh-en']) for (const scoring of ['reference', 'ordered', 'provisional']) {
    if (!reports.some((r) => r.details.some((d) => d.language === language && d.scoring === scoring && d.variant === 'original'))) continue
    const metric = language === 'en' ? 'WER' : language === 'zh-en' ? 'MER' : 'CER'
    text += `## ${language} · ${metric}${scoring === 'reference' ? '' : ` · ${scoring.toUpperCase()} — DIAGNOSTIC ONLY`}\n\n`
    if (scoring === 'provisional') text += 'Edited official minutes or unverified references. Excluded from formal aggregates; inspect the sample notes before interpreting these scores.\n\n'
    if (scoring === 'ordered') text += 'Manual overlapping turns are concatenated by start time. Another valid order can add errors; this is not overlap-aware meeting WER/CER or a separation benchmark. Excluded from formal aggregates.\n\n'
    let sampleIds
    text += '| Model | Runtime | Language mode | VAD / policy / denoise | Samples | Errors / units | Rate | RTF (scope) | ASR files MiB |\n|---|---|---|---|---:|---:|---:|---|---:|\n'
    for (const r of reports) {
      const rows = r.details.filter((d) => d.variant === 'original' && d.language === language && d.scoring === scoring)
      if (!rows.length) continue
      const ids = rows.map((d) => d.id).sort().join(',')
      if (sampleIds !== undefined && ids !== sampleIds) throw new Error(`Different ${language} samples; cannot compare subsets`)
      sampleIds = ids
      const key = [language, scoring, r.model, r.vad, r.policy, r.denoise].join('/')
      if (seen.has(key)) throw new Error(`Duplicate configuration: ${key}`)
      seen.add(key)
      const errors = rows.reduce((n, d) => n + d.score.errors, 0), units = rows.reduce((n, d) => n + d.score.units, 0)
      const elapsed = rows.reduce((n, d) => n + d.elapsedMs, 0), duration = rows.reduce((n, d) => n + d.duration, 0)
      text += `| ${r.model} | ${r.runtime ? r.runtime.passed ? 'PASS' : '**FAIL**' : 'not recorded'} | ${r.languageMode || 'unrecorded'} | ${r.vad} / ${r.policy} / ${r.denoise} | ${rows.length} | ${errors} / ${units} | ${units ? (errors / units * 100).toFixed(2) + '%' : '—'} | ${(elapsed / 1000 / duration).toFixed(3)} (${r.timingScope || 'desktop-pipeline'}) | ${r.modelBytes ? (r.modelBytes / 2 ** 20).toFixed(0) : '—'} |\n`
    }
    if (sampleIds === undefined) text += '\nNo scored samples for this language.\n'
    text += '\n'
  }
  if (reports[0].suite === 'meetings') {
    text += '## Per meeting / industry\n\n| Recording | Industry / kind | Scoring | Configuration | Errors / units | Rate | RTF |\n|---|---|---|---|---:|---:|---:|\n'
    for (const r of reports) for (const d of r.details.filter((d) => d.variant === 'original' && d.scoring !== 'silence')) {
      text += `| ${d.id} | ${d.industry} / ${d.kind} | ${d.scoring} | ${r.model} / ${r.vad} / ${r.policy} / ${r.denoise} | ${d.score ? `${d.score.errors} / ${d.score.units}` : '—'} | ${d.score?.rate != null ? (d.score.rate * 100).toFixed(2) + '%' : '—'} | ${d.rtf.toFixed(3)} |\n`
    }
    text += '\n'
  }
  text += '## Speech / silence checks\n\n'
  for (const r of reports) text += `- ${r.model} / ${r.vad} / ${r.policy} / ${r.denoise}: **${r.checks.passed ? 'PASS' : 'FAIL'}**${r.checks.failures.length ? ' — ' + r.checks.failures.join('; ') : ''}\n`
  return text + '\nUnsupported languages are omitted, never scored as zero errors. These selected recordings do not establish a universal winner or validate a language router. Read-speech fixtures, scenario meetings and actual business/public meetings are identified in their manifests.\n'
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = process.argv[2]
  if (!root) throw new Error('Usage: node eval/asr/compare.mjs REPORT_DIRECTORY')
  const reports = globSync('**/report.json', { cwd: root }).sort().map((p) => JSON.parse(readFileSync(resolve(root, p), 'utf8')))
  const incomplete = globSync('**/input.json', { cwd: root }).map(dirname).filter((p) => !existsSync(resolve(root, p, 'report.json'))).sort()
  const markdown = compare(reports) + (incomplete.length ? '\n## Failed or incomplete runs — no score\n\nThese runs have inputs but no scored report. Inspect their logs and CI job status; missing results are not passes.\n\n' + incomplete.map((p) => `- ${p}`).join('\n') + '\n' : '')
  console.log(markdown)
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown)
}
