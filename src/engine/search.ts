// 在会议里搜：会议名、会上说的话、板上的字（导入的原文也算）、摘要。
// 空格隔开的几个词都要在这场会里出现，但不要求在同一处。纯函数，桌面版和网页版用同一套。

import type { Graph } from './graph.ts'

export interface Searchable {
  id: string
  title: string
  summary?: string
  graph?: Graph
  lines?: { text: string; page?: number; manual?: boolean }[]
  pages?: { graph: Graph }[]
}

export interface Hit {
  where: 'title' | 'line' | 'board' | 'summary'
  /** 第几张板（从 0 数） */
  page?: number
  /** 会议记录里的第几条 */
  line?: number
  /** 命中处前后的一小段 */
  text: string
  /** snippet 里每个命中词的 [开始, 结束) */
  marks: [number, number][]
}

const MAX_HITS = 5
const AROUND = 24

export const terms = (q: string) => q.toLowerCase().split(/\s+/).filter(Boolean)

function snippet(s: string, ts: string[]): Pick<Hit, 'text' | 'marks'> | null {
  const flat = s.replace(/\s+/g, ' ').trim()
  const low = flat.toLowerCase()
  const first = Math.min(...ts.map((t) => low.indexOf(t)).filter((i) => i >= 0))
  if (!Number.isFinite(first)) return null
  const start = Math.max(0, first - AROUND)
  const end = Math.min(flat.length, first + AROUND * 3)
  const text = `${start ? '…' : ''}${flat.slice(start, end)}${end < flat.length ? '…' : ''}`
  const lowText = text.toLowerCase()
  const marks: [number, number][] = []
  for (const t of ts)
    for (let i = lowText.indexOf(t); i >= 0; i = lowText.indexOf(t, i + t.length)) marks.push([i, i + t.length])
  marks.sort((a, b) => a[0] - b[0])
  return { text, marks: marks.filter((m, i) => !i || m[0] >= marks[i - 1][1]) }
}

/** 这场会没有全部命中就返回 null；没搜词也返回 null */
export function searchMeeting(m: Searchable, query: string): Hit[] | null {
  const ts = terms(query)
  if (!ts.length) return null
  const all: Hit[] = []
  const seen = new Set<string>()
  const add = (s: string | undefined, hit: Pick<Hit, 'where' | 'page' | 'line'>) => {
    if (!s) return
    const low = s.toLowerCase()
    const here = ts.filter((t) => low.includes(t))
    if (!here.length) return
    for (const t of here) seen.add(t)
    const sn = snippet(s, here)
    if (sn) all.push({ ...hit, ...sn })
  }
  add(m.title, { where: 'title' })
  m.lines?.forEach((l, i) => {
    if (!l.manual) add(l.text, { where: 'line', line: i, page: l.page })
  })
  const boards = m.pages?.length ? m.pages.map((p) => p.graph) : m.graph ? [m.graph] : []
  boards.forEach((g, page) => {
    for (const n of g.nodes) add(n.text ?? n.label, { where: 'board', page })
  })
  add(m.summary, { where: 'summary' })
  if (seen.size < ts.length) return null
  return all.slice(0, MAX_HITS)
}
