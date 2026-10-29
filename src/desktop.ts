// 桌面版（Tauri）才有的能力。网页版里这些函数都返回 null 或什么都不做，界面照旧用手填的模型地址。

export const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface Gpu {
  id: string
  name: string
  totalMib: number
  discrete: boolean
}

export interface LlmStatus {
  state: 'starting' | 'ready' | 'no-model' | 'error'
  url?: string
  model?: string
  device?: string
  message?: string
  modelsDir?: string
  gpus: Gpu[]
}

export interface ModelInfo {
  id: string
  label: string
  note: string
  kind: 'llm' | 'voice'
  size: number
  installed: boolean
  partial: number
  selected: boolean
  recommended: boolean
}

export interface Catalog {
  models: ModelInfo[]
  dir?: string
  downloading?: string
  freeBytes?: number
}

export interface VoiceStatus {
  installed: boolean
  listening: boolean
  device?: string
  message?: string
}

export interface Heard {
  text: string
  start: number
  end: number
}

export interface Progress {
  id: string
  state: 'downloading' | 'verifying' | 'done' | 'error' | 'canceled'
  downloaded: number
  total: number
  speed: number
  source?: string
  message?: string
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

export async function llmStatus(): Promise<LlmStatus | null> {
  return isDesktop ? call<LlmStatus>('llm_status') : null
}

export const models = {
  list: () => call<Catalog>('models_list'),
  download: (id: string) => call<void>('model_download', { id }),
  cancel: () => call<void>('model_cancel'),
  select: (id: string) => call<void>('model_select', { id }),
  remove: (id: string) => call<void>('model_delete', { id }),
}

export const voice = {
  status: () => call<VoiceStatus>('voice_status'),
  devices: () => call<string[]>('voice_devices'),
  start: (device?: string) => call<void>('voice_start', { device: device ?? null }),
  stop: () => call<void>('voice_stop'),
}

/** 订阅外壳推来的事件，返回取消订阅的函数 */
export async function on<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isDesktop) return () => {}
  const { listen } = await import('@tauri-apps/api/event')
  return listen<T>(event, (e) => cb(e.payload))
}

/** 订阅下载进度 */
export const onProgress = (cb: (p: Progress) => void) => on<Progress>('model-progress', cb)
