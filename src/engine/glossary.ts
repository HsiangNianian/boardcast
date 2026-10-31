// 术语表：公司内部的叫法（项目代号、服务名、英文缩写）。语音常把它们听成别的字，模型也会写偏。
// 一行一条：「正确写法 = 错写1, 错写2」，只写正确写法也行。
// - 识别出来的话先按术语表纠正（整词替换，宁可少改不可改错），再交给模型；
// - 术语表也放进给模型的消息里，起名照标准写法。

export interface Term {
  term: string
  /** 常见的错写、读音 */
  also: string[]
}

export function parseGlossary(text: string): Term[] {
  const out: Term[] = []
  for (const raw of text.split('\n')) {
    const line = raw.replace(/#.*$/, '').trim()
    if (!line) continue
    const [left, right = ''] = line.split(/\s*[=＝]\s*/, 2)
    const term = left.trim()
    if (!term) continue
    const also = right
      .split(/[,，、;；]/)
      .map((s) => s.trim())
      .filter((s) => s && s !== term)
    const hit = out.find((t) => t.term === term)
    if (hit) hit.also.push(...also.filter((a) => !hit.also.includes(a)))
    else out.push({ term, also })
  }
  return out
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
    .flatMap((t) => [t.term, ...t.also].map((v) => ({ v, to: t.term })))
    .sort((a, b) => b.v.length - a.v.length)
  let out = text
  for (const { v, to } of rules) {
    const re = new RegExp(pattern(v), 'gi')
    out = out.replace(re, to)
  }
  return out
}

/** 给模型看的一行：标准写法，用顿号隔开 */
export function glossaryLine(terms: Term[]): string {
  return terms.map((t) => t.term).join('、')
}
