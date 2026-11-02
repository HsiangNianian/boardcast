// 笔：一串骨架点 → 像马克笔写出来的一笔（perfect-freehand 出轮廓，canvas 填充）。
// 和桌面版 src/ink/pen.ts 是同一支笔，只是换成 canvas，并且所有随机数都在一开始抽好：
// 潦草度推杆拖动时，同一个字只是「变形」，不会每一帧重新抽签乱跳。

import { DEFAULT_ENGLISH_WEIGHT, DEFAULT_MESSY, DEFAULT_WEIGHT } from '../../../src/ink/defaults'
import { getStroke } from 'perfect-freehand'

export type Pt = [number, number]
export type Stroke = Pt[]

/** 稳定的伪随机数发生器（mulberry32），同一个 seed 每次都一样 */
export function rng(seed: string): () => number {
  let h = 2166136261
  for (const c of seed) h = Math.imul(h ^ c.charCodeAt(0), 16777619)
  let a = h >>> 0
  return () => {
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export const clamp = (v: number, a = 0, b = 1) => (v < a ? a : v > b ? b : v)
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t
export const smoothstep = (a: number, b: number, v: number) => {
  const t = clamp((v - a) / (b - a))
  return t * t * (3 - 2 * t)
}
export const easeOut = (t: number) => 1 - (1 - t) ** 3
export const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2)
export const dist = (a: Pt, b: Pt) => Math.hypot(a[0] - b[0], a[1] - b[1])

/** 在点之间补点 */
export function densify(s: Stroke, step: number): Stroke {
  if (s.length < 2) return s.slice()
  const out: Stroke = [s[0]]
  for (let i = 0; i < s.length - 1; i++) {
    const a = s[i]
    const b = s[i + 1]
    const n = Math.max(1, Math.ceil(dist(a, b) / step))
    for (let k = 1; k <= n; k++) out.push([a[0] + ((b[0] - a[0]) * k) / n, a[1] + ((b[1] - a[1]) * k) / n])
  }
  return out
}

/** 拉普拉斯平滑：把折角磨圆。alpha 连续可调，所以潦草度一点点加，笔画就一点点从「写」变成「甩」 */
export function smooth(s: Stroke, alpha: number, iters: number): Stroke {
  if (alpha <= 0 || s.length < 3) return s
  let cur = s
  for (let it = 0; it < iters; it++) {
    const next: Stroke = [cur[0]]
    for (let i = 1; i < cur.length - 1; i++) {
      const p = cur[i]
      const a = cur[i - 1]
      const b = cur[i + 1]
      next.push([p[0] + alpha * ((a[0] + b[0]) / 2 - p[0]), p[1] + alpha * ((a[1] + b[1]) / 2 - p[1])])
    }
    next.push(cur[cur.length - 1])
    cur = next
  }
  return cur
}

/** 手抖：沿着笔画加一点低频偏移 */
export function wobble(s: Stroke, amp: number, f1: number, ph: number): Stroke {
  if (!amp || s.length < 2) return s
  const n = s.length - 1
  return s.map(([x, y], i) => {
    const d = Math.sin((i / n) * Math.PI * f1 + ph) * amp
    return [x + d * 0.6, y + d]
  })
}

export function polyLength(s: Stroke) {
  let L = 0
  for (let i = 1; i < s.length; i++) L += dist(s[i - 1], s[i])
  return L
}

/** 按弧长取 [a, b] 这一段（0..1） */
export function sliceRange(s: Stroke, a: number, b: number, total = polyLength(s)): Stroke {
  if (s.length < 2) return s.slice()
  const A = clamp(a) * total
  const B = clamp(b) * total
  const out: Stroke = []
  let acc = 0
  for (let i = 1; i < s.length; i++) {
    const p = s[i - 1]
    const q = s[i]
    const d = dist(p, q) || 1e-6
    const e = acc + d
    if (e >= A && acc <= B) {
      if (!out.length) {
        const t = clamp((A - acc) / d)
        out.push([lerp(p[0], q[0], t), lerp(p[1], q[1], t)])
      }
      if (e <= B) out.push(q)
      else {
        const t = clamp((B - acc) / d)
        out.push([lerp(p[0], q[0], t), lerp(p[1], q[1], t)])
        break
      }
    }
    acc = e
  }
  return out
}

export interface OutlineOpts {
  taper?: number
  thinning?: number
  last?: boolean
  /** 自定义压感（和点一一对应），给「立体」的圈用 */
  press?: number[]
}

export function outlineOf(pts: Stroke, size: number, o: OutlineOpts = {}): number[][] {
  const L = polyLength(pts)
  // 只给长笔收尾巴，起笔不收：马克笔落笔本来就是钝的
  const taper = o.taper ?? (L > size * 5 ? Math.min(size * 2, L * 0.2) : 0)
  const input = o.press ? pts.map((p, i) => [p[0], p[1], o.press![Math.min(i, o.press!.length - 1)]]) : pts
  return getStroke(input, {
    size,
    thinning: o.thinning ?? 0.32,
    smoothing: 0.5,
    streamline: 0.08,
    last: o.last ?? true,
    simulatePressure: !o.press,
    start: { taper: 0, cap: true },
    end: { taper, cap: true },
  })
}

export function toPath(o: number[][], p = new Path2D()): Path2D {
  if (o.length < 2) return p
  p.moveTo(o[0][0], o[0][1])
  for (let i = 1; i <= o.length; i++) {
    const a = o[i % o.length]
    const b = o[(i + 1) % o.length]
    p.quadraticCurveTo(a[0], a[1], (a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
  }
  p.closePath()
  return p
}

// —— 一笔、一组笔画 ——

export interface Mark {
  pts: Stroke
  len: number
  size: number
  alpha: number
  taper?: number
  press?: number[]
  path?: Path2D
}

/** 连笔时带出来的细丝：thin 是它的浓淡（0..1） */
export interface RawMark {
  pts: Stroke
  thin?: number
  f1: number
  ph: number
  latin?: boolean
}

export interface Box {
  x0: number
  y0: number
  x1: number
  y1: number
}

export interface Piece {
  id: string
  color: string
  alpha: number
  marks: Mark[]
  /** 写字动画用的总长度（含笔与笔之间的停顿） */
  len: number
  gap: number
  box: Box
}

export interface PieceOpts {
  alpha?: number
  /** 手抖的倍数 */
  wob?: number
  taper?: number
  step?: number
  gap?: number
  press?: (t: number, p: Pt) => number
  /** 笔画粗细的倍数：只改笔头宽度，不改字形（手抖、补点仍按原笔宽算，推粗细时字不会跟着变形） */
  weight?: number
  englishWeight?: number
}

export function makePiece(id: string, color: string, raws: (Stroke | RawMark)[], pen: number, messy = DEFAULT_MESSY, o: PieceOpts = {}): Piece {
  const r = rng(id)
  // 手抖到 0.8 以后基本封顶：再狂也不能把笔画抖成一团
  const me = messy < 0.8 ? messy : 0.8 + (messy - 0.8) * 0.25
  const amp = pen * (0.12 + 0.42 * me) * (o.wob ?? 1)
  const step = o.step ?? Math.max(1, pen * 0.38)
  const marks: Mark[] = []
  const box: Box = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity }
  const gap = o.gap ?? pen * 5
  let len = 0
  for (const raw of raws) {
    const isStroke = Array.isArray(raw)
    const src = isStroke ? raw : raw.pts
    if (!src.length) continue
    const f1 = isStroke ? 0.5 + r() * 2 : raw.f1
    const ph = isStroke ? r() * Math.PI * 2 : raw.ph
    const thin = isStroke ? undefined : raw.thin
    const weight = !isStroke && raw.latin ? o.englishWeight ?? DEFAULT_ENGLISH_WEIGHT : o.weight ?? DEFAULT_WEIGHT
    const size = (thin !== undefined ? pen * 0.42 : pen) * weight
    const pts = wobble(densify(src, step), thin !== undefined ? amp * 0.5 : amp, f1, ph)
    const L = polyLength(pts)
    let press: number[] | undefined
    if (o.press) {
      press = []
      let acc = 0
      for (let i = 0; i < pts.length; i++) {
        if (i) acc += dist(pts[i - 1], pts[i])
        press.push(o.press(L ? acc / L : 0, pts[i]))
      }
    }
    marks.push({ pts, len: L, size, alpha: thin ?? 1, taper: o.taper, press })
    len += L + gap
    for (const [x, y] of pts) {
      if (x < box.x0) box.x0 = x
      if (y < box.y0) box.y0 = y
      if (x > box.x1) box.x1 = x
      if (y > box.y1) box.y1 = y
    }
  }
  const pad = pen * Math.max(1, o.weight ?? DEFAULT_WEIGHT, o.englishWeight ?? DEFAULT_ENGLISH_WEIGHT)
  box.x0 -= pad
  box.y0 -= pad
  box.x1 += pad
  box.y1 += pad
  return { id, color, alpha: o.alpha ?? 1, marks, len: Math.max(1, len - gap), gap, box }
}

function fullPath(m: Mark) {
  if (!m.path) m.path = toPath(outlineOf(m.pts, m.size, { taper: m.taper, press: m.press }))
  return m.path
}

/**
 * 把一组笔画画出来。f 是写到哪儿了（0..1，按笔画长度算，笔与笔之间有停顿）；
 * from > 0 时从开头擦掉一截（圈被「擦掉」时，擦痕追着笔尾走）。
 */
export interface DrawOpts {
  /** 描边加粗（像素），笔触「加深一圈」 */
  bold?: number
  /** 换个颜色画（影子） */
  color?: string
}

export function drawPiece(ctx: CanvasRenderingContext2D, p: Piece, f = 1, a = 1, from = 0, o: DrawOpts = {}) {
  if (f <= 0 || a <= 0 || from >= 1) return
  const color = o.color ?? p.color
  ctx.fillStyle = color
  const bold = o.bold ?? 0
  if (bold > 0.05) {
    ctx.strokeStyle = color
    ctx.lineWidth = bold
    ctx.lineJoin = 'round'
  }
  const fill = (path: Path2D) => {
    ctx.fill(path)
    if (bold > 0.05) ctx.stroke(path)
  }
  const end = f >= 1 ? Infinity : f * p.len
  const start = from * p.len
  let acc = 0
  for (const m of p.marks) {
    if (acc > end) break
    const a0 = acc
    const a1 = acc + m.len
    acc = a1 + p.gap
    if (a1 < start) continue
    ctx.globalAlpha = p.alpha * m.alpha * a
    if (end >= a1 && start <= a0) {
      fill(fullPath(m))
      continue
    }
    const t0 = clamp((start - a0) / (m.len || 1))
    const t1 = clamp((end - a0) / (m.len || 1))
    if (t1 <= t0) continue
    const part = sliceRange(m.pts, t0, t1, m.len)
    if (part.length < 2) continue
    const press = m.press && sliceRange(m.press.map((v, i) => [i, v] as Pt), t0, t1).map((q) => q[1])
    fill(toPath(outlineOf(part, m.size, { taper: t1 >= 1 ? m.taper : 0, last: t1 >= 1, press })))
  }
  ctx.globalAlpha = 1
}

/** 画布：按设备像素比放大，坐标仍然用 CSS 像素 */
export function fitCanvas(c: HTMLCanvasElement, w: number, h: number, maxDpr = 2) {
  const dpr = Math.min(maxDpr, window.devicePixelRatio || 1)
  const W = Math.max(1, Math.round(w * dpr))
  const H = Math.max(1, Math.round(h * dpr))
  if (c.width !== W || c.height !== H) {
    c.width = W
    c.height = H
  }
  c.style.width = `${w}px`
  c.style.height = `${h}px`
  const ctx = c.getContext('2d')!
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  return { ctx, dpr }
}

export const reduceMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches
