// 调模型出操作。两种接口：
// - llamacpp（默认）：本地 llama-server，OpenAI 兼容接口加上 llama.cpp 自己的扩展参数
//   （grammar 约束输出、cache_prompt 复用前缀、关掉 Qwen 的思考模式）；
// - openai：用户自己填的远程接口，只发 OpenAI 标准字段。没有语法约束，靠提示词和 parseOps 的容错。
// 远程失败时可以退回 `fallback`（一般是内置的本地模型），这一句照样能出板书。

import type { Graph } from './graph.ts'
import { type Utterance, grammar, messages } from './prompt.ts'

export interface LlmConfig {
  baseUrl: string
  model?: string
  /** 关掉 Qwen 的思考模式；有些模型不认这个参数，会忽略 */
  noThink?: boolean
  maxTokens?: number
  flavor?: 'llamacpp' | 'openai'
  /** 自己发请求（桌面版由外壳代发远程请求，key 不进网页）；不给就用 fetch */
  send?: (body: Record<string, unknown>) => Promise<ChatResponse>
  apiKey?: string
  /** 这个接口失败时改用的接口 */
  fallback?: LlmConfig
  /** 给界面看的名字 */
  label?: string
}

export const defaultLlm: LlmConfig = { baseUrl: 'http://127.0.0.1:8080', noThink: true, maxTokens: 160 }

export interface ChatResponse {
  choices: { message: { content: string } }[]
  usage?: { prompt_tokens?: number; completion_tokens?: number }
  timings?: { prompt_n?: number; prompt_ms?: number; predicted_n?: number; predicted_ms?: number }
}

export interface LlmResult {
  text: string
  /** 整个请求的墙钟时间 */
  ms: number
  promptTokens?: number
  /** 真正需要重新读入的 token（命中前缀缓存的部分不算） */
  promptEvaluated?: number
  promptMs?: number
  outputTokens?: number
  outputMs?: number
  /** 实际是哪个接口出的结果 */
  via?: string
  /** 首选接口失败、改用了 fallback 时，首选接口的错误 */
  fellBack?: string
}

/** 有些推理模型会把思考过程放在 <think> 里，去掉再解析 */
export function stripThinking(text: string): string {
  return text.replace(/<think>[\s\S]*?(<\/think>|$)/g, '').trim()
}

/** `https://api.openai.com/v1` 和完整的 `/chat/completions` 都认 */
export function chatUrl(base: string): string {
  const b = base.trim().replace(/\/+$/, '')
  return b.endsWith('/chat/completions') ? b : `${b}/chat/completions`
}

/**
 * 远程接口：先试着关掉思考模式（vLLM / SGLang / llama.cpp 认 chat_template_kwargs，
 * 通义千问的接口认顶层的 enable_thinking）；严格的服务（比如 OpenAI 官方）会因为不认识的字段回 400，
 * 那就去掉重试，并记住这个地址以后都不发。
 */
const strictServers = new Set<string>()

function body(cfg: LlmConfig, g: Graph, recent: Utterance[], next: Utterance, extras = true): Record<string, unknown> {
  const b: Record<string, unknown> = {
    model: cfg.model ?? 'local',
    messages: messages(g, recent, next),
    temperature: 0,
    // 远程模型可能是推理模型，先想再答，给够余量；本地有语法约束，160 足够
    max_tokens: cfg.maxTokens ?? (cfg.flavor === 'openai' ? 1024 : 160),
  }
  if (cfg.flavor !== 'openai') {
    b.grammar = grammar(g)
    b.cache_prompt = true
    if (cfg.noThink) b.chat_template_kwargs = { enable_thinking: false }
  } else if (extras && cfg.noThink !== false && !strictServers.has(cfg.baseUrl)) {
    b.chat_template_kwargs = { enable_thinking: false }
    b.enable_thinking = false
  }
  return b
}

/** 是不是「不认识这个参数」一类的 400 */
function rejectsExtras(e: unknown): boolean {
  const s = String(e)
  return /\b400\b/.test(s) && /unrecogni[sz]ed|unknown|unexpected|extra|not permitted|additional properties|enable_thinking|chat_template_kwargs/i.test(s)
}

async function post(cfg: LlmConfig, b: Record<string, unknown>, signal?: AbortSignal): Promise<ChatResponse> {
  if (cfg.send) return cfg.send(b)
  const url = cfg.flavor === 'openai' ? chatUrl(cfg.baseUrl) : `${cfg.baseUrl}/v1/chat/completions`
  const headers: Record<string, string> = { 'content-type': 'application/json' }
  if (cfg.apiKey) headers.authorization = `Bearer ${cfg.apiKey}`
  const res = await fetch(url, { method: 'POST', headers, body: JSON.stringify(b), signal })
  if (!res.ok) throw new Error(`${cfg.label ?? '模型'} ${res.status}: ${await res.text()}`)
  return (await res.json()) as ChatResponse
}

export async function proposeOps(
  cfg: LlmConfig,
  g: Graph,
  recent: Utterance[],
  next: Utterance,
  signal?: AbortSignal,
): Promise<LlmResult> {
  const t0 = performance.now()
  let data: ChatResponse
  try {
    try {
      data = await post(cfg, body(cfg, g, recent, next), signal)
    } catch (e) {
      if (cfg.flavor !== 'openai' || strictServers.has(cfg.baseUrl) || !rejectsExtras(e)) throw e
      strictServers.add(cfg.baseUrl)
      data = await post(cfg, body(cfg, g, recent, next, false), signal)
    }
  } catch (e) {
    if (!cfg.fallback || signal?.aborted) throw e
    const r = await proposeOps(cfg.fallback, g, recent, next, signal)
    return { ...r, fellBack: String(e) }
  }
  const ms = performance.now() - t0
  const raw = data.choices?.[0]?.message?.content ?? ''
  return {
    text: cfg.flavor === 'openai' ? stripThinking(raw) : raw,
    ms,
    promptTokens: data.usage?.prompt_tokens,
    promptEvaluated: data.timings?.prompt_n,
    promptMs: data.timings?.prompt_ms,
    outputTokens: data.timings?.predicted_n ?? data.usage?.completion_tokens,
    outputMs: data.timings?.predicted_ms,
    via: cfg.label,
  }
}
