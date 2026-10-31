// 字形 → 笔画。每一笔是一串点（单位：字号 = 1），按书写顺序排好。
// - 汉字用 hanzi-writer-data 的骨架线，本身就是按笔顺排的；
// - ASCII 用 Hershey futural 单线字体；
// - 中文标点没有现成数据，用几笔手画近似。

import hershey from './hershey.json'

export type Pt = [number, number]
export type Stroke = Pt[]
/** 拉丁字母的写法：工整时是手写印刷体（futural），潦草时换成连写体（scripts） */
export type Latin = 'print' | 'script'

export interface Glyph {
  strokes: Stroke[]
  /** 前进宽度（字号 = 1） */
  advance: number
}

const cache = new Map<string, Promise<Glyph | null>>()
const CDN = 'https://cdn.jsdelivr.net/npm/hanzi-writer-data@2.0.1'

// hanzi-writer 的坐标：1024 见方，y 轴朝上，基线在 900
const hz = (p: number[]): Pt => [p[0] / 1024, (900 - p[1]) / 1024]

async function loadHanzi(ch: string): Promise<Glyph | null> {
  // 导出的回放 HTML 把用到的字直接塞在页面里，不联网也能写
  const embedded = (globalThis as { __boardcastHanzi?: Record<string, number[][][]> }).__boardcastHanzi?.[ch]
  if (embedded) return { strokes: embedded.map((s) => s.map(hz)), advance: 1 }
  for (const base of ['/hanzi', CDN]) {
    try {
      const r = await fetch(`${base}/${encodeURIComponent(ch)}.json`)
      if (!r.ok) continue
      const data = await r.json()
      // 本地是精简过的 medians 数组；CDN 上是完整对象
      const medians: number[][][] = Array.isArray(data) ? data : data.medians
      return { strokes: medians.map((s) => s.map(hz)), advance: 1 }
    } catch {
      /* 换下一个来源 */
    }
  }
  return null
}

// Hershey：y 轴朝下，大写字母大约占 1..22，宽度 = o × 1.68
const H_SCALE = 1 / 30
function hersheyGlyph(ch: string, latin: Latin): Glyph | null {
  if (ch === ' ') return { strokes: [], advance: 0.35 }
  const data = (hershey as Record<Latin, [string, number][]>)[latin][ch.charCodeAt(0) - 33]
  if (!data) return null
  const [d, o] = data
  const strokes: Stroke[] = []
  for (const part of d.split('M').slice(1)) {
    const pts = part
      .replace('L', ' ')
      .trim()
      .split(/\s+/)
      .map((xy) => xy.split(',').map(Number) as Pt)
      .filter((p) => p.length === 2 && !Number.isNaN(p[0]))
      .map(([x, y]): Pt => [x * H_SCALE, (y + 4) * H_SCALE])
    if (pts.length) strokes.push(pts)
  }
  return { strokes, advance: o * 1.68 * H_SCALE }
}

// 中文标点：几笔就够
const PUNCT: Record<string, Glyph> = {
  '，': { strokes: [[[0.22, 0.78], [0.26, 0.84], [0.2, 0.95]]], advance: 0.5 },
  '。': { strokes: [[[0.22, 0.76], [0.3, 0.74], [0.34, 0.82], [0.27, 0.88], [0.2, 0.83], [0.22, 0.76]]], advance: 0.5 },
  '、': { strokes: [[[0.18, 0.74], [0.3, 0.86]]], advance: 0.5 },
  '：': { strokes: [[[0.25, 0.38], [0.26, 0.42]], [[0.25, 0.74], [0.26, 0.78]]], advance: 0.5 },
  '；': { strokes: [[[0.25, 0.38], [0.26, 0.42]], [[0.27, 0.74], [0.22, 0.92]]], advance: 0.5 },
  '？': {
    strokes: [[[0.28, 0.3], [0.4, 0.18], [0.58, 0.2], [0.62, 0.34], [0.45, 0.5], [0.44, 0.64]], [[0.44, 0.8], [0.45, 0.84]]],
    advance: 0.8,
  },
  '！': { strokes: [[[0.4, 0.18], [0.38, 0.64]], [[0.38, 0.8], [0.39, 0.84]]], advance: 0.6 },
  '（': { strokes: [[[0.5, 0.1], [0.32, 0.4], [0.32, 0.7], [0.5, 0.98]]], advance: 0.6 },
  '）': { strokes: [[[0.2, 0.1], [0.38, 0.4], [0.38, 0.7], [0.2, 0.98]]], advance: 0.6 },
  '「': { strokes: [[[0.5, 0.12], [0.3, 0.12], [0.3, 0.6]]], advance: 0.6 },
  '」': { strokes: [[[0.3, 0.5], [0.3, 0.95], [0.1, 0.95]]], advance: 0.6 },
  '—': { strokes: [[[0.05, 0.55], [0.95, 0.53]]], advance: 1 },
  '→': { strokes: [[[0.05, 0.55], [0.9, 0.55]], [[0.68, 0.4], [0.92, 0.55], [0.68, 0.7]]], advance: 1 },
  '−': { strokes: [[[0.15, 0.55], [0.75, 0.55]]], advance: 0.9 },
  '×': { strokes: [[[0.2, 0.35], [0.65, 0.78]], [[0.65, 0.35], [0.2, 0.78]]], advance: 0.85 },
  '★': {
    strokes: [[[0.5, 0.1], [0.62, 0.42], [0.95, 0.42], [0.68, 0.62], [0.78, 0.95], [0.5, 0.74], [0.22, 0.95], [0.32, 0.62], [0.05, 0.42], [0.38, 0.42], [0.5, 0.1]]],
    advance: 1,
  },
  '↳': { strokes: [[[0.2, 0.2], [0.2, 0.7], [0.8, 0.7]], [[0.62, 0.55], [0.82, 0.7], [0.62, 0.85]]], advance: 0.9 },
}

const FULLWIDTH = /[！-～]/

export function glyph(ch: string, latin: Latin = 'print'): Promise<Glyph | null> {
  const ascii = ch.charCodeAt(0) < 128
  const full = FULLWIDTH.test(ch)
  const key = ascii || full ? `${latin}:${ch}` : ch
  let p = cache.get(key)
  if (!p) {
    if (PUNCT[ch]) p = Promise.resolve(PUNCT[ch])
    else if (ascii) p = Promise.resolve(hersheyGlyph(ch, latin))
    // 全角字母数字转半角，用 Hershey 写
    else if (full) p = Promise.resolve(hersheyGlyph(String.fromCharCode(ch.charCodeAt(0) - 0xfee0), latin))
    else p = loadHanzi(ch)
    cache.set(key, p)
  }
  return p
}
