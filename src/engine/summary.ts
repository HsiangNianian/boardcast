// 会议摘要：按会议记录加板书结构，让模型写出结论、决定、待办（带负责人）、待讨论。
// 会开得长，一次塞不下：先把记录切成几段各自记要点，再连同板书合成一份。

import { type Graph, describe } from './graph.ts'
import { type ChatMessage, type LlmConfig, type LlmResult, chat } from './llm.ts'
import type { Utterance } from './prompt.ts'

export const SECTIONS = ['结论', '决定', '待办', '待讨论'] as const

/** 一段记录最多这么多字；本地小模型的上下文有限，再长就分段 */
const CHUNK = 3000

const RULES = `只根据给出的内容写，不要编造；没有的部分写「无」。
用简体中文，每条一行，以「- 」开头，简短。
待办写成「- [ ] 事情（负责人：某某）」，没提到负责人就不写括号。
严格按下面的格式输出，不要别的话：
### 结论
### 决定
### 待办
### 待讨论`

function transcript(lines: Utterance[]): string {
  return lines.map((l) => (l.speaker ? `${l.speaker}：${l.text}` : l.text)).join('\n')
}

/** 按字数把记录切成几段，不把一句话切开 */
export function chunks(lines: Utterance[], size = CHUNK): Utterance[][] {
  const out: Utterance[][] = []
  let cur: Utterance[] = []
  let n = 0
  for (const l of lines) {
    if (cur.length && n + l.text.length > size) {
      out.push(cur)
      cur = []
      n = 0
    }
    cur.push(l)
    n += l.text.length + 1
  }
  if (cur.length) out.push(cur)
  return out
}

export function summaryMessages(g: Graph, said: string, partial = false): ChatMessage[] {
  const system = partial
    ? '你在帮人整理会议记录。这是一场长会议中的一段，记下这一段里的要点：说了什么结论、定了什么、谁要做什么、还有什么没定。每条一行，以「- 」开头。'
    : `你在帮人写会议摘要。\n${RULES}`
  const board = describe(g)
  const user = partial ? `会议记录（片段）：\n${said}` : `白板上的内容：\n${board}\n\n${said}`
  return [
    { role: 'system', content: system },
    { role: 'user', content: user },
  ]
}

/** 「## 结论」「**结论**」「【待办事项】」「结论：」这些写法都认成小节标题 */
const HEADING = /^\s*(?:#{1,6}\s*)?(?:\*\*|【)?\s*(结论|决定|待办|待办事项|待讨论)\s*(?:\*\*|】)?\s*[:：]?\s*(?:\*\*)?\s*$/

/**
 * 模型的输出收拾一下：去掉代码块围栏和格式前面的废话，小节标题统一成「### 结论」，缺的小节补上「无」。
 * 一个小节都认不出来时原样返回（总比四个「无」强）；空的就是空的。
 */
export function tidySummary(text: string): string {
  const lines = text.replace(/```[a-z]*\n?/gi, '').trim().split('\n')
  let found = 0
  const out = lines.map((l) => {
    const m = HEADING.exec(l)
    if (!m) return l
    found++
    return `### ${m[1] === '待办事项' ? '待办' : m[1]}`
  })
  let t = out.join('\n').trim()
  if (!found) return t
  const first = t.indexOf('### ')
  if (first > 0) t = t.slice(first)
  for (const s of SECTIONS) if (!new RegExp(`^### ${s}$`, 'm').test(t)) t += `\n\n### ${s}\n- 无`
  return t.trim()
}

/** 发一次；推理模型可能把 token 全花在思考上、正文是空的，那就给多一倍再试一次 */
async function ask(cfg: LlmConfig, msgs: ChatMessage[], tokens: number, signal?: AbortSignal): Promise<LlmResult> {
  let r = await chat(cfg, msgs, tokens, signal)
  if (!r.text.trim() && !signal?.aborted) r = await chat(cfg, msgs, Math.min(tokens * 2, 8192), signal)
  if (!r.text.trim()) {
    const why = r.finish === 'length' || r.reasoning ? '模型把 token 都用在思考上了，没写出正文' : '模型返回了空内容'
    throw new Error(`${why}${r.via ? `（${r.via}）` : ''}`)
  }
  return r
}

export interface SummaryResult {
  text: string
  /** 一共调了几次模型 */
  calls: number
  ms: number
  via?: string
  fellBack?: string
}

export async function summarize(
  cfg: LlmConfig,
  g: Graph,
  lines: Utterance[],
  signal?: AbortSignal,
  progress?: (done: number, total: number) => void,
): Promise<SummaryResult> {
  const t0 = performance.now()
  // 远程的可能是推理模型，先想再写，给足余量
  const remote = cfg.flavor === 'openai'
  const parts = chunks(lines)
  let said = `会议记录：\n${transcript(lines) || '（没有记录）'}`
  let last: LlmResult | undefined
  let calls = 0
  if (parts.length > 1) {
    const notes: string[] = []
    for (const [i, p] of parts.entries()) {
      progress?.(i, parts.length + 1)
      last = await ask(cfg, summaryMessages(g, transcript(p), true), remote ? 2048 : 600, signal)
      calls++
      notes.push(`第 ${i + 1} 段：\n${last.text.trim()}`)
    }
    said = `会议记录太长，下面是逐段记下的要点：\n${notes.join('\n\n')}`
    progress?.(parts.length, parts.length + 1)
  }
  const r = await ask(cfg, summaryMessages(g, said), remote ? 4096 : 1024, signal)
  calls++
  const text = tidySummary(r.text)
  if (!text) throw new Error('模型没有返回摘要内容')
  return { text, calls, ms: performance.now() - t0, via: r.via, fellBack: r.fellBack ?? last?.fellBack }
}

/** 「仅文本」复制：去掉 Markdown 记号，待办写成方框，列表写成圆点 */
export function plainText(md: string): string {
  return md
    .split('\n')
    .map((l, i) =>
      l
        // 小节之间空一行
        .replace(/^#{1,6}\s*/, i ? '\n' : '')
        .replace(/^(\s*)[-*+]\s+\[[xX]\]\s*/, '$1☑ ')
        .replace(/^(\s*)[-*+]\s+\[ \]\s*/, '$1☐ ')
        .replace(/^(\s*)[-*+]\s+/, '$1• ')
        .replace(/\*\*(.+?)\*\*/g, '$1')
        .replace(/~~(.+?)~~/g, '$1')
        .replace(/`([^`]+)`/g, '$1'),
    )
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}
