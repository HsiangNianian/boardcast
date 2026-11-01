// 导入 PDF：每页一块板。页面渲染成一张图垫在底下，文字层拼成一块块「原文」钉在原位，开会时模型在上面圈画、连线、写批注。
// 扫描件没有文字层：只有底图，没有可指认的原文（以后由视觉模型来看）。
// pdf.js 按需加载；它要的字体映射（中文 PDF 离不开）、标准字体、wasm 由 vite 插件放在 /pdfjs/ 下。

import type { Page } from '../meetings.ts'
import { applyOps, parseOps } from '../engine/ops.ts'
import { pageGraph } from '../engine/regions.ts'
import { type PaperState, type TextPiece, analyzePage } from './paper.ts'
import { inkBox } from './segment.ts'
import { msg as strings } from '../i18n/index.ts'

const t = () => strings().errors

/** 页面在板上的宽度（板书坐标）；底图按两倍渲染，放大看也清楚 */
const WIDTH = 900
/** 最多建多少张板：每张一张图存在会议里，太多会让会议文件很大 */
export const MAX_PAGES = 40

export interface PdfImport {
  pages: Page[]
  /** PDF 一共几页 */
  total: number
  /** 板建够了上限，后面还有多少页没看 */
  unread: number
  /** 没有文字层的页数（扫描件、整页的图） */
  scanned: number
  /** 看起来是论文（有摘要、带编号的章节、图注） */
  paper: boolean
  /** 整页都是参考文献、没建板的页数 */
  refsSkipped: number
  /** 文档标题：PDF 元数据里的，没有就取第一页字最大的那行 */
  title?: string
}

export async function importPdf(data: ArrayBuffer | Uint8Array, onPage?: (done: number, total: number) => void): Promise<PdfImport> {
  const pdfjs = await import('pdfjs-dist')
  pdfjs.GlobalWorkerOptions.workerSrc = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default
  const base = new URL('pdfjs/', document.baseURI).href
  const task = pdfjs.getDocument({
    data,
    cMapUrl: `${base}cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${base}standard_fonts/`,
    wasmUrl: `${base}wasm/`,
  })
  let doc
  try {
    doc = await task.promise
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    throw new Error(/password/i.test(msg) ? t().pdfPassword : t().pdfBroken(msg))
  }
  const total = doc.numPages
  const meta = (await doc.getMetadata().catch(() => null))?.info as { Title?: string } | undefined
  let title = meta?.Title?.trim() || undefined
  const pages: Page[] = []
  let scanned = 0
  let refsSkipped = 0
  const state: PaperState = { inRefs: false }
  const headings: string[] = []
  let captions = 0
  let read = 0
  for (let i = 1; i <= total && pages.length < MAX_PAGES; i++) {
    onPage?.(i - 1, total)
    read = i
    const page = await doc.getPage(i)
    const scale = WIDTH / page.getViewport({ scale: 1 }).width
    const vp = page.getViewport({ scale })

    const content = await page.getTextContent()
    const pieces: TextPiece[] = []
    for (const it of content.items) {
      if (!('str' in it) || !it.str.trim()) continue
      const t = pdfjs.Util.transform(vp.transform, it.transform)
      const h = Math.hypot(t[2], t[3])
      // 斜着、竖着的字（arXiv 侧边的标记）：看转到页面方向以后的矩阵，横放的页面（/Rotate 90）里字其实是正的
      const rotated = Math.abs(t[1]) > 0.01 || Math.abs(t[2]) > 0.01
      pieces.push({ text: it.str, x: t[4], y: t[5] - h, w: it.width * scale, h, rotated })
    }
    // 没有元数据标题：第一页字最大的那一行
    const flat = pieces.filter((p) => !p.rotated)
    if (i === 1 && !title && flat.length) {
      // 竖排的 arXiv 标记不算（它的「字高」是整条竖排的长度）
      const big = Math.max(...flat.map((p) => p.h))
      const line = flat.filter((p) => p.h >= big * 0.95).sort((a, b) => a.y - b.y || a.x - b.x)
      const text = line.map((p) => p.text).join(' ').replace(/\s+/g, ' ').trim()
      if (text.length >= 4 && text.length <= 120) title = text
    }
    const wasRefs = state.inRefs
    const { areas, headings: hs } = analyzePage(pieces, vp.width, vp.height, state)
    headings.push(...hs.filter((h) => !/^(Abstract|摘要|References|Bibliography|参考文献)/i.test(h)))
    captions += areas.filter((a) => a.kind === 'figure' || a.kind === 'table').length
    // 整页都是参考文献：不建板（条目没用上，翻页反而碍事），也不用渲染
    const refsOnly = areas.length > 0 && areas.every((a) => a.kind === 'reference' || (a.kind === 'heading' && /^(References|Bibliography|参考文献)/i.test(a.text)))
    if (refsOnly && (wasRefs || state.inRefs) && pages.length) {
      refsSkipped++
      page.cleanup()
      continue
    }

    const canvas = document.createElement('canvas')
    canvas.width = Math.round(vp.width * 2)
    canvas.height = Math.round(vp.height * 2)
    const ctx = canvas.getContext('2d')!
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    await page.render({ canvas, canvasContext: ctx, viewport: page.getViewport({ scale: scale * 2 }) }).promise
    const image = canvas.toDataURL('image/jpeg', 0.82)
    page.cleanup()
    // 没有文字层的页（整页的图、扫描件）：内容占的地方算一整块，开会时能指
    if (!areas.length) {
      scanned++
      const box = inkBox(ctx.getImageData(0, 0, canvas.width, canvas.height))
      if (box) areas.push({ text: '', name: t().figureOfPage(i), kind: 'figure', x: box.x / 2, y: box.y / 2, w: box.w / 2, h: box.h / 2 })
    }
    pages.push({ graph: pageGraph({ w: vp.width, h: vp.height, no: i }, areas), underlay: image })
  }
  onPage?.(total, total)
  await task.destroy()
  // 看起来是论文：第一张板上起一个「论文结构」话题，按节列出来，讨论时可以往上挂批注、问题
  const paper = headings.length >= 2 && (captions > 0 || headings.some((h) => /^§1\b/.test(h)))
  if (paper && pages.length) {
    // 条目写成「Deep Residual Learning（§3）」：板书自己会给条目编号，章节号放后面；
    // 也和第一页上的标题原文「§3 Deep Residual Learning」区分开，不然会被当成那块钉死的原文、挂不上
    const clean = (s: string) => {
      const m = /^§(\S+)\s+(.*)$/.exec(s.replace(/[「」\n]/g, ''))
      return m ? `${m[2].slice(0, 28)}（§${m[1]}）` : s.replace(/[「」\n]/g, '').slice(0, 30)
    }
    const outline = t().paperOutline
    const ops = [`T 「${outline}」`, ...headings.slice(0, 30).map((h) => `L 「${outline}」 「${clean(h)}」`)].join('\n')
    pages[0] = { ...pages[0], graph: applyOps(pages[0].graph, parseOps(ops), 'ink') }
  }
  return { pages, total, unread: total - read, scanned, paper, refsSkipped, title: title?.slice(0, 80) }
}
