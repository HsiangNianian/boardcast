// 桌面版（Tauri）才有的能力。网页版里这些函数都返回 null 或什么都不做，界面照旧用手填的模型地址。

import { msg } from './i18n/index.ts'
import { en } from './i18n/en.ts'
import type { SpeechAssetState } from './engine/speech.ts'
import type { ChatResponse } from './engine/llm.ts'
import { isLoopback } from './privacy.ts'

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

/** Format at display time so an existing status follows language changes. */
export function llmProblem(status: LlmStatus): string {
  const t = msg().errors
  if (status.state === 'no-model') return t.noBoardModel
  const message = status.message ?? ''
  if (message === 'llama-server was not found in the app installation') return t.modelEngineMissing
  // shortcut: match the native message format exactly; update together if that contract changes.
  const start = /^llama-server failed to start; log: ([\s\S]+)$/.exec(message)
  if (start) return t.modelStartFailed(start[1])
  const exit = /^llama-server exited unexpectedly \((.*?)\); log: ([\s\S]+)$/.exec(message)
  if (exit) return t.modelExited(exit[1], exit[2])
  return message || msg().app.builtinUnavailable
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

export function modelCopy(model: Pick<ModelInfo, 'id' | 'label' | 'note'>): { label: string; note: string } {
  const catalog: Record<string, { label: string; note: string }> = msg().models.catalog
  return Object.hasOwn(catalog, model.id) ? catalog[model.id] : model
}

function modelName(value: string): string {
  const entry = Object.entries(en.models.catalog).find(([id, model]) => value === id || value === model.label)
  return entry ? modelCopy({ id: entry[0], ...entry[1] }).label : value
}

/** Keep native data raw; existing progress and errors follow the current language on render. */
export function modelMessage(message: string): string {
  const s = msg().models
  switch (message) {
    case 'App data folder not found': return s.dataFolderMissing
    case 'Download in progress; cancel it first': return s.cancelBeforeDelete
    case 'Verifying the downloaded portion…': return s.verifyingPartial
    case 'Verifying file…': return s.verifyingFile
    case 'All download sources failed': return s.sourcesFailed
  }
  // shortcut: match fixed native formats; update these matches if models.rs changes its messages.
  let m = /^Model not found: ([\s\S]+)$/.exec(message)
  if (m) return s.modelMissing(m[1])
  m = /^Not enough disk space: (.+) needs (\d+\.\d) GB more \(plus (\d+\.\d) GB spare\); only (\d+\.\d) GB is free on the model folder's disk$/.exec(message)
  if (m) return s.diskSpace(modelName(m[1]), m[2], m[3], m[4])
  m = /^(.+) is downloading; wait for it to finish or cancel it first$/.exec(message)
  if (m) return s.otherDownloading(modelName(m[1]))
  m = /^(.+) can't be selected as the built-in model$/.exec(message)
  if (m) return s.cannotSelect(modelName(m[1]))
  m = /^(.+) has not been downloaded yet$/.exec(message)
  if (m) return s.notDownloaded(modelName(m[1]))
  m = /^Couldn't check free disk space: ([\s\S]+)$/.exec(message)
  if (m) return s.spaceCheckFailed(m[1])
  m = /^Couldn't create the folder: ([\s\S]+)$/.exec(message)
  if (m) return s.createFolderFailed(m[1])
  m = /^Couldn't write to the model folder: ([\s\S]+)$/.exec(message)
  if (m) return s.writeFolderFailed(m[1])
  m = /^Write failed: ([\s\S]+)$/.exec(message)
  if (m) return s.writeFailed(m[1])
  m = /^Verification failed \(sha256 ([a-f0-9]{64})\); file deleted\. Please download it again$/.exec(message)
  if (m) return s.verificationFailed(m[1])
  return message
}

export interface VoiceStatus {
  installed: boolean
  listening: boolean
  device?: string
  message?: string
  language?: string | null
}

export type VoiceSource = 'mic' | 'both' | 'system'

export interface InputCheckStatus {
  id: string
  state: 'starting' | 'active' | 'stopped' | 'error'
  device?: string
  level?: number
  message?: string
  reason?: 'cancelled' | 'timeout'
}

export type SpeechProvider = 'sensevoice' | 'apple'
export interface SpeechModels {
  selection: { provider: SpeechProvider; locale: string }
  senseVoiceInstalled: boolean
  listening: boolean
  preparing: boolean
  autoLocales: string[]
  activeLocale: string | null
  apple: {
    isApple: boolean
    available: boolean
    reason: 'platform' | 'os' | 'hardware' | null
    defaultLocale: string
    installedLocales: string[]
    osVersion: string
    locales: { id: string; name: string; status: SpeechAssetState }[]
  } | null
  appleError: string | null
}

export interface Heard {
  id: string
  revision: number
  sources: string[]
  quality?: { rms: number; peak: number; clipped: number; sourceRms: number[] }
  review?: { reason: string; action: string; original: string; candidate: string; decision: string; asrMs: number }
  text: string
  start: number
  end: number
  /** 这句话的声纹（单位长度）；没有声纹模型或者句子太短时没有 */
  voiceprint?: number[]
  timing?: HeardTiming
}

/** Where the time went in the shell before a sentence reached the app (see voice.rs `Timing`) */
export interface HeardTiming {
  /** Pause waited for after the sentence ended */
  waitMs: number
  asrMs: number
  voiceprintMs: number
  /** Ended at a short breath in a long sentence rather than a real pause */
  soft: boolean
  reviewMs?: number
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

export async function nativeRequest<T>(request: (requestId: string) => Promise<T>, signal?: AbortSignal): Promise<T> {
  signal?.throwIfAborted()
  const requestId = await call<string>('remote_prepare')
  const cancel = () => { void call<void>('remote_cancel', { requestId }).catch(() => {}) }
  try {
    signal?.addEventListener('abort', cancel, { once: true })
    signal?.throwIfAborted()
    const reply = await request(requestId)
    signal?.throwIfAborted()
    return reply
  } catch (error) {
    signal?.throwIfAborted()
    throw error
  } finally {
    signal?.removeEventListener('abort', cancel)
    cancel()
  }
}

export async function llmStatus(): Promise<LlmStatus | null> {
  return isDesktop ? call<LlmStatus>('llm_status') : null
}

/** Desktop requests stay in the native process: WebView network policy must not gate our own server. */
export async function localChat(baseUrl: string, body: Record<string, unknown>, signal?: AbortSignal): Promise<ChatResponse> {
  signal?.throwIfAborted()
  if (!isLoopback(baseUrl)) throw new Error(msg().errors.localAddress)
  if (isDesktop) {
    try {
      return await nativeRequest<ChatResponse>(requestId => call('llm_chat', { baseUrl, body, requestId }), signal)
    } catch (error) {
      if (error === 'LOCAL_MODEL_UNAVAILABLE') throw new Error(msg().app.builtinUnavailable)
      throw error
    }
  }
  const response = await fetch(`${baseUrl}/v1/chat/completions`, {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
    signal, redirect: 'error',
  })
  if (!response.ok) throw new Error(msg().errors.localHttp(response.status, (await response.text()).slice(0, 300)))
  return response.json()
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
    if (s.state === 'no-model' || s.state === 'error') throw new Error(llmProblem(s))
    await new Promise((r) => setTimeout(r, 700))
    s = await call<LlmStatus>('llm_status')
  }
  throw new Error(msg().errors.builtinTimeout)
}

export const models = {
  list: () => call<Catalog>('models_list'),
  download: (id: string) => call<void>('model_download', { id }),
  cancel: () => call<void>('model_cancel'),
  select: (id: string) => call<void>('model_select', { id }),
  remove: (id: string) => call<void>('model_delete', { id }),
}

export const exportedFiles = {
  open: (path: string) => call<void>('export_open', { path }),
  reveal: (path: string) => call<void>('export_reveal', { path }),
}

export const voice = {
  models: () => call<SpeechModels>('voice_models'),
  select: (provider: SpeechProvider, locale = 'auto') => call<void>('voice_select', { provider, locale }),
  prepare: (locale: string) => call<void>('voice_prepare', { locale }),
  status: () => call<VoiceStatus>('voice_status'),
  devices: () => call<string[]>('voice_devices'),
  /** `source`：只收麦克风、麦克风加系统声音（线上会议），或者只收系统声音 */
  start: (device?: string, source: VoiceSource = 'mic') => call<void>('voice_start', { device: device ?? null, source }),
  stop: () => call<void>('voice_stop'),
  inputCheck: {
    start: (id: string, device: string, source: VoiceSource) => call<void>('voice_input_check_start', { id, device: device || null, source }),
    stop: (id?: string) => call<void>('voice_input_check_stop', { id: id ?? null }),
  },
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

/** 按 arXiv 编号下载论文 PDF（外壳代下，只认 arxiv.org），返回字节 */
export async function arxivPdf(id: string, signal?: AbortSignal): Promise<Uint8Array> {
  const b64 = await nativeRequest(requestId => call<string>('arxiv_pdf', { id, requestId }), signal)
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

export type ShellMenuAction = 'settings' | 'new-meeting' | 'meetings' | 'import' | 'export' | 'keys' | 'diagnostics' | 'check-updates' | 'undo' | 'redo' | 'select-all'
export interface ShellExitRequest { id: number; reason: 'close' | 'quit' }

/** Native menu actions and explicit save-before-exit handshake. */
export const shell = {
  setShortcut: (accel: string | null) => call<void>('shortcut_set', { accel }),
  wayland: () => call<boolean>('session_wayland'),
  onToggle: (cb: () => void) => on<unknown>('mic-toggle', cb),
  takeToggle: () => call<boolean>('shell_take_toggle'),
  showWindow: () => call<void>('shell_show_window'),
  setLocale: (locale: 'en' | 'zh') => call<void>('shell_set_locale', { locale }),
  onMenuAction: (cb: (action: ShellMenuAction) => void) => on<ShellMenuAction>('shell-menu-action', cb),
  onExitRequested: (cb: (request: ShellExitRequest) => void) => on<ShellExitRequest>('shell-exit-request', cb),
  pendingExit: () => call<ShellExitRequest | null>('shell_pending_exit'),
  resolveExit: (id: number, allow: boolean) => call<boolean>('shell_resolve_exit', { id, allow }),
}

/** 诊断信息：系统和 llama-server 日志的末尾 */
export const diagnosticsInfo = () => call<{ os: string; arch: string; log: string | null }>('diagnostics_info')

/** 自动更新：检查、下载安装并重启（成功的话 install 不会返回） */
export const updates = {
  version: async () => (await import('@tauri-apps/api/app')).getVersion(),
  check: async () => {
    try {
      return await call<UpdateInfo | null>('update_check')
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error)
      throw new Error(detail === 'UPDATE_CHECK_TIMEOUT' ? msg().update.checkTimedOut : msg().update.checkFailed(detail))
    }
  },
  download: () => call<void>('update_download'),
  install: () => call<void>('update_install'),
  onProgress: (cb: (p: UpdateProgress) => void) => on<UpdateProgress>('update-progress', cb),
}
