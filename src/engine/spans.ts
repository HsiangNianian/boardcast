// 在一块原文里定位一个词、一句话：文字层记着每一行的碎片和位置，算出要圈的地方。
// 碎片里每个字的横坐标没有记，按字符宽度估算，圈一个词够准了。
// 比较时不看大小写和空白；行尾断词的连字符不算（「automa-」「tion」能匹配 automation）。

import { type Box, type Graph, type Mark, type Node, type RegionRow, type Span, cloneGraph } from './graph.ts'

export type SpanUnit = 'word' | 'sentence'

export function rotatePoint([x, y]: [number, number], angle = 0): [number, number] {
  if (!Number.isFinite(angle) || !angle) return [x, y]
  const cos = Math.cos(angle), sin = Math.sin(angle)
  return [x * cos - y * sin, x * sin + y * cos]
}

interface Ch {
  /** 原来的字符（行与行之间、碎片之间补的空格也算一个） */
  c: string
  row: number
  x0: number
  x1: number
  /** 比较时跳过：空白、行尾断词的连字符 */
  skip: boolean
}

const weight = (c: string) => (/[iljtfI.,;:'!|()[\]]/.test(c) ? 0.6 : /[mwMW]/.test(c) ? 1.4 : /[^\x00-\x7f]/.test(c) ? 1.9 : 1)
const cjk = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u
const closing = /[)\]}'"”’」』》〉】〕）»]/u

function chars(rows: RegionRow[]): Ch[] {
  const out: Ch[] = []
  rows.forEach((row, ri) => {
    let lastEnd: number | undefined
    for (const seg of row.segs) {
      // 碎片之间有缝就是词的边界
      if (lastEnd !== undefined && seg.x - lastEnd > row.h * 0.12) out.push({ c: ' ', row: ri, x0: lastEnd, x1: seg.x, skip: true })
      const total = [...seg.t].reduce((s, c) => s + weight(c), 0) || 1
      let x = seg.x
      for (const c of seg.t) {
        const w = (weight(c) / total) * seg.w
        out.push({ c, row: ri, x0: x, x1: x + w, skip: /\s/.test(c) })
        x += w
      }
      lastEnd = seg.x + seg.w
    }
    // 中文换行不插空格；英文行尾连字符连着下一行，其余换行保留词间空格。
    const last = out.at(-1)
    const next = [...(rows[ri + 1]?.segs[0]?.t ?? '')][0]
    if (last?.c === '-' && next && /[a-z]/.test(next)) last.skip = true
    else if (next && !(last && (cjk.test(last.c) || cjk.test(next) || /[。！？「」『』]/.test((last.c + next).normalize('NFKC'))))) out.push({ c: ' ', row: ri, x0: lastEnd ?? 0, x1: lastEnd ?? 0, skip: true })
  })
  return out
}

/** 句子在这里结束吗：句号后面是大写字母、汉字或者结尾；缩写（et al.、Fig.、e.g.、单个大写字母）不算 */
function endsSentence(cs: Ch[], i: number): boolean {
  let punctuation = i
  while (punctuation > 0 && (cs[punctuation].skip || closing.test(cs[punctuation].c.normalize('NFKC')))) punctuation--
  // 终止符和紧随其后的右引号/括号属于同一句，不能留给下一句。
  let after = i + 1
  while (cs[after]?.skip) after++
  const next = (cs[after]?.c ?? '').normalize('NFKC')
  if (closing.test(next) || /[。！？?!]/.test(next)) return false
  const c = cs[punctuation].c.normalize('NFKC')
  if (/[。！？]/.test(c)) return true
  if (!/[.?!]/.test(c)) return false
  let j = i + 1
  while (j < cs.length && (cs[j].skip || /[)"”’\]\d,]/.test(cs[j].c))) j++
  // 引用上标、括号之后才是下一句的开头
  if (j >= cs.length) return true
  if (!/[A-Z\p{Script=Han}]/u.test(cs[j].c)) return false
  if (c !== '.') return true
  let k = punctuation - 1
  let word = ''
  while (k >= 0 && !cs[k].skip && /[\p{L}.]/u.test(cs[k].c)) word = cs[k--].c + word
  return !/^(et|al|e\.g|i\.e|cf|vs|etc|Fig|Figs|Eq|Eqs|Ref|Refs|Dr|No|[A-Z])$/.test(word)
}

function sentenceAround(cs: Ch[], a: number, b: number): [number, number] {
  let start = a
  while (start > 0 && !endsSentence(cs, start - 1)) start--
  while (start < cs.length && cs[start].skip) start++
  let end = b
  while (end < cs.length && !endsSentence(cs, end - 1)) end++
  return [start, end]
}

function wordAround(cs: Ch[], i: number): [number, number] {
  const onCjk = cjk.test(cs[i].c)
  const word = (c: Ch) => !/\s/.test(c.c) && /[\p{L}\p{N}_-]/u.test(c.c) && (onCjk || !cjk.test(c.c))
  // 行尾断词的连字符连着下一行，算同一个词
  const inside = (c: Ch) => word(c) || (c.c === '-' && c.skip)
  if (!inside(cs[i])) return [i, i + 1]
  let a = i
  let b = i + 1
  while (a > 0 && inside(cs[a - 1])) a--
  while (b < cs.length && inside(cs[b])) b++
  const text = cs.slice(a, b).map((c) => c.c).join('')
  if (cjk.test(text)) {
    const offset = cs.slice(a, i).reduce((n, c) => n + c.c.length, 0)
    const language = /[\p{Script=Hiragana}\p{Script=Katakana}]/u.test(text) ? 'ja' : 'zh'
    const segment = new Intl.Segmenter(language, { granularity: 'word' }).segment(text).containing(offset)!
    a += [...text.slice(0, segment.index)].length
    b = a + [...segment.segment].length
  }
  return [a, b]
}

function toSpan(cs: Ch[], a: number, b: number, mark: Mark, rows: RegionRow[]): Span {
  const boxes = new Map<number, Box>()
  for (let i = a; i < b; i++) {
    const c = cs[i]
    if (/\s/.test(c.c)) continue
    const row = rows[c.row]
    const box = boxes.get(c.row)
    if (box) box.w = Math.max(box.x + box.w, c.x1) - box.x
    else boxes.set(c.row, { x: c.x0, y: row.y, w: c.x1 - c.x0, h: row.h, ...(row.angle ? { angle: row.angle } : {}) })
  }
  const text = cs
    .slice(a, b)
    .filter((c) => !(c.c === '-' && c.skip))
    .map((c) => c.c)
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
  return { mark, text, boxes: [...boxes.values()].map((x) => ({ ...x, x: Math.round(x.x), y: Math.round(x.y), w: Math.round(x.w), h: Math.round(x.h) })) }
}

/** 按原文里的字找：`unit` 是 sentence 时扩到它所在的整句 */
export function locate(rows: RegionRow[], quote: string, mark: Mark, unit: SpanUnit = 'word'): Span | null {
  const cs = chars(rows)
  const index: number[] = []
  let hay = ''
  cs.forEach((c, i) => {
    if (c.skip) return
    const lower = c.c.normalize('NFKD').toLowerCase()
    hay += lower
    // 字符表按码点记录，字符串匹配返回 UTF-16 索引；大小写转换也可能增加长度。
    for (let j = 0; j < lower.length; j++) index.push(i)
  })
  const q = quote.normalize('NFKD').toLowerCase().replace(/\s+/g, '')
  const at = q ? hay.indexOf(q) : -1
  if (at < 0) return null
  let a = index[at]
  let b = index[at + q.length - 1] + 1
  if (unit === 'sentence') [a, b] = sentenceAround(cs, a, b)
  return toSpan(cs, a, b, mark, rows)
}

/** 按点到的位置找：手动圈画 */
export function spanAt(rows: RegionRow[], x: number, y: number, mark: Mark, unit: SpanUnit): Span | null {
  const cs = chars(rows)
  const points = rows.map((r) => rotatePoint([x, y], -(r.angle ?? 0)))
  const distances = rows.map((r, i) => {
    const dx = Math.max(0, Math.min(...r.segs.map((s) => s.x)) - points[i][0], points[i][0] - Math.max(...r.segs.map((s) => s.x + s.w)))
    return dx * dx + (r.y + r.h / 2 - points[i][1]) ** 2
  })
  const row = distances.reduce((best, d, i) => d < distances[best] ? i : best, 0)
  const localX = points[row]?.[0] ?? x
  let hit = -1
  let dist = Infinity
  cs.forEach((c, i) => {
    if (c.row !== row || /\s/.test(c.c)) return
    const d = localX < c.x0 ? c.x0 - localX : localX > c.x1 ? localX - c.x1 : 0
    if (d < dist) {
      dist = d
      hit = i
    }
  })
  if (hit < 0) return null
  const [a, b] = unit === 'word' ? wordAround(cs, hit) : sentenceAround(cs, hit, hit + 1)
  return toSpan(cs, a, b, mark, rows)
}

const same = (a: Span, b: Span) => a.mark === b.mark && JSON.stringify(a.boxes) === JSON.stringify(b.boxes)

export function addSpan(n: Node, s: Span): void {
  if (!n.spans?.some((x) => same(x, s))) n.spans = [...(n.spans ?? []), s]
}

/** 手动：点到的词或句已经有这个标记就去掉，没有就加上 */
export function toggleSpan(g: Graph, id: string, s: Span): Graph {
  const out = cloneGraph(g)
  const n = out.nodes.find((x) => x.id === id)
  if (!n) return g
  if (n.spans?.some((x) => same(x, s))) {
    const left = n.spans.filter((x) => !same(x, s))
    if (left.length) n.spans = left
    else delete n.spans
  } else addSpan(n, s)
  return out
}

export function hasSpan(n: Node | undefined, s: Span | null): boolean {
  return !!s && !!n?.spans?.some((x) => same(x, s))
}
