// 整场会导出成多页 PDF：每张板一页（带导入页的底图），后面是摘要和会议记录。
// 都画在画布上（中文用系统字体，不用往 PDF 里塞字体），转成 JPEG，交给 pdf.ts 装订。
// 代价：PDF 里的字不能选中复制；要可复制的文字，用 Markdown 导出。

import { msg } from '../i18n/index.ts'
import { compose } from '../ink/compose.ts'
import { WHITEBOARD } from '../ink/InkBoard.tsx'
import type { ReplayData, ReplayPage } from '../player/data.ts'
import { StableLayout } from './layout.ts'
import { A4_LANDSCAPE, type PdfPage, makePdf } from './pdf.ts'

/** 画布尺寸：A4 横向的两倍多，打印也够清楚 */
const CW = 2400
const CH = Math.round((CW * A4_LANDSCAPE.h) / A4_LANDSCAPE.w)
const M = 110
const FONT = '"PingFang SC", "Noto Sans CJK SC", "Source Han Sans SC", "Microsoft YaHei", sans-serif'
const MUTED = '#6b6b6b'

function canvas(): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const c = document.createElement('canvas')
  c.width = CW
  c.height = CH
  const x = c.getContext('2d')!
  x.fillStyle = WHITEBOARD.paper
  x.fillRect(0, 0, CW, CH)
  return [c, x]
}

async function toPage(c: HTMLCanvasElement): Promise<PdfPage> {
  const blob = await new Promise<Blob>((ok, fail) => c.toBlob((b) => (b ? ok(b) : fail(new Error('生成图片失败'))), 'image/jpeg', 0.88))
  return { jpeg: new Uint8Array(await blob.arrayBuffer()), width: c.width, height: c.height }
}

/** 页眉：会议名，右边是「第 N 张板」 */
function header(x: CanvasRenderingContext2D, title: string, right: string) {
  x.fillStyle = MUTED
  x.font = `28px ${FONT}`
  x.textBaseline = 'top'
  x.fillText(title, M, 50, CW - M * 2 - 400)
  const w = x.measureText(right).width
  x.fillText(right, CW - M - w, 50)
}

/** 一张板：用同一套排版和手写笔画画成 SVG，再画到画布上 */
async function boardPage(page: ReplayPage, title: string, label: string): Promise<PdfPage> {
  const layout = new StableLayout()
  layout.restore(page.layout)
  const l = await layout.layout(page.graph)
  const pieces = await compose(page.graph, l, new Set(), WHITEBOARD, 0.3, 1, page.base)
  const P = 40
  const x0 = l.bounds.x0 - P
  const y0 = l.bounds.y0 - P
  const w = l.bounds.x1 - l.bounds.x0 + P * 2
  const h = l.bounds.y1 - l.bounds.y0 + P * 2
  const paths = (under: boolean) =>
    pieces
      .filter((p) => !!p.under === under)
      .map((p) => `<g fill="${p.color}" opacity="${p.opacity}"${under ? ' style="mix-blend-mode:multiply"' : ''}>${p.strokes.map((s) => `<path d="${s.d}"/>`).join('')}</g>`)
      .join('')
  const u = page.graph.underlay
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x0} ${y0} ${w} ${h}" width="${w}" height="${h}">${
    page.underlay && u ? `<image href="${page.underlay}" x="0" y="0" width="${u.w}" height="${u.h}" preserveAspectRatio="none"/>` : ''
  }${paths(true)}${paths(false)}</svg>`
  const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }))
  const [c, x] = canvas()
  try {
    const img = new Image()
    img.src = url
    await img.decode()
    header(x, title, label)
    // 板书按比例放进页眉以下的区域，居中
    const top = 110
    const k = Math.min((CW - M) / w, (CH - top - 50) / h)
    const dw = w * k
    const dh = h * k
    x.drawImage(img, (CW - dw) / 2, top + (CH - top - 50 - dh) / 2, dw, dh)
  } finally {
    URL.revokeObjectURL(url)
  }
  return toPage(c)
}

interface TextLine {
  text: string
  size: number
  bold?: boolean
  color?: string
  /** 前面空多少 */
  gap?: number
  indent?: number
}

/** 摘要和会议记录排成一行行：Markdown 只认小节标题和条目 */
function textLines(data: ReplayData): TextLine[] {
  const t = msg().doc
  const out: TextLine[] = [{ text: data.title, size: 52, bold: true }]
  const d = new Date(data.createdAt)
  const p2 = (n: number) => String(n).padStart(2, '0')
  out.push({ text: `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())} ${p2(d.getHours())}:${p2(d.getMinutes())} · boardcast`, size: 26, color: MUTED, gap: 8 })
  if (data.summary?.trim()) {
    out.push({ text: t.summary, size: 38, bold: true, gap: 50 })
    for (const raw of data.summary.split('\n')) {
      const l = raw.trimEnd()
      if (!l.trim()) continue
      if (/^#{1,6}\s/.test(l)) out.push({ text: l.replace(/^#+\s*/, ''), size: 30, bold: true, color: MUTED, gap: 22 })
      else {
        const todo = /^\s*[-*]\s+\[( |x|X)\]\s*(.*)$/.exec(l)
        const item = /^\s*[-*]\s+(.*)$/.exec(l)
        out.push({ text: todo ? `${todo[1] === ' ' ? '☐' : '☑'} ${todo[2]}` : item ? `• ${item[1]}` : l, size: 28, indent: 20 })
      }
    }
  }
  if (data.lines.length) {
    out.push({ text: t.transcript, size: 38, bold: true, gap: 50 })
    const many = data.pages.length > 1
    let page: number | undefined
    for (const l of data.lines) {
      if (many && (l.page ?? 0) !== page) {
        page = l.page ?? 0
        out.push({ text: t.board(page + 1), size: 26, color: MUTED, gap: 18 })
      }
      if (l.manual) out.push({ text: `${t.edited}${l.text}`, size: 26, color: MUTED, indent: 20 })
      else out.push({ text: l.speaker ? `${l.speaker}${t.colon}${l.text}` : l.text, size: 28, indent: 20 })
    }
  }
  return out
}

/** 按宽度折行：一个字一个字量 */
function wrap(x: CanvasRenderingContext2D, text: string, max: number): string[] {
  const out: string[] = []
  let cur = ''
  for (const ch of text) {
    if (cur && x.measureText(cur + ch).width > max) {
      out.push(cur)
      cur = ch === ' ' ? '' : ch
    } else cur += ch
  }
  if (cur) out.push(cur)
  return out.length ? out : ['']
}

async function textPages(data: ReplayData): Promise<PdfPage[]> {
  const pages: PdfPage[] = []
  let [c, x] = canvas()
  let y = M
  let n = 1
  const start = () => {
    ;[c, x] = canvas()
    header(x, data.title, msg().doc.transcriptN(++n))
    y = 130
  }
  for (const t of textLines(data)) {
    x.font = `${t.bold ? '600 ' : ''}${t.size}px ${FONT}`
    const indent = t.indent ?? 0
    const rows = wrap(x, t.text, CW - M * 2 - indent)
    y += t.gap ?? 0
    for (const row of rows) {
      const lh = t.size * 1.55
      if (y + lh > CH - M) {
        pages.push(await toPage(c))
        start()
        x.font = `${t.bold ? '600 ' : ''}${t.size}px ${FONT}`
      }
      x.fillStyle = t.color ?? WHITEBOARD.ink
      x.textBaseline = 'top'
      x.fillText(row, M + indent, y)
      y += lh
    }
  }
  pages.push(await toPage(c))
  return pages
}

export async function meetingPdf(data: ReplayData, onPage?: (done: number, total: number) => void): Promise<Uint8Array> {
  const pages: PdfPage[] = []
  const boards = data.pages.filter((p) => p.graph.nodes.length || p.underlay)
  for (const [i, p] of boards.entries()) {
    onPage?.(i, boards.length)
    pages.push(await boardPage(p, data.title, boards.length > 1 ? msg().doc.board(i + 1) : msg().doc.boardSingle))
  }
  if (data.summary?.trim() || data.lines.length) pages.push(...(await textPages(data)))
  return makePdf(pages, A4_LANDSCAPE, data.title)
}
