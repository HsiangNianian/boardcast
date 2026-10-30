// 导入 PDF：每页一块板。页面渲染成一张图垫在底下，文字层拼成一块块「原文」钉在原位，开会时模型在上面圈画、连线、写批注。
// 扫描件没有文字层：只有底图，没有可指认的原文（以后由视觉模型来看）。
// pdf.js 按需加载；它要的字体映射（中文 PDF 离不开）、标准字体、wasm 由 vite 插件放在 /pdfjs/ 下。

import type { Page } from '../meetings.ts'
import { pageGraph } from '../engine/regions.ts'
import { type Piece, blocks } from './blocks.ts'

/** 页面在板上的宽度（板书坐标）；底图按两倍渲染，放大看也清楚 */
const WIDTH = 900
/** 最多导入多少页：每页一张图存在会议里，太多会让会议文件很大 */
export const MAX_PAGES = 40

export interface PdfImport {
  pages: Page[]
  /** PDF 一共几页（超过上限的没导） */
  total: number
  /** 没有文字层的页数（扫描件） */
  scanned: number
}

export async function importPdf(data: ArrayBuffer, onPage?: (done: number, total: number) => void): Promise<PdfImport> {
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
    throw new Error(/password/i.test(msg) ? '这个 PDF 有密码，先去掉密码再导入' : `PDF 打不开：${msg}`)
  }
  const total = doc.numPages
  const n = Math.min(total, MAX_PAGES)
  const pages: Page[] = []
  let scanned = 0
  for (let i = 1; i <= n; i++) {
    onPage?.(i - 1, n)
    const page = await doc.getPage(i)
    const scale = WIDTH / page.getViewport({ scale: 1 }).width
    const vp = page.getViewport({ scale })

    const canvas = document.createElement('canvas')
    canvas.width = Math.round(vp.width * 2)
    canvas.height = Math.round(vp.height * 2)
    const ctx = canvas.getContext('2d')!
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    await page.render({ canvas, canvasContext: ctx, viewport: page.getViewport({ scale: scale * 2 }) }).promise
    const image = canvas.toDataURL('image/jpeg', 0.82)

    const content = await page.getTextContent()
    const pieces: Piece[] = []
    for (const it of content.items) {
      if (!('str' in it) || !it.str.trim()) continue
      const t = pdfjs.Util.transform(vp.transform, it.transform)
      const h = Math.hypot(t[2], t[3])
      pieces.push({ text: it.str, x: t[4], y: t[5] - h, w: it.width * scale, h })
    }
    const areas = blocks(pieces)
    if (!areas.length) scanned++
    pages.push({ graph: pageGraph({ w: vp.width, h: vp.height }, areas), underlay: image })
    page.cleanup()
  }
  onPage?.(n, n)
  await task.destroy()
  return { pages, total, scanned }
}
