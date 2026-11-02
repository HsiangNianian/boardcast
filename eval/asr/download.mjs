import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { parseArgs } from 'node:util'
import { download, sha256 } from './files.mjs'

export const directory = join(dirname(fileURLToPath(import.meta.url)), '.data')
export const corpus = JSON.parse(readFileSync(new URL('./corpus.json', import.meta.url), 'utf8'))
export function corpusFor(suite) {
  if (!['quick', 'full', 'meetings'].includes(suite)) throw new Error('Use --suite quick|full|meetings')
  return suite === 'meetings' ? JSON.parse(readFileSync(new URL('./meetings.json', import.meta.url), 'utf8')) : corpus
}
const escape = (value) => String(value).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c])

export function validateCorpus(value) {
  if (!value.version || !value.samples?.length || new Set(value.samples.map((s) => s.id)).size !== value.samples.length) throw new Error('Empty or duplicate corpus')
  for (const s of value.samples) {
    if (!/^[a-z0-9-]+$/.test(s.id) || s.file !== `${s.id}.wav` || !value.sources[s.source]) throw new Error(`Invalid sample: ${s.id}`)
    if (!['zh', 'ja', 'en', 'zh-en', 'silence'].includes(s.language) || !Number.isFinite(s.duration) || !(s.duration > 0)) throw new Error(`Invalid language/duration: ${s.id}`)
    if (!['reference', 'ordered', 'provisional', 'unscored', 'silence'].includes(s.scoring) || !Array.isArray(s.reference)) throw new Error(`Missing reference policy: ${s.id}`)
    if (s.original && (!/^https:\/\//.test(s.original.url) || !Number.isFinite(s.original.offset) || s.original.offset < 0)) throw new Error(`Invalid original audio: ${s.id}`)
    for (const r of s.reference) {
      if (!(r.start >= 0 && r.start <= r.end && r.end <= s.duration + 0.001) || typeof r.text !== 'string') throw new Error(`Invalid reference span: ${s.id}`)
    }
    if (['reference', 'ordered', 'provisional'].includes(s.scoring) && !s.reference.some((r) => r.text.trim())) throw new Error(`Empty reference: ${s.id}`)
  }
}

export function listeningPage(value) {
  const cards = value.samples.map((s) => {
    const source = value.sources[s.source]
    const text = s.reference.map((r) => `[${r.start.toFixed(2)}–${r.end.toFixed(2)}] ${r.speaker}: ${r.text}`).join('\n')
    return `<article><h2>${escape(s.title)}</h2><p>${escape(s.language)} · ${(s.duration / 60).toFixed(1)} min · ${escape(s.scoring)}${s.quick ? ' · quick' : ''}${s.industry ? ` · ${escape(s.industry)} · ${escape(s.kind)}` : ''}</p>
      <audio controls preload="none" src="${escape(s.file)}" aria-label="${escape(s.title)}"></audio>
      <p><a href="${escape(s.file)}" download>Download WAV</a> · <a href="${escape(source.url)}">${escape(source.name)}</a> · <a href="${escape(source.licenseUrl)}">${escape(source.license)}</a></p>
      <p>${escape(s.note || '')}</p><details><summary>Reference transcript / 参考转写</summary><pre>${escape(text || '(silence)')}</pre></details></article>`
  }).join('\n')
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>boardcast · ASR audio</title>
    <style>body{font:16px/1.6 system-ui,sans-serif;max-width:1080px;margin:40px auto;padding:0 24px;background:#f6f5f0;color:#242824}h1{font-size:30px}h2{font-size:18px;margin:0}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:16px}article{padding:20px;background:white;border:1px solid #deded6;border-radius:12px}article p{font-size:14px;color:#62665f}audio{width:100%}a{color:#176251}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:14px/1.7 system-ui}summary{cursor:pointer}</style>
    <h1>中 / 日 / 英 · ASR listening samples</h1><p>${escape(value.version)} · 16 kHz / mono / PCM16 · Audio is downloaded locally.</p>
    <p>reference：发布方逐字稿；ordered：含重叠发言，按起始时间拼接的诊断分数；provisional：整理过的会议记录或待核转写；unscored：只展示文本。请查看每条录音的场景、参考文本与时间戳说明。来源和许可见 <a href="LICENSES.md">LICENSES.md</a>。</p><main class="grid">${cards}</main></html>`
}

export async function prepareAudio(value = corpus) {
  validateCorpus(value)
  for (const sample of value.samples) {
    const path = join(directory, sample.file)
    if (!sample.original) {
      await download({ ...sample, url: sample.url || `${value.baseUrl}/${sample.file}` }, path)
      continue
    }
    // Keep recordings with text-only/unclear redistribution grants at their upstream host.
    if (existsSync(path) && statSync(path).size === sample.bytes && await sha256(path) === sample.sha256) continue
    const original = join(directory, 'sources', `${sample.id}.audio`)
    await download(sample.original, original)
    const partial = `${path}.${process.pid}.part.wav`
    try {
      execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-y', '-fflags', '+bitexact', '-ss', String(sample.original.offset), '-i', original,
        '-t', String(sample.duration), '-map_metadata', '-1', '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', '-flags:a', '+bitexact', partial], { stdio: 'inherit' })
      if (statSync(partial).size !== sample.bytes || await sha256(partial) !== sample.sha256) throw new Error(`Converted audio SHA-256 mismatch: ${sample.id}; check the documented ffmpeg version`)
      renameSync(partial, path)
    } finally {
      rmSync(partial, { force: true })
    }
  }
  writeFileSync(join(directory, 'index.html'), listeningPage(value))
  writeFileSync(join(directory, 'LICENSES.md'), readFileSync(new URL('./LICENSES.md', import.meta.url)))
  console.log(`Audio ready: ${pathToFileURL(join(directory, 'index.html'))}`)
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  const { values } = parseArgs({ options: { suite: { type: 'string', default: 'full' } } })
  await prepareAudio(corpusFor(values.suite))
}
