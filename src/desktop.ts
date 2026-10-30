// 桌面版（Tauri）才有的能力。网页版里这些函数都返回 null 或什么都不做，界面照旧用手填的模型地址。

export const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface Gpu {
  id: string
  name: string
  totalMib: number
  discrete: boolean
}

export interface LlmStatus {
  /** idle：远程模型优先，内置模型还没启动 */
  state: 'idle' | 'starting' | 'ready' | 'no-model' | 'error'
  url?: string
  model?: string
  device?: string
  message?: string
  modelsDir?: string
  gpus: Gpu[]
  /** 带着看图组件（导入图片时能读字） */
  vision?: boolean
}

export interface ModelInfo {
  id: string
  label: string
  note: string
  kind: 'llm' | 'voice' | 'vision'
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

export type VoiceSource = 'mic' | 'both' | 'system'

export interface Heard {
  text: string
  start: number
  end: number
  /** 这句话的声纹（单位长度）；没有声纹模型或者句子太短时没有 */
  voiceprint?: number[]
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

/** 内置模型在待命就拉起来，已经在跑就什么都不做 */
export const llmEnsure = () => call<LlmStatus>('llm_ensure')
/** 停掉内置模型（远程模型能用时，把显存让出来） */
export const llmStop = () => call<void>('llm_stop')

/** 拉起内置模型并等它就绪，返回它的地址；没有模型或者起不来就抛错 */
export async function localReady(timeoutMs = 240_000): Promise<string> {
  let s = await llmEnsure()
  const t0 = Date.now()
  while (Date.now() - t0 < timeoutMs) {
    if (s.state === 'ready' && s.url) return s.url
    if (s.state === 'no-model' || s.state === 'error') throw new Error(s.message ?? '内置模型不可用')
    await new Promise((r) => setTimeout(r, 700))
    s = await call<LlmStatus>('llm_status')
  }
  throw new Error('内置模型启动超时')
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
  /** `source`：只收麦克风、麦克风加系统声音（线上会议），或者只收系统声音 */
  start: (device?: string, source: VoiceSource = 'mic') => call<void>('voice_start', { device: device ?? null, source }),
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

export interface UpdateInfo {
  version: string
  current: string
  /** 更新说明（Release 正文） */
  notes?: string
  date?: string
}

export interface UpdateProgress {
  done: number
  total?: number
}

/** 自动更新：检查、下载安装并重启（成功的话 install 不会返回） */
export const updates = {
  version: async () => (await import('@tauri-apps/api/app')).getVersion(),
  check: () => call<UpdateInfo | null>('update_check'),
  install: () => call<void>('update_install'),
  onProgress: (cb: (p: UpdateProgress) => void) => on<UpdateProgress>('update-progress', cb),
}
