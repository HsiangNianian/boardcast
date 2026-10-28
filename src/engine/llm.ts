// 调本地 llama-server（OpenAI 兼容接口 + llama.cpp 自己的扩展参数）。

import type { Graph } from './graph.ts'
import { type Utterance, grammar, messages } from './prompt.ts'

export interface LlmConfig {
  baseUrl: string
  model?: string
  /** 关掉 Qwen 的思考模式；有些模型不认这个参数，会忽略 */
  noThink?: boolean
  maxTokens?: number
}

export const defaultLlm: LlmConfig = { baseUrl: 'http://127.0.0.1:8080', noThink: true, maxTokens: 160 }

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
}

export async function proposeOps(
  cfg: LlmConfig,
  g: Graph,
  recent: Utterance[],
  next: Utterance,
  signal?: AbortSignal,
): Promise<LlmResult> {
  const body: Record<string, unknown> = {
    model: cfg.model ?? 'local',
    messages: messages(g, recent, next),
    temperature: 0,
    max_tokens: cfg.maxTokens ?? 160,
    grammar: grammar(g),
    cache_prompt: true,
  }
  if (cfg.noThink) body.chat_template_kwargs = { enable_thinking: false }
  const t0 = performance.now()
  const res = await fetch(`${cfg.baseUrl}/v1/chat/completions`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  })
  if (!res.ok) throw new Error(`llama-server ${res.status}: ${await res.text()}`)
  const data = (await res.json()) as {
    choices: { message: { content: string } }[]
    usage?: { prompt_tokens?: number; completion_tokens?: number }
    timings?: { prompt_n?: number; prompt_ms?: number; predicted_n?: number; predicted_ms?: number }
  }
  const ms = performance.now() - t0
  return {
    text: data.choices[0]?.message.content ?? '',
    ms,
    promptTokens: data.usage?.prompt_tokens,
    promptEvaluated: data.timings?.prompt_n,
    promptMs: data.timings?.prompt_ms,
    outputTokens: data.timings?.predicted_n ?? data.usage?.completion_tokens,
    outputMs: data.timings?.predicted_ms,
  }
}
