// 术语表：公司内部的叫法（项目代号、服务名、英文缩写）。语音常把它们听成别的字，模型也会写偏。
// 一行一条：「正确写法 = 错写1, 错写2」，只写正确写法也行。
// - 识别出来的话先按术语表纠正（整词替换，宁可少改不可改错），再交给模型；
// - 术语表也放进给模型的消息里，起名照标准写法。

import type { Utterance } from './prompt.ts'

export function correctUtterance(u: Utterance, terms: Term[]): Utterance {
  const text = correct(u.text, terms)
  return { ...u, text, rawText: u.rawText ?? (text !== u.text ? u.text : undefined) }
}

export interface Term {
  term: string
  /** 常见的错写、读音 */
  also: string[]
}

const usable = (s: string) => /[^\s_-]/.test(s)

export function readGlossary(text: string): { terms: Term[]; invalidLines: number[] } {
  const out: Term[] = []
  const invalidLines: number[] = []
  for (const [index, raw] of text.split('\n').entries()) {
    const line = raw.replace(/(^|\s)#.*$/, '').trim()
    if (!line) continue
    const parts = line.split(/[=＝]/)
    const term = parts[0].trim()
    const aliases = (parts[1] ?? '')
      .split(/[,，、;；]/)
      .map((s) => s.trim())
      .filter(Boolean)
    if (parts.length > 2 || !usable(term) || (parts.length === 2 && (!aliases.length || aliases.some((s) => !usable(s))))) {
      invalidLines.push(index + 1)
      continue
    }
    const also = [...new Set(aliases)].filter((s) => s !== term)
    const hit = out.find((t) => t.term === term)
    if (hit) hit.also.push(...also.filter((a) => !hit.also.includes(a)))
    else out.push({ term, also })
  }
  return { terms: out, invalidLines }
}

export function parseGlossary(text: string): Term[] {
  return readGlossary(text).terms
}

export function formatGlossary(terms: Term[]): string {
  return terms.map((t) => (t.also.length ? `${t.term} = ${t.also.join(', ')}` : t.term)).join('\n')
}

const latin = (s: string) => /^[\x20-\x7e]+$/.test(s)
const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** 英文写法拆成词：MediaCrawler → media crawler，graphRAG → graph rag，API-Gateway → api gateway */
function words(s: string): string[] {
  return s
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .split(/[\s\-_]+/)
    .filter(Boolean)
}

/** 一个写法对应的正则：英文按词匹配（不区分大小写，词之间空格、横线可有可无，前后不能连着字母数字）；
 * 带中文的原样匹配，只是空格可有可无、英文部分不区分大小写（「api网关」也认） */
function pattern(s: string): string {
  if (!latin(s)) return escape(s).replace(/\s+/g, '\\s*')
  const ws = words(s)
  return `(?<![A-Za-z0-9])${ws.map(escape).join('[\\s\\-_]*')}(?![A-Za-z0-9])`
}

/** 按术语表纠正一句话 */
export function correct(text: string, terms: Term[]): string {
  if (!terms.length || !text) return text
  // 长的先换：「订单中心服务」要在「订单中心」之前
  const rules = terms
    .filter((t) => usable(t.term))
    .flatMap((t) => [t.term, ...t.also].filter(usable).map((v) => ({
      v, to: t.term, canonical: v === t.term, source: pattern(v), match: new RegExp(`^(?:${pattern(v)})$`, 'i'),
    })))
    .sort((a, b) => b.v.length - a.v.length)
  if (!rules.length) return text
  // Match the original once: a corrected name must not become another rule's input.
  return text.replace(new RegExp(rules.map((r) => `(?:${r.source})`).join('|'), 'gi'), (matched) => {
    const hits = rules.filter((r) => r.match.test(matched))
    const names = hits.filter((r) => r.canonical)
    const outputs = new Set((names.length ? names : hits).map((r) => r.to))
    return outputs.size === 1 ? [...outputs][0] : matched
  })
}

/** 给模型看的一行：标准写法，用顿号隔开 */
export function glossaryLine(terms: Term[]): string {
  return terms.map((t) => t.term).join('、')
}
