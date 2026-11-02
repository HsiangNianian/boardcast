import { appendFileSync, globSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export function compare(reports) {
  if (!reports.length) throw new Error('No ASR reports; an empty comparison is not a pass')
  for (const key of ['manifestSha256', 'engineSha256', 'policySha256', 'harnessSha256', 'scorerSha256', 'suite']) {
    if (new Set(reports.map((r) => r[key])).size !== 1) throw new Error(`Incompatible reports: ${key}`)
  }
  const seen = new Set()
  let text = '# ASR candidate comparison\n\nLower CER/WER/MER is better. Language-selected controls are labelled explicitly; this does not test a language router. RTF includes desktop segmentation and voiceprints, excludes loading, and is not live latency. Compare timings only on the same hardware.\n\n'
  for (const language of ['zh', 'ja', 'en', 'zh-en']) {
    const scoring = language === 'zh' ? 'provisional' : 'reference'
    const metric = language === 'en' ? 'WER' : language === 'zh-en' ? 'MER' : 'CER'
    text += `## ${language} · ${metric}${language === 'zh' ? ' · PROVISIONAL REFERENCES' : ''}\n\n`
    if (language === 'zh') text += 'Chinese references have not been human-verified. These numbers must not decide a production model or enter formal reference aggregates.\n\n'
    let sampleIds
    text += '| Model | Language mode | VAD / policy / denoise | Samples | Errors / units | Rate | RTF (scope) | ASR files MiB |\n|---|---|---|---:|---:|---:|---|---:|\n'
    for (const r of reports) {
      const rows = r.details.filter((d) => d.variant === 'original' && d.language === language && d.scoring === scoring)
      if (!rows.length) continue
      const ids = rows.map((d) => d.id).sort().join(',')
      if (sampleIds !== undefined && ids !== sampleIds) throw new Error(`Different ${language} samples; cannot compare subsets`)
      sampleIds = ids
      const key = [language, r.model, r.vad, r.policy, r.denoise].join('/')
      if (seen.has(key)) throw new Error(`Duplicate configuration: ${key}`)
      seen.add(key)
      const errors = rows.reduce((n, d) => n + d.score.errors, 0), units = rows.reduce((n, d) => n + d.score.units, 0)
      const elapsed = rows.reduce((n, d) => n + d.elapsedMs, 0), duration = rows.reduce((n, d) => n + d.duration, 0)
      text += `| ${r.model} | ${r.languageMode || 'unrecorded'} | ${r.vad} / ${r.policy} / ${r.denoise} | ${rows.length} | ${errors} / ${units} | ${units ? (errors / units * 100).toFixed(2) + '%' : '—'} | ${(elapsed / 1000 / duration).toFixed(3)} (${r.timingScope || 'desktop-pipeline'}) | ${r.modelBytes ? (r.modelBytes / 2 ** 20).toFixed(0) : '—'} |\n`
    }
    if (sampleIds === undefined) text += '\nNo scored samples for this language.\n'
    text += '\n'
  }
  text += '## Speech / silence checks\n\n'
  for (const r of reports) text += `- ${r.model} / ${r.vad} / ${r.policy} / ${r.denoise}: **${r.checks.passed ? 'PASS' : 'FAIL'}**${r.checks.failures.length ? ' — ' + r.checks.failures.join('; ') : ''}\n`
  return text + '\nUnsupported languages are omitted, never scored as zero errors. Japanese fixtures are read speech; the corpus is too small to establish a universal winner.\n'
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = process.argv[2]
  if (!root) throw new Error('Usage: node eval/asr/compare.mjs REPORT_DIRECTORY')
  const reports = globSync('**/report.json', { cwd: root }).sort().map((p) => JSON.parse(readFileSync(resolve(root, p), 'utf8')))
  const markdown = compare(reports)
  console.log(markdown)
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown)
}
