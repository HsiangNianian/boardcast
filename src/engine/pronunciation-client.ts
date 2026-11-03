let worker: Worker | undefined
let nextId = 0

/** Keep dictionary decompression and tokenization off the UI thread. */
export async function pronunciations(texts: string[], language: 'zh' | 'ja'): Promise<string[][]> {
  if (typeof window === 'undefined') {
    const module = './pronunciation.ts'
    return (await import(/* @vite-ignore */ module)).pronunciationKeys(texts, language)
  }
  worker ??= new Worker(new URL('./pronunciation.worker.ts', import.meta.url), { type: 'module' })
  const current = worker
  const id = nextId++
  return new Promise((resolve, reject) => {
    const finish = (error?: string, keys?: string[][]) => {
      clearTimeout(timer)
      current.removeEventListener('message', message)
      current.removeEventListener('error', failed)
      if (error) reject(new Error(error))
      else resolve(keys!)
    }
    const message = ({ data }: MessageEvent) => { if (data.id === id) finish(data.error, data.keys) }
    const failed = () => {
      current.terminate()
      if (worker === current) worker = undefined
      finish('Pronunciation worker failed')
    }
    const timer = setTimeout(() => {
      current.terminate()
      if (worker === current) worker = undefined
      finish('Pronunciation dictionary load timed out')
    }, 15_000)
    current.addEventListener('message', message)
    current.addEventListener('error', failed)
    current.postMessage({ id, texts, language })
  })
}
