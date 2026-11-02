import { models } from './catalog.mjs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export function matrix(mode) {
  if (!['quick', 'full', 'sweep', 'meetings'].includes(mode)) throw new Error(`Unknown ASR matrix: ${mode}`)
  if (mode === 'meetings') return { include: [
    ...['sensevoice', 'paraformer-zh-en', 'reazonspeech', 'parakeet-unified-en', 'qwen3-asr-0.6b', 'whisper-small']
      .map((model) => ({ model, denoise: 'none', vad: 'on', policy: 'static' })),
    { model: 'sensevoice', denoise: 'none', vad: 'on', policy: 'dynamic' },
  ] }
  const include = []
  for (const model of mode === 'quick' ? ['sensevoice'] : ['sensevoice', 'whisper-base']) {
    for (const denoise of mode === 'quick' ? ['none'] : ['none', 'gtcrn']) {
      for (const vad of ['on', 'off']) include.push({ model, denoise, vad, policy: 'static' })
    }
  }
  include.push({ model: 'sensevoice', denoise: 'none', vad: 'on', policy: 'dynamic' })
  if (mode === 'sweep') for (const model of Object.keys(models).filter((m) => !['sensevoice', 'whisper-base'].includes(m))) {
    include.push({ model, denoise: 'none', vad: 'on', policy: 'static' })
  }
  return { include }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) console.log(JSON.stringify(matrix(process.argv[2])))
