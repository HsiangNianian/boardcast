// 导出：Markdown、PNG、SVG、.excalidraw。
// - PNG / SVG：手写模式把画面上的 SVG 按内容范围裁出来（PNG 再栅格化），看到什么导出什么；铅笔模式用 Excalidraw 自己的导出；
// - .excalidraw：铅笔模式带上手动改过的内容；手写模式按同一份排版现生成一份；
// - 桌面版弹系统的另存为对话框（外壳代写文件），网页版直接下载。

import { exportToBlob, exportToSvg, serializeAsJSON } from '@excalidraw/excalidraw'
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { board } from './board/layout.ts'
import { type SaidLine, toMarkdown } from './board/markdown.ts'
import { type Look, scene } from './board/scene.ts'
import { isDesktop } from './desktop.ts'
import type { Graph } from './engine/graph.ts'

export type ExportKind = 'md' | 'png' | 'svg' | 'excalidraw'

interface Board {
  graph: Graph
  lines: SaidLine[]
  mode: 'ink' | 'pencil'
  /** 铅笔模式下的 Excalidraw；手写模式为 null */
  api: ExcalidrawImperativeAPI | null
  look: Look
  paper: string
  title?: string
  summary?: string
}

const pad2 = (n: number) => String(n).padStart(2, '0')
function fileName(ext: string, when = new Date()) {
  return `boardcast-${when.getFullYear()}${pad2(when.getMonth() + 1)}${pad2(when.getDate())}-${pad2(when.getHours())}${pad2(when.getMinutes())}.${ext}`
}

/** 手写板：把画面上的 SVG 裁到内容范围，去掉还没写完的遮罩，垫上纸色 */
async function inkSvg(b: Board, scale = 1): Promise<{ svg: string; width: number; height: number }> {
  const el = document.querySelector<SVGSVGElement>('.ink svg')
  if (!el) throw new Error('找不到白板')
  const { bounds } = await board.layout(b.graph)
  const P = 48
  const x = bounds.x0 - P
  const y = bounds.y0 - P
  const w = bounds.x1 - bounds.x0 + 2 * P
  const h = bounds.y1 - bounds.y0 + 2 * P
  const NS = 'http://www.w3.org/2000/svg'
  const svg = el.cloneNode(true) as SVGSVGElement
  svg.setAttribute('xmlns', NS)
  svg.setAttribute('viewBox', `${x} ${y} ${w} ${h}`)
  svg.setAttribute('width', String(Math.round(w * scale)))
  svg.setAttribute('height', String(Math.round(h * scale)))
  svg.removeAttribute('data-messy')
  // 正在写的笔画用遮罩一点点露出来；导出时直接全写完
  for (const m of svg.querySelectorAll('[mask]')) m.removeAttribute('mask')
  for (const m of svg.querySelectorAll('mask')) m.remove()
  const bg = document.createElementNS(NS, 'rect')
  for (const [k, v] of Object.entries({ x, y, width: w, height: h, fill: b.paper })) bg.setAttribute(k, String(v))
  svg.insertBefore(bg, svg.firstChild)
  return { svg: new XMLSerializer().serializeToString(svg), width: Math.round(w * scale), height: Math.round(h * scale) }
}

/** 手写板的 PNG：同一份 SVG 画到 2 倍大的画布上 */
async function inkPng(b: Board): Promise<Blob> {
  const { bounds } = await board.layout(b.graph)
  const scale = Math.min(2, 8000 / Math.max(bounds.x1 - bounds.x0 + 96, bounds.y1 - bounds.y0 + 96))
  const { svg, width, height } = await inkSvg(b, scale)
  const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }))
  try {
    const img = new Image()
    img.src = url
    await img.decode()
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    canvas.getContext('2d')!.drawImage(img, 0, 0, width, height)
    return await new Promise<Blob>((ok, fail) => canvas.toBlob((x) => (x ? ok(x) : fail(new Error('生成图片失败'))), 'image/png'))
  } finally {
    URL.revokeObjectURL(url)
  }
}

async function pencilSvg(api: ExcalidrawImperativeAPI, paper: string): Promise<string> {
  const el = await exportToSvg({
    elements: api.getSceneElements(),
    appState: { ...api.getAppState(), exportBackground: true, exportWithDarkMode: false, viewBackgroundColor: paper },
    files: api.getFiles(),
    exportPadding: 48,
  })
  return new XMLSerializer().serializeToString(el)
}

async function pencilPng(api: ExcalidrawImperativeAPI, paper: string): Promise<Blob> {
  return exportToBlob({
    elements: api.getSceneElements(),
    // 导出永远是浅色的纸：深色模式只是 Excalidraw 在画面上反了色
    appState: { ...api.getAppState(), exportBackground: true, exportWithDarkMode: false, viewBackgroundColor: paper },
    files: api.getFiles(),
    mimeType: 'image/png',
    exportPadding: 48,
    getDimensions: (width: number, height: number) => ({ width: width * 2, height: height * 2, scale: 2 }),
  })
}

async function excalidrawFile(b: Board): Promise<string> {
  if (b.api) return serializeAsJSON(b.api.getSceneElements(), b.api.getAppState(), b.api.getFiles(), 'local')
  const l = await board.layout(b.graph)
  return serializeAsJSON(scene(b.graph, l, new Set(), b.look), { viewBackgroundColor: b.paper }, {}, 'local')
}

export async function build(kind: ExportKind, b: Board): Promise<{ name: string; blob: Blob }> {
  if (kind === 'md') return { name: fileName('md'), blob: new Blob([toMarkdown(b.graph, b.lines, b.title, new Date(), b.summary)], { type: 'text/markdown' }) }
  if (kind === 'excalidraw')
    return { name: fileName('excalidraw'), blob: new Blob([await excalidrawFile(b)], { type: 'application/json' }) }
  if (kind === 'svg') {
    const svg = b.mode === 'pencil' && b.api ? await pencilSvg(b.api, b.paper) : (await inkSvg(b)).svg
    return { name: fileName('svg'), blob: new Blob([svg], { type: 'image/svg+xml' }) }
  }
  const blob = b.mode === 'pencil' && b.api ? await pencilPng(b.api, b.paper) : await inkPng(b)
  return { name: fileName('png'), blob }
}

function base64(blob: Blob): Promise<string> {
  return new Promise((ok, fail) => {
    const r = new FileReader()
    r.onload = () => ok(String(r.result).split(',')[1] ?? '')
    r.onerror = () => fail(r.error)
    r.readAsDataURL(blob)
  })
}

/** 存文件：返回存到哪儿（网页版是文件名），用户取消时返回 null */
export async function saveFile(name: string, blob: Blob): Promise<string | null> {
  if (isDesktop) {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<string | null>('export_save', { name, dataBase64: await base64(blob) })
  }
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
  return name
}
