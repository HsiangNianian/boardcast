// 导出：Markdown、PNG、SVG、动态 SVG、.excalidraw。
// - PNG / SVG：手写模式把画面上的 SVG 按内容范围裁出来（PNG 再栅格化），看到什么导出什么；铅笔模式用 Excalidraw 自己的导出；
// - .excalidraw：铅笔模式带上手动改过的内容；手写模式按同一份排版现生成一份；
// - 桌面版弹系统的另存为对话框（外壳代写文件），网页版直接下载。

import { exportToBlob, exportToSvg, serializeAsJSON } from '@excalidraw/excalidraw'
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { type LayoutMemory, StableLayout, board } from './board/layout.ts'
import { type SaidLine, toMarkdown } from './board/markdown.ts'
import { type Look, scene } from './board/scene.ts'
import { isDesktop } from './desktop.ts'
import type { Graph } from './engine/graph.ts'
import { type Step, inkOnly, replayTo } from './engine/steps.ts'
import { type ReplayData, replayHtml, usedHan } from './player/data.ts'
import { currentLang, msg } from './i18n/index.ts'
import { type FreeScene, hidesInk, imageSvg, mergeScene, overlayElements, overlaySvg, sceneBounds, withInheritedInk, withPageUnderlay } from './board/freeScene.ts'
import { withSceneOverlay } from './board/sceneOverlay.ts'

export type ExportKind = 'md' | 'png' | 'svg' | 'anim' | 'excalidraw' | 'html' | 'pdf'

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
  /** 一场会有几张板时，所有板（Markdown 按板分节） */
  pages?: Graph[]
  /** 导出回放网页用：整场会的数据 */
  replay?: ReplayData
  /** 动态 SVG 用：这张板的回放记录（定先后）、导入页的底图、笔迹的潦草度和粗细（和白板上一样） */
  steps?: Step[]
  underlay?: string
  base?: Graph
  baseScene?: FreeScene
  messy?: number
  weight?: number
  englishWeight?: number
  scene?: FreeScene
  layout?: LayoutMemory
}

const pad2 = (n: number) => String(n).padStart(2, '0')
function fileName(ext: string, when = new Date()) {
  return `boardcast-${when.getFullYear()}${pad2(when.getMonth() + 1)}${pad2(when.getDate())}-${pad2(when.getHours())}${pad2(when.getMinutes())}.${ext}`
}

/** 手写板：把画面上的 SVG 裁到内容范围，去掉还没写完的遮罩，垫上纸色 */
async function inkSvg(b: Board, scale = 1): Promise<{ svg: string; width: number; height: number }> {
  const [{ compose }, { WHITEBOARD }] = await Promise.all([import('./ink/compose.ts'), import('./ink/InkBoard.tsx')])
  const layout = new StableLayout()
  layout.restore(b.layout)
  const l = await layout.layout(b.graph)
  const bounds = sceneBounds(l.bounds, b.scene?.overlay)
  const pieces = (await compose(b.graph, l, new Set(), WHITEBOARD, b.messy, b.weight, b.base, undefined, b.englishWeight)).filter((p) => !hidesInk(p.key, b.scene?.overlay))
  const P = 48
  const x = bounds.x0 - P, y = bounds.y0 - P
  const w = bounds.x1 - bounds.x0 + 2 * P, h = bounds.y1 - bounds.y0 + 2 * P
  const width = Math.round(w * scale), height = Math.round(h * scale)
  const u = b.graph.underlay
  const underlay = b.underlay && u ? imageSvg(b.underlay, 0, 0, u.w, u.h) : ''
  const paths = [true, false].map((under) => pieces.filter((p) => !!p.under === under).map((p) => `<g fill="${p.color}" opacity="${p.opacity}"${under ? ' style="mix-blend-mode:multiply"' : ''}>${p.strokes.map((s) => `<path d="${s.d}"/>`).join('')}</g>`).join('')).join('')
  return { svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${w} ${h}" width="${width}" height="${height}"><rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${WHITEBOARD.paper}"/>${underlay}${paths}${overlaySvg(b.scene?.overlay)}</svg>`, width, height }
}

/** 手写板的 PNG：同一份 SVG 画到 2 倍大的画布上 */
async function inkPng(b: Board): Promise<Blob> {
  const layout = new StableLayout()
  layout.restore(b.layout)
  const bounds = sceneBounds((await layout.layout(b.graph)).bounds, b.scene?.overlay)
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
    return await new Promise<Blob>((ok, fail) => canvas.toBlob((x) => (x ? ok(x) : fail(new Error(msg().errors.imageFailed))), 'image/png'))
  } finally {
    URL.revokeObjectURL(url)
  }
}

/** 动态 SVG：这张板最终的样子，按会上画出来的先后一笔一笔写出来，写完停在全貌（见 ink/animate.ts） */
async function animSvg(b: Board): Promise<string> {
  const [{ compose }, { WHITEBOARD }, { animatedSvg, ranks, timeline }] = await Promise.all([
    import('./ink/compose.ts'),
    import('./ink/InkBoard.tsx'),
    import('./ink/animate.ts'),
  ])
  // 铅笔稿是一闪而过的，不算板书
  const g = inkOnly(b.graph)
  if (!g.nodes.length && !b.scene?.overlay?.url && !b.underlay) throw new Error(msg().errors.emptyBoard)
  // 另用一份排版，从白板现在的位置开始排：看到的和导出的一样，也不动白板自己的排版记忆
  const layout = new StableLayout()
  layout.restore(b.layout)
  const l = await layout.layout(g)
  const pieces = (await compose(g, l, new Set(), WHITEBOARD, b.messy, b.weight, b.base, undefined, b.englishWeight)).filter((p) => !hidesInk(p.key, b.scene?.overlay))
  const { timed } = timeline(pieces, ranks(g, l, b.steps ?? [], pieces.map((p) => p.key)))
  const P = 48
  const bounds = sceneBounds(l.bounds, b.scene?.overlay)
  const box = { x: bounds.x0 - P, y: bounds.y0 - P, w: bounds.x1 - bounds.x0 + 2 * P, h: bounds.y1 - bounds.y0 + 2 * P }
  const u = g.underlay
  return animatedSvg(timed, box, WHITEBOARD.paper, b.underlay && u ? { href: b.underlay, w: u.w, h: u.h } : undefined).replace('</svg>', `${overlaySvg(b.scene?.overlay)}</svg>`)
}

async function pencilSvg(state: FreeScene, paper: string): Promise<string> {
  const el = await exportToSvg({
    elements: state.elements,
    appState: { exportBackground: true, exportWithDarkMode: false, viewBackgroundColor: paper },
    files: state.files,
    exportPadding: 48,
  })
  return new XMLSerializer().serializeToString(el)
}

async function pencilPng(state: FreeScene, paper: string): Promise<Blob> {
  return exportToBlob({
    elements: state.elements,
    // 导出永远是浅色的纸：深色模式只是 Excalidraw 在画面上反了色
    appState: { exportBackground: true, exportWithDarkMode: false, viewBackgroundColor: paper },
    files: state.files,
    mimeType: 'image/png',
    exportPadding: 48,
    getDimensions: (width: number, height: number) => {
      const scale = Math.min(2, 8000 / Math.max(width, height))
      return { width: width * scale, height: height * scale, scale }
    },
  })
}

/** Excalidraw needs the page underlay as an actual image element as well as recovery metadata. */
function pencilScene(b: Board): FreeScene {
  return withPageUnderlay(withInheritedInk(b.scene!, b.graph, b.base, b.baseScene), b.graph, b.underlay)
}

async function excalidrawFile(b: Board): Promise<string> {
  const state = pencilScene(b)
  const drawing = JSON.parse(serializeAsJSON(state.elements, { viewBackgroundColor: b.paper }, state.files, 'local'))
  return JSON.stringify({ ...drawing, boardcast: { version: 2, graph: b.graph, layout: b.layout, underlay: b.underlay, generated: b.scene!.generated, overrides: b.scene!.overrides, hiddenGenerated: b.scene!.hiddenGenerated, modelCandidates: b.scene!.modelCandidates } })
}

/** 回放网页：播放器、整场会的数据、用到的汉字笔画，全塞进一个 HTML，双击就能看，不联网 */
async function replayFile(data: ReplayData): Promise<Blob> {
  const at = (p: string) => new URL(p, document.baseURI)
  const res = await fetch(at('player/player.js'))
  if (!res.ok) throw new Error(msg().errors.noPlayer)
  const player = await res.text()
  const hanzi: Record<string, unknown> = {}
  await Promise.all(
    usedHan(data).map(async (ch) => {
      try {
        const r = await fetch(at(`hanzi/${encodeURIComponent(ch)}.json`))
        if (!r.ok) return
        const d = await r.json()
        hanzi[ch] = Array.isArray(d) ? d : d.medians
      } catch {
        // 缺一个字：播放器里那个字照常从网上取，取不到就空着
      }
    }),
  )
  return new Blob([replayHtml(data, player, hanzi)], { type: 'text/html' })
}

export async function build(kind: ExportKind, b: Board): Promise<{ name: string; blob: Blob }> {
  // Capture before the first await: switching pages/modes cannot change this export.
  b = { ...b, graph: structuredClone(b.graph), base: structuredClone(b.base), baseScene: structuredClone(b.baseScene), scene: structuredClone(b.scene), layout: structuredClone(b.layout ?? board.snapshot()), replay: structuredClone(b.replay) }
  // XML cannot represent control characters or unpaired surrogates in saved titles.
  const svgTitle = (b.title?.trim() || msg().app.untitled).replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\ud800-\udfff\ufffe\uffff]/gu, '\ufffd')
  if ((kind === 'html' || kind === 'pdf') && b.replay) {
    for (const page of b.replay.pages) {
      const layout = new StableLayout()
      layout.restore(page.layout)
      const projected = await layout.layout(page.graph)
      page.layout = layout.snapshot()
      if (page.scene) {
        page.scene = await withSceneOverlay(mergeScene(page.scene, await scene(page.graph, projected, new Set(), b.look)), page.graph, page.base, page.steps?.[0]?.scene)
      }
      for (const [i, step] of (page.steps ?? []).entries()) if (step.scene) step.scene = await withSceneOverlay(step.scene, replayTo(page.steps!, i + 1), page.base, page.steps?.[0]?.scene)
    }
  }
  if (kind !== 'md' && kind !== 'html' && kind !== 'pdf') {
    const layout = new StableLayout()
    layout.restore(b.layout)
    b.scene = await withSceneOverlay(mergeScene(b.scene, await scene(b.graph, await layout.layout(b.graph), new Set(), b.look)), b.graph, b.base, b.baseScene)
  }
  if (kind === 'html') {
    if (!b.replay) throw new Error(msg().doc.noData)
    return { name: fileName('html'), blob: await replayFile(b.replay) }
  }
  if (kind === 'pdf') {
    if (!b.replay) throw new Error(msg().doc.noData)
    const { meetingPdf } = await import('./board/meetingPdf.ts')
    const bytes = await meetingPdf(b.replay)
    return { name: fileName('pdf'), blob: new Blob([bytes as BlobPart], { type: 'application/pdf' }) }
  }
  if (kind === 'md') {
    const pages = b.replay?.pages ?? [{ scene: b.scene, underlay: b.underlay }]
    const omitted = pages.some((p) => p.underlay || p.scene && overlayElements(p.scene).elements.length)
    const note = omitted ? currentLang() === 'zh' ? '\n\n> 此 Markdown 不包含自由图形、笔迹、图片和自定义视觉样式；请同时导出 SVG、PDF 或 Excalidraw 保留完整视觉内容。\n' : '\n\n> This Markdown omits free drawings, strokes, images and custom visual styling. Export SVG, PDF or Excalidraw alongside it to preserve the complete visual board.\n' : ''
    return { name: fileName('md'), blob: new Blob([toMarkdown(b.graph, b.lines, b.title, new Date(), b.summary, b.pages), note], { type: 'text/markdown' }) }
  }
  if (kind === 'excalidraw')
    return { name: fileName('excalidraw'), blob: new Blob([await excalidrawFile(b)], { type: 'application/json' }) }
  if (kind === 'svg' || kind === 'anim') {
    const svg = kind === 'anim' ? await animSvg(b) : b.mode === 'pencil' ? await pencilSvg(pencilScene(b), b.paper) : (await inkSvg(b)).svg
    const doc = new DOMParser().parseFromString(svg, 'image/svg+xml'), root = doc.documentElement
    if (root.namespaceURI !== 'http://www.w3.org/2000/svg' || root.localName !== 'svg') throw new Error(msg().errors.imageFailed)
    const title = doc.createElementNS(root.namespaceURI, 'title')
    title.textContent = svgTitle
    root.prepend(title)
    return { name: fileName(kind === 'anim' ? 'anim.svg' : 'svg'), blob: new Blob([new XMLSerializer().serializeToString(doc)], { type: 'image/svg+xml' }) }
  }
  const blob = b.mode === 'pencil' ? await pencilPng(pencilScene(b), b.paper) : await inkPng(b)
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
