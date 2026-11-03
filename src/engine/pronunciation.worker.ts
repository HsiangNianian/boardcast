import { pronunciationKeys } from './pronunciation.ts'

self.onmessage = async ({ data }: MessageEvent<{ id: number; texts: string[]; language: 'zh' | 'ja' }>) => {
  try {
    self.postMessage({ id: data.id, keys: await pronunciationKeys(data.texts, data.language) })
  } catch (error) {
    self.postMessage({ id: data.id, error: String(error) })
  }
}
