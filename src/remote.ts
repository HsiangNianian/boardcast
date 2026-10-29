// 远程模型（用户自己填的 OpenAI 兼容接口）的存取和请求。
// - 桌面版：配置和 API key 存在外壳那边（remote.json，权限 0600），请求也由外壳代发，key 不进网页；
// - 网页版：只能存在本机浏览器的 localStorage 里，由浏览器直接请求。很多服务商不允许浏览器跨域调用，可能会连不上。

import { isDesktop } from './desktop.ts'
import { type ChatResponse, chatUrl } from './engine/llm.ts'

export interface RemoteView {
  baseUrl: string
  model: string
  enabled: boolean
  /** API key 的首尾几位，比如「sk-…a1b2」 */
  keyHint?: string
}

export interface RemoteTest {
  ms: number
  reply: string
}

export interface RemoteDraft {
  baseUrl: string
  /** 不填表示沿用已保存的 key */
  apiKey?: string
  model: string
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

// —— 网页版：存在 localStorage ——

const KEY = 'boardcast.remote'
interface Stored extends RemoteDraft {
  apiKey: string
  enabled: boolean
}

function read(): Stored {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '{}')
    return { baseUrl: v.baseUrl ?? '', apiKey: v.apiKey ?? '', model: v.model ?? '', enabled: !!v.enabled }
  } catch {
    return { baseUrl: '', apiKey: '', model: '', enabled: false }
  }
}

function hint(key: string): string | undefined {
  return key ? `${key.slice(0, 3)}…${key.slice(-4)}` : undefined
}

async function webPost(s: { baseUrl: string; apiKey: string }, body: Record<string, unknown>): Promise<ChatResponse> {
  if (!s.baseUrl.trim()) throw new Error('还没填 base URL')
  const headers: Record<string, string> = { 'content-type': 'application/json' }
  if (s.apiKey) headers.authorization = `Bearer ${s.apiKey}`
  let res: Response
  try {
    res = await fetch(chatUrl(s.baseUrl), { method: 'POST', headers, body: JSON.stringify(body) })
  } catch (e) {
    throw new Error(`连不上（网页版可能被服务商的跨域限制挡住了，桌面版没有这个问题）：${e}`)
  }
  const text = await res.text()
  if (!res.ok) {
    let msg = text.slice(0, 300)
    try {
      msg = JSON.parse(text).error?.message ?? msg
    } catch {
      /* 不是 JSON 就原样显示 */
    }
    throw new Error(`HTTP ${res.status}：${msg}`)
  }
  return JSON.parse(text) as ChatResponse
}

export const remote = {
  async get(): Promise<RemoteView> {
    if (isDesktop) return call<RemoteView>('remote_get')
    const s = read()
    return { baseUrl: s.baseUrl, model: s.model, enabled: s.enabled, keyHint: hint(s.apiKey) }
  },

  async save(d: RemoteDraft & { enabled: boolean }): Promise<RemoteView> {
    if (isDesktop)
      return call<RemoteView>('remote_save', {
        baseUrl: d.baseUrl,
        apiKey: d.apiKey ?? null,
        model: d.model,
        enabled: d.enabled,
      })
    const s = read()
    const next: Stored = {
      baseUrl: d.baseUrl.trim(),
      apiKey: d.apiKey !== undefined ? d.apiKey.trim() : s.apiKey,
      model: d.model.trim(),
      enabled: d.enabled,
    }
    try {
      localStorage.setItem(KEY, JSON.stringify(next))
    } catch {
      throw new Error('浏览器不让存设置')
    }
    return { baseUrl: next.baseUrl, model: next.model, enabled: next.enabled, keyHint: hint(next.apiKey) }
  },

  /** 发一条最小的对话请求确认能用；不传参数就测已保存的配置 */
  async test(d?: RemoteDraft): Promise<RemoteTest> {
    if (isDesktop)
      return call<RemoteTest>('remote_test', {
        baseUrl: d?.baseUrl ?? null,
        apiKey: d?.apiKey ?? null,
        model: d?.model ?? null,
      })
    const s = read()
    const model = (d?.model ?? s.model).trim()
    if (!model) throw new Error('还没填模型名')
    const t0 = performance.now()
    const r = await webPost(
      { baseUrl: d?.baseUrl ?? s.baseUrl, apiKey: d?.apiKey ?? s.apiKey },
      { model, messages: [{ role: 'user', content: '只回复 OK 两个字母。' }], max_tokens: 8, temperature: 0 },
    )
    const reply = r.choices?.[0]?.message?.content
    if (reply === undefined) throw new Error('返回里没有 choices[0].message.content，不像是 OpenAI 兼容接口')
    return { ms: Math.round(performance.now() - t0), reply: reply.trim().slice(0, 40) }
  },

  /** 用已保存的配置发对话请求，给 LlmConfig.send 用 */
  async send(body: Record<string, unknown>): Promise<ChatResponse> {
    if (isDesktop) return call<ChatResponse>('remote_chat', { body })
    const s = read()
    return webPost(s, { ...body, model: s.model })
  },
}
