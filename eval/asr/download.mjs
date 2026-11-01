import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { download } from './files.mjs'

export const directory = join(dirname(fileURLToPath(import.meta.url)), '.data')
export const corpus = JSON.parse(readFileSync(new URL('./corpus.json', import.meta.url), 'utf8'))
const escape = (value) => String(value).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c])

export function validateCorpus(value) {
  if (!value.version || !value.samples?.length || new Set(value.samples.map((s) => s.id)).size !== value.samples.length) throw new Error('Empty or duplicate corpus')
  for (const s of value.samples) {
    if (!/^[a-z0-9-]+$/.test(s.id) || s.file !== `${s.id}.wav` || !value.sources[s.source]) throw new Error(`Invalid sample: ${s.id}`)
    if (!['zh', 'ja', 'en', 'zh-en', 'silence'].includes(s.language) || !(s.duration > 0)) throw new Error(`Invalid language/duration: ${s.id}`)
    if (!['reference', 'provisional', 'unscored', 'silence'].includes(s.scoring) || !Array.isArray(s.reference)) throw new Error(`Missing reference policy: ${s.id}`)
    for (const r of s.reference) {
      if (!(r.start >= 0 && r.start <= r.end && r.end <= s.duration + 0.001) || typeof r.text !== 'string') throw new Error(`Invalid reference span: ${s.id}`)
    }
    if (['reference', 'provisional'].includes(s.scoring) && !s.reference.some((r) => r.text.trim())) throw new Error(`Empty reference: ${s.id}`)
  }
}

export function listeningPage(value) {
  const cards = value.samples.map((s) => {
    const source = value.sources[s.source]
    const text = s.reference.map((r) => `[${r.start.toFixed(2)}–${r.end.toFixed(2)}] ${r.speaker}: ${r.text}`).join('\n')
    return `<article><h2>${escape(s.title)}</h2><p>${escape(s.language)} · ${s.duration.toFixed(2)} s · ${escape(s.scoring)}${s.quick ? ' · quick' : ''}</p>
      <audio controls preload="none" src="${escape(s.file)}" aria-label="${escape(s.title)}"></audio>
      <p><a href="${escape(s.file)}" download>Download WAV</a> · <a href="${escape(source.url)}">${escape(source.name)}</a> · <a href="${escape(source.licenseUrl)}">${escape(source.license)}</a></p>
      <p>${escape(s.note || '')}</p><details><summary>Reference transcript / 参考转写</summary><pre>${escape(text || '(silence)')}</pre></details></article>`
  }).join('\n')
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>boardcast · ASR audio</title>
    <style>body{font:16px/1.6 system-ui,sans-serif;max-width:1080px;margin:40px auto;padding:0 24px;background:#f6f5f0;color:#242824}h1{font-size:30px}h2{font-size:18px;margin:0}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:16px}article{padding:20px;background:white;border:1px solid #deded6;border-radius:12px}article p{font-size:14px;color:#62665f}audio{width:100%}a{color:#176251}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:14px/1.7 system-ui}summary{cursor:pointer}</style>
    <h1>中 / 日 / 英 · ASR listening samples</h1><p>${escape(value.version)} · 16 kHz / mono / PCM16 · Audio is downloaded locally.</p>
    <p>中文、英文为会议；日文为 FLEURS 真人朗读，尚不覆盖日语会议。中文社区派生转写标为 provisional；重叠会议整段只展示转写，不计入普通 CER/WER。所有来源和许可见 <a href="LICENSES.md">LICENSES.md</a>。</p><main class="grid">${cards}</main></html>`
}

export async function prepareAudio() {
  validateCorpus(corpus)
  for (const sample of corpus.samples) {
    await download({ ...sample, url: `${corpus.baseUrl}/${sample.file}` }, join(directory, sample.file))
  }
  writeFileSync(join(directory, 'index.html'), listeningPage(corpus))
  writeFileSync(join(directory, 'LICENSES.md'), readFileSync(new URL('./LICENSES.md', import.meta.url)))
  console.log(`Audio ready: ${pathToFileURL(join(directory, 'index.html'))}`)
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) await prepareAudio()
