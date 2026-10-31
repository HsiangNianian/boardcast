// PDF 一页的版面分析：把文字层的碎片切成一块块「原文」，并按论文里的叫法起名。
// 普通文档也走这里，只是用不上图注、公式编号这些规则。
//
// 1. 去掉旋转的字（arXiv 侧边的竖排标记）和页码；
// 2. 找栏缝：页面中间一条没有文字穿过的竖带，左右两栏各拼各的，跨栏的（标题、通栏大图）单独算；
// 3. 拼行：同一栏里、高度差不多、横向挨着的碎片拼成一行；
// 4. 给行分类：章节标题（3. / 3.1 / IV. / 一、 / Abstract / References）、图注表注（Figure 3. / Table 2 / 图 4）、
//    公式编号（行尾的 (5)）、参考文献条目（[12] 开头）、正文；
// 5. 拼段落：同一栏上下挨着的正文行拼在一起，遇到首行缩进、上一行没写满、标题、图注就另起一段；
// 6. 图表：从图注往上（表注往下）找到图表本身占的那块，里面的零碎文字（坐标轴、图例）并进来，整块叫「图 3：……」；
// 7. 公式和它的编号合成一块，叫「公式 (5)」。
// 纯函数，坐标是板书坐标（页面左上角为原点，y 向下）。

import type { Area } from '../engine/regions.ts'

export interface TextPiece {
  text: string
  x: number
  y: number
  w: number
  h: number
  /** 字是斜的或竖排的 */
  rotated?: boolean
}

export type BlockKind = 'heading' | 'figure' | 'table' | 'equation' | 'reference' | 'text'

export interface PaperArea extends Area {
  kind: BlockKind
  /** 起好的名字（图 3：……、公式 (5)、§3 方法）；正文没有，用开头几个字 */
  name?: string
}

export interface PageAnalysis {
  areas: PaperArea[]
  /** 这一页的章节标题（按出现顺序），给「论文结构」用 */
  headings: string[]
  /** 两栏排版 */
  columns: 1 | 2
}

/** 跨页的状态：是不是已经进了参考文献；这篇文档的图表、图注谁上谁下（一篇论文是统一的，按看得清的那些投票） */
export interface PaperState {
  inRefs: boolean
  /** 正数：图在图注上面的多；负数：图在图注下面的多 */
  figureUp?: number
  /** 正数：表在表注上面的多（表注在下）；负数：表在表注下面的多 */
  tableUp?: number
  /** 已经出现过的图注、表注（「figure 3」）：同一个编号只认第一次，后面的是正文里引用它 */
  captions?: Set<string>
}

type Side = 'left' | 'right' | 'span'

interface Line {
  text: string
  x: number
  y: number
  w: number
  h: number
  side: Side
  kind: BlockKind
  /** 图表、公式的编号 */
  num?: string
}

const latin = (ch: string | undefined) => !!ch && /[A-Za-z0-9,.;:)]/.test(ch)
const join = (a: string, b: string) => (latin(a.at(-1)) && /[A-Za-z0-9(]/.test(b[0] ?? '') ? `${a} ${b}` : a + b)

// 图注、表注：编号后面紧跟「.」「:」，再是大写字母或汉字。正文里引用图表（「Fig. 3).」「Table 3 shows」）不算
const CAPTION = /^(Figure|FIGURE|Fig\.?|图|Table|TABLE|Tab\.?|表)\s*([0-9]+|[IVX]+)\s*[.:：．]\s*(?=[A-Z\p{Script=Han}])/u
// 编号后面要跟大写字母或者汉字（「3 x3 conv」这种图里的标注不算）
const NUMBERED = /^((?:\d+\.)*\d+\.?|[IVX]+\.|[A-Z]\.|[一二三四五六七八九十]+、|第[一二三四五六七八九十\d]+[章节])\s*(?=[A-Z][A-Za-z]|\p{Script=Han})/u
const NAMED = /^(Abstract|Introduction|Related Work|Conclusions?|Acknowledge?ments?|References|Bibliography|Appendix|摘要|引言|结论|致谢|参考文献|附录)\b/i
const REFS = /^(References|Bibliography|参考文献)\s*$/i
const EQNUM = /\((\d+[a-z]?)\)\s*$/

function median(xs: number[]): number {
  if (!xs.length) return 0
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.floor(s.length / 2)]
}

/** 栏缝：页面 35%–65% 之间一条几乎没有碎片穿过的竖带，而且两边都有不少字。
 * 允许少数几个碎片穿过：论文第一页的标题、作者名、单位都是居中排的，正好横跨栏缝 */
function gutter(ps: TextPiece[], W: number): number | null {
  const narrow = ps.filter((p) => p.w < W * 0.55)
  if (narrow.length < 20) return null
  const allowed = Math.max(3, Math.round(narrow.length * 0.03))
  let best: { a: number; b: number } | null = null
  let start = -1
  for (let x = Math.round(W * 0.35); x <= Math.round(W * 0.65); x++) {
    const hit = narrow.filter((p) => p.x < x && p.x + p.w > x).length > allowed
    if (!hit && start < 0) start = x
    if ((hit || x === Math.round(W * 0.65)) && start >= 0) {
      if (!best || x - start > best.b - best.a) best = { a: start, b: x }
      start = -1
    }
  }
  if (!best || best.b - best.a < 6) return null
  const g = (best.a + best.b) / 2
  const left = narrow.filter((p) => p.x + p.w <= g).length
  const right = narrow.filter((p) => p.x >= g).length
  return left > narrow.length * 0.2 && right > narrow.length * 0.2 ? g : null
}

export function analyzePage(pieces: TextPiece[], W: number, H: number, state: PaperState = { inRefs: false }): PageAnalysis {
  // —— 1. 去掉旋转的字、页码 ——
  const ps = pieces.filter((p) => {
    if (p.rotated || !p.text.trim() || p.w <= 0 || p.h <= 0) return false
    const pageNo = /^\s*\d{1,4}\s*$/.test(p.text) && (p.y > H * 0.9 || p.y + p.h < H * 0.07)
    return !pageNo
  })
  const g = gutter(ps, W)
  const sideOf = (x: number, w: number): Side => (g === null ? 'left' : x + w <= g + 2 ? 'left' : x >= g - 2 ? 'right' : 'span')
  const body = median(ps.map((p) => p.h))

  // —— 2. 拼行（不跨栏缝） ——
  const lines: Line[] = []
  for (const p of [...ps].sort((a, b) => a.y - b.y || a.x - b.x)) {
    const side = sideOf(p.x, p.w)
    const hit = lines.find(
      (l) =>
        l.side === side &&
        Math.abs(l.y + l.h / 2 - (p.y + p.h / 2)) < Math.min(l.h, p.h) * 0.5 &&
        p.x >= l.x - 1 &&
        p.x - (l.x + l.w) < Math.max(l.h, p.h) * 1.5,
    )
    if (hit) {
      hit.text = join(hit.text, p.text.trim())
      const x1 = Math.max(hit.x + hit.w, p.x + p.w)
      const y0 = Math.min(hit.y, p.y)
      hit.h = Math.max(hit.y + hit.h, p.y + p.h) - y0
      hit.y = y0
      hit.w = x1 - hit.x
      hit.side = sideOf(hit.x, hit.w)
    } else lines.push({ text: p.text.trim(), x: p.x, y: p.y, w: p.w, h: p.h, side, kind: 'text' })
  }

  // —— 3. 行分类 ——
  const colWidth = (s: Side) => (g === null || s === 'span' ? W * 0.8 : W * 0.4)
  for (const l of lines) {
    const cap = CAPTION.exec(l.text)
    if (cap) {
      const kind = /^(Table|TABLE|Tab|表)/.test(cap[1]) ? 'table' : 'figure'
      state.captions ??= new Set()
      const key = `${kind} ${cap[2]}`
      if (!state.captions.has(key)) {
        state.captions.add(key)
        l.kind = kind
        l.num = cap[2]
        continue
      }
    }
    if (REFS.test(l.text)) {
      l.kind = 'heading'
      state.inRefs = true
      continue
    }
    // 标题一般不长、不以句号结尾；编号带点的（「4.3. 」「4. 」）可以和栏一样宽，长标题常见
    const wideOk = l.w < colWidth(l.side) * 0.9 || (/^\d+(\.\d+)*\.\s/.test(l.text) && l.w <= colWidth(l.side) * 1.02)
    const short = l.text.length <= 80 && wideOk && !/[.。]$/.test(l.text.replace(/^[\d.]+\s*/, '')) && !/:\/\//.test(l.text)
    // 带编号的标题要从栏的左边起（行中间冒出来的「4 For each…」是脚注号）
    const atLeft = l.x - Math.min(...lines.filter((o) => o.side === l.side).map((o) => o.x)) < body * 2
    // 像句子的不是标题：没带点的大编号（「10 The responses are…」是正文里冒出来的数字），或者有 is / are / we 之类
    const lead = /^(\d+)\s/.exec(l.text)
    const sentence = (lead && Number(lead[1]) > 20) || /\b(is|are|was|were|we|our|this|that)\b/i.test(l.text.replace(NUMBERED, ''))
    if (short && l.h >= body * 0.9 && ((NUMBERED.test(l.text) && atLeft && !sentence) || NAMED.test(l.text)) && !(state.inRefs && /^\[\d+\]/.test(l.text))) {
      // 编号后面直接跟正文的长句不算标题（「1 http://…」脚注、「2 This hypothesis…」）
      if (NAMED.test(l.text) && /^(Appendix|附录)/i.test(l.text)) state.inRefs = false
      if (!NAMED.test(l.text) && state.inRefs) state.inRefs = false
      l.kind = 'heading'
      continue
    }
    if (state.inRefs) {
      l.kind = 'reference'
      continue
    }
    const eq = EQNUM.exec(l.text)
    if (eq && (/^\(\d+[a-z]?\)$/.test(l.text.trim()) || (/[=≤≥<>∑∫]/.test(l.text) && l.text.length < 120))) {
      l.kind = 'equation'
      l.num = eq[1]
    }
  }
  // 单独一行的「(2)」并到同一高度的公式上
  for (const l of lines.filter((x) => x.kind === 'equation' && /^\(\d+[a-z]?\)$/.test(x.text.trim()))) {
    const host = lines.find((o) => o !== l && o.kind !== 'heading' && Math.abs(o.y + o.h / 2 - (l.y + l.h / 2)) < Math.max(o.h, l.h) * 0.8 && o.x + o.w <= l.x + 2 && (g === null || o.side === l.side || o.side === 'span'))
    if (host) {
      host.text = `${host.text} ${l.text.trim()}`
      host.w = l.x + l.w - host.x
      host.kind = 'equation'
      host.num = l.num
      l.text = ''
    }
  }
  const live = lines.filter((l) => l.text)

  // —— 4. 拼段落 ——
  type Block = PaperArea & { side: Side; lh: number; lines: number; num?: string; lastRight?: number }
  const blocks: Block[] = []
  const order: Side[] = ['span', 'left', 'right']
  const right = (s: Side) => Math.max(...live.filter((l) => l.side === s).map((l) => l.x + l.w))
  const sorted = [...live].sort((a, b) => (g === null ? a.y - b.y : order.indexOf(a.side) - order.indexOf(b.side) || a.y - b.y) || a.x - b.x)
  for (const l of sorted) {
    const prev = blocks.findLast((b) => b.side === l.side)
    const lastLine = prev && { right: prev.x + prev.w, bottom: prev.y + prev.h }
    const glue =
      prev &&
      // 正文行、参考文献条目的续行可以接上去；图注、表注的续行是正文行
      (l.kind === 'text' || (l.kind === 'reference' && !/^\[\d+\]/.test(l.text))) &&
      (prev.kind === 'text' || prev.kind === 'reference' || ((prev.kind === 'figure' || prev.kind === 'table') && prev.lines < 6)) &&
      l.y - lastLine!.bottom < Math.max(prev.lh, l.h) * 0.9 &&
      l.y >= lastLine!.bottom - prev.lh * 0.6 &&
      // 字号差不多；带上标（脚注号、10⁴）的行会高一点，和正文差得不多也算
      (Math.max(prev.lh, l.h) / Math.min(prev.lh, l.h) < 1.3 || (Math.abs(prev.lh - body) < body * 0.25 && l.h < body * 1.7 && l.h > body * 0.8)) &&
      // 首行缩进：新段落
      !(l.kind === 'text' && l.x > prev.x + l.h * 0.8) &&
      // 上一段最后一行没写满：段落结束了（图注、表注不管）
      !(prev.kind === 'text' && prev.lastRight !== undefined && prev.lastRight < right(l.side) - l.h * 2.5)
    if (glue) {
      prev.text = join(prev.text, l.text)
      const x0 = Math.min(prev.x, l.x)
      prev.w = Math.max(prev.x + prev.w, l.x + l.w) - x0
      prev.x = x0
      prev.h = l.y + l.h - prev.y
      prev.lines++
      prev.lastRight = l.x + l.w
    } else
      blocks.push({ text: l.text, x: l.x, y: l.y, w: l.w, h: l.h, kind: l.kind, side: l.side, lh: l.h, lines: 1, num: l.num, lastRight: l.x + l.w })
  }

  // —— 5. 图表：从图注往上或往下找图表本身占的地方，里面的零碎文字（表格的格子、坐标轴、图例）并进来 ——
  // 真正的正文段落：够长、够宽、正文字号。图里的标注再宽也不算，它们挡不住找图的范围
  // 每行平均写了多少（汉字算两个）：正文一行几十个字母，表格格子、图例拼起来的「段落」一行只有二三十
  const perLine = (b: Block) => (b.text.length + (b.text.match(/\p{Script=Han}/gu)?.length ?? 0)) / b.lines
  // 大半是字母、汉字（表格里一行行的数字不算正文）
  const prose = (b: Block) => {
    const chars = b.text.replace(/\s/g, '')
    return chars.length > 0 && (chars.match(/[\p{L}]/gu)?.length ?? 0) / chars.length >= 0.6
  }
  const solid = (b: Block) =>
    (b.kind === 'text' && b.text.length >= 60 && b.w > colWidth(b.side) * 0.5 && b.lh >= body * 0.85 && perLine(b) >= 35 && prose(b)) ||
    b.kind === 'equation' ||
    (b.kind === 'heading' && b.lh >= body * 0.95)
  const isCap = (b: Block) => b.kind === 'figure' || b.kind === 'table'
  const gone = new Set<Block>()
  const zoneOf = (cap: Block) => {
    const x0 = cap.side === 'right' && g !== null ? g : 0
    const x1 = cap.side === 'left' && g !== null ? g : W
    const overlaps = (b: Block) => b !== cap && !gone.has(b) && b.x < x1 && b.x + b.w > x0
    // 往一个方向找：碰到正文段落或者别的图注就停
    return (up: boolean) => {
      const walls = blocks.filter((b) => overlaps(b) && (solid(b) || isCap(b)) && (up ? b.y + b.h <= cap.y : b.y >= cap.y + cap.h))
      const top = up ? Math.max(0, ...walls.map((b) => b.y + b.h)) : cap.y + cap.h
      const bottom = up ? cap.y : Math.min(H, ...walls.map((b) => b.y))
      const inside = blocks.filter((b) => overlaps(b) && !solid(b) && !isCap(b) && b.y >= top - 2 && b.y + b.h <= bottom + 2)
      return { top, bottom, inside, x0, x1 }
    }
  }
  // 先投票：一边有零碎文字、另一边没有的，方向是清楚的
  const caps = blocks.filter(isCap)
  for (const cap of caps) {
    const zone = zoneOf(cap)
    const a = zone(true).inside.length
    const b = zone(false).inside.length
    const key = cap.kind === 'figure' ? 'figureUp' : 'tableUp'
    if (a > 0 && b === 0) state[key] = (state[key] ?? 0) + 1
    if (b > 0 && a === 0) state[key] = (state[key] ?? 0) - 1
  }
  for (const cap of caps) {
    const zone = zoneOf(cap)
    const above = zone(true)
    const below = zone(false)
    // 清楚的按它自己；两边都有（上一张表的格子和下一张表的格子）就按这篇文档的习惯；没有习惯可循就按常见的：图注在图下、表注在表上
    const vote = cap.kind === 'figure' ? state.figureUp : state.tableUp
    const up =
      above.inside.length && !below.inside.length ? true : below.inside.length && !above.inside.length ? false : vote ? vote > 0 : cap.kind === 'figure'
    const z = up ? above : below
    const { x0, x1 } = z
    for (const b of z.inside) gone.add(b)
    if (z.bottom - z.top > body * 2) {
      // 图表本身占的地方：没有文字层的话取整栏宽
      const xs = z.inside.length
        ? [Math.min(cap.x, ...z.inside.map((b) => b.x)), Math.max(cap.x + cap.w, ...z.inside.map((b) => b.x + b.w))]
        : [Math.min(cap.x, x0 + 8), Math.max(cap.x + cap.w, x1 - 8)]
      const y0 = up ? z.top + body * 0.5 : cap.y
      const y1 = up ? cap.y + cap.h : z.bottom - body * 0.5
      cap.x = xs[0]
      cap.w = xs[1] - xs[0]
      cap.y = y0
      cap.h = y1 - y0
    }
    // 图注后面几行（加粗的 Left: / Right: 之类断开了的）也并进来
    for (const b of blocks)
      if (b !== cap && !gone.has(b) && !isCap(b) && b.kind !== 'heading' && b.x >= cap.x - 4 && b.x + b.w <= cap.x + cap.w + 4 && b.y >= cap.y - 4 && b.y + b.h <= cap.y + cap.h + body * 3 && b.y < cap.y + cap.h && b.lh < body * 1.2) {
        gone.add(b)
        cap.h = Math.max(cap.h, b.y + b.h - cap.y)
      }
    const rest = cap.text.replace(CAPTION, '').trim()
    cap.name = `${cap.kind === 'figure' ? '图' : '表'} ${cap.num}${rest ? `：${rest}` : ''}`
  }

  // —— 6. 起名 ——
  const headings: string[] = []
  for (const b of blocks) {
    if (b.kind === 'equation') b.name = `公式 (${b.num})`
    if (b.kind === 'heading') {
      const t = b.text.replace(/\s+/g, ' ').trim()
      const m = NUMBERED.exec(t)
      b.name = m ? `§${m[1].replace(/\.$/, '')} ${t.slice(m[0].length).trim()}` : t
      headings.push(b.name)
    }
  }
  const areas = blocks
    .filter((b) => !gone.has(b))
    .map(({ text, x, y, w, h, kind, name }) => ({ text, x, y, w, h, kind, ...(name ? { name } : {}) }))
  return { areas, headings, columns: g === null ? 1 : 2 }
}
