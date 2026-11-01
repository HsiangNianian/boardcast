// 会议摘要：按会议记录加板书结构，让模型写出结论、决定、待办（带负责人）、待讨论。
// 会开得长，一次塞不下：先把记录切成几段各自记要点，再连同板书合成一份。
// 一场会有几张板时，每张板都带上（「第 N 张板」分节）；导入页上只带被圈画、批注过的原文。

import { type Graph, cloneGraph, describe } from './graph.ts'
import { type ChatMessage, type LlmConfig, type LlmResult, chat } from './llm.ts'
import { msg } from '../i18n/index.ts'
import { type SpokenLang, spokenLang } from './lang.ts'
import type { Utterance } from './prompt.ts'

/**
 * Model-facing wording of the summary, per meeting language. An English meeting gets English
 * instructions and English section headings, so the summary comes out in English.
 */
const WORDS = {
  zh: {
    sections: ['结论', '决定', '待办', '待讨论'],
    none: '无',
    board: (n: number) => `第 ${n} 张板`,
    pageMark: (n: number) => `【第 ${n} 张板】`,
    colon: '：',
    noLines: '（没有记录）',
    transcript: '会议记录：',
    fragment: '会议记录（片段）：',
    boardIntro: '白板上的内容：',
    part: (n: number) => `第 ${n} 段：`,
    tooLong: '会议记录太长，下面是逐段记下的要点：',
    partial: '你在帮人整理会议记录。这是一场长会议中的一段，记下这一段里的要点：说了什么结论、定了什么、谁要做什么、还有什么没定。每条一行，以「- 」开头。',
    system: '你在帮人写会议摘要。',
    rules: `只根据给出的内容写，不要编造；没有的部分写「无」。
用简体中文，每条一行，以「- 」开头，简短。
待办写成「- [ ] 事情（负责人：某某）」，没提到负责人就不写括号。
白板有好几张时，结论、待办后面可以注明在哪张板上讨论的，比如「（第 2 张板）」。
严格按下面的格式输出，不要别的话：`,
  },
  en: {
    sections: ['Conclusions', 'Decisions', 'To-dos', 'Open questions'],
    none: 'None',
    board: (n: number) => `Board ${n}`,
    pageMark: (n: number) => `[Board ${n}]`,
    colon: ': ',
    noLines: '(no transcript)',
    transcript: 'Transcript:',
    fragment: 'Transcript (excerpt):',
    boardIntro: 'On the whiteboard:',
    part: (n: number) => `Part ${n}:`,
    tooLong: 'The transcript is too long; these are the notes taken part by part:',
    partial:
      "You are helping someone tidy up meeting notes. This is one part of a long meeting; note its key points: conclusions reached, what was decided, who will do what, and what is still open. One point per line, starting with '- '.",
    system: 'You are helping someone write a meeting summary.',
    rules: `Write only from what is given, never make things up; write "None" for an empty section.
Write in English, one short point per line, starting with "- ".
Write to-dos as "- [ ] task (owner: name)"; leave out the parentheses if no owner was mentioned.
With several boards, conclusions and to-dos may note which board they were discussed on, e.g. "(Board 2)".
Output exactly the format below and nothing else:`,
  },
} as const


/** 一段记录最多这么多字；本地小模型的上下文有限，再长就分段 */
const CHUNK = 3000

const rules = (lang: SpokenLang) => `${WORDS[lang].rules}\n${WORDS[lang].sections.map((s) => `### ${s}`).join('\n')}`

/** 会议记录写成文字；说话时在第几张板变了的地方插一行「【第 N 张板】」 */
export function transcript(lines: (Utterance & { page?: number })[], lang: SpokenLang = 'zh'): string {
  const w = WORDS[lang]
  const many = new Set(lines.map((l) => l.page ?? 0)).size > 1
  const out: string[] = []
  let page: number | undefined
  for (const l of lines) {
    if (many && (l.page ?? 0) !== page) {
      page = l.page ?? 0
      out.push(w.pageMark(page + 1))
    }
    out.push(l.speaker ? `${l.speaker}${w.colon}${l.text}` : l.text)
  }
  return out.join('\n')
}

/** 按字数把记录切成几段，不把一句话切开 */
export function chunks<T extends Utterance>(lines: T[], size = CHUNK): T[][] {
  const out: T[][] = []
  let cur: T[] = []
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

/** 一张板给摘要看的样子：导入页上没人动过的原文不带（太多了，也没讨论到） */
function boardText(g: Graph): string {
  const touched = new Set(g.nodes.filter((n) => n.parent).map((n) => n.parent!))
  for (const e of g.edges) touched.add(e.from).add(e.to)
  const out = cloneGraph(g)
  out.nodes = out.nodes.filter((n) => n.kind !== 'region' || n.marks.length || n.spans?.length || touched.has(n.id))
  return describe(out)
}

/** 所有板的内容；只有一张板时不分节。每张板太长就截断，免得挤掉会议记录 */
export function boardsText(boards: Graph[], max = 1500, lang: SpokenLang = 'zh'): string {
  const live = boards.filter((g) => g.nodes.length)
  const cut = (s: string) => (s.length > max ? `${s.slice(0, max)}\n……` : s)
  if (live.length <= 1) return cut(boardText(live[0] ?? boards[0]))
  return boards
    .map((g, i) => (g.nodes.length ? `${WORDS[lang].board(i + 1)}${WORDS[lang].colon.trim()}\n${cut(boardText(g))}` : ''))
    .filter(Boolean)
    .join('\n\n')
}

export function summaryMessages(g: Graph | Graph[], said: string, partial = false, lang: SpokenLang = 'zh'): ChatMessage[] {
  const w = WORDS[lang]
  const system = partial ? w.partial : `${w.system}\n${rules(lang)}`
  const board = boardsText(Array.isArray(g) ? g : [g], undefined, lang)
  const user = partial ? `${w.fragment}\n${said}` : `${w.boardIntro}\n${board}\n\n${said}`
  return [
    { role: 'system', content: system },
    { role: 'user', content: user },
  ]
}

/** 「## 结论」「**结论**」「【待办事项】」「结论：」这些写法都认成小节标题 */
const HEADING = /^\s*(?:#{1,6}\s*)?(?:\*\*|【)?\s*(结论|决定|待办|待办事项|待讨论)\s*(?:\*\*|】)?\s*[:：]?\s*(?:\*\*)?\s*$/
/** The same for English: "## Conclusions", "**Action items:**", "Next steps" … mapped to the canonical name */
const HEADING_EN =
  /^\s*(?:#{1,6}\s*)?(?:\*\*)?\s*(conclusions?|decisions?( made)?|to-?dos?|action items?|next steps|open questions?|open issues|to discuss)\s*(?:\*\*)?\s*:?\s*(?:\*\*)?\s*$/i
const canonicalEn = (h: string): string => {
  const k = h.toLowerCase()
  if (k.startsWith('conclusion')) return 'Conclusions'
  if (k.startsWith('decision')) return 'Decisions'
  if (k.startsWith('open') || k === 'to discuss') return 'Open questions'
  return 'To-dos'
}

/**
 * 模型的输出收拾一下：去掉代码块围栏和格式前面的废话，小节标题统一成「### 结论」，缺的小节补上「无」。
 * 一个小节都认不出来时原样返回（总比四个「无」强）；空的就是空的。
 */
export function tidySummary(text: string, lang: SpokenLang = 'zh'): string {
  const lines = text.replace(/```[a-z]*\n?/gi, '').trim().split('\n')
  let found = 0
  const out = lines.map((l) => {
    if (lang === 'en') {
      const m = HEADING_EN.exec(l)
      if (!m) return l
      found++
      return `### ${canonicalEn(m[1])}`
    }
    const m = HEADING.exec(l)
    if (!m) return l
    found++
    return `### ${m[1] === '待办事项' ? '待办' : m[1]}`
  })
  let t = out.join('\n').trim()
  if (!found) return t
  const first = t.indexOf('### ')
  if (first > 0) t = t.slice(first)
  for (const s of WORDS[lang].sections) if (!new RegExp(`^### ${s}$`, 'm').test(t)) t += `\n\n### ${s}\n- ${WORDS[lang].none}`
  return t.trim()
}

/** 发一次；推理模型可能把 token 全花在思考上、正文是空的，那就给多一倍再试一次 */
async function ask(cfg: LlmConfig, msgs: ChatMessage[], tokens: number, signal?: AbortSignal): Promise<LlmResult> {
  let r = await chat(cfg, msgs, tokens, signal)
  if (!r.text.trim() && !signal?.aborted) r = await chat(cfg, msgs, Math.min(tokens * 2, 8192), signal)
  if (!r.text.trim()) {
    const e = msg().errors
    const why = r.finish === 'length' || r.reasoning ? e.allThinking : e.emptyReply
    throw new Error(`${why}${r.via ? msg().doc.aside(r.via) : ''}`)
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
  /** 当前这张板，或者一场会的所有板 */
  g: Graph | Graph[],
  lines: (Utterance & { page?: number })[],
  signal?: AbortSignal,
  progress?: (done: number, total: number) => void,
): Promise<SummaryResult> {
  const t0 = performance.now()
  // 远程的可能是推理模型，先想再写，给足余量
  const remote = cfg.flavor === 'openai'
  const lang = spokenLang(lines.map((l) => l.text))
  const w = WORDS[lang]
  const parts = chunks(lines)
  let said = `${w.transcript}\n${transcript(lines, lang) || w.noLines}`
  let last: LlmResult | undefined
  let calls = 0
  if (parts.length > 1) {
    const notes: string[] = []
    for (const [i, p] of parts.entries()) {
      progress?.(i, parts.length + 1)
      last = await ask(cfg, summaryMessages(g, transcript(p, lang), true, lang), remote ? 2048 : 600, signal)
      calls++
      notes.push(`${w.part(i + 1)}\n${last.text.trim()}`)
    }
    said = `${w.tooLong}\n${notes.join('\n\n')}`
    progress?.(parts.length, parts.length + 1)
  }
  const r = await ask(cfg, summaryMessages(g, said, false, lang), remote ? 4096 : 1024, signal)
  calls++
  const text = tidySummary(r.text, lang)
  if (!text) throw new Error(msg().errors.noSummary)
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
