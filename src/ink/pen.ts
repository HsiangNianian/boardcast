// 笔：把一串骨架点变成像马克笔写出来的一笔。
// - 每个字随机一点倾斜、缩放、基线起伏，同一个 id 每次都一样，重排时不抖；
// - 每一笔起笔收笔变细（perfect-freehand 的 taper），速度越快越细；
// - 线条、圈、箭头也走同一支笔，画风统一。

import { getStroke } from 'perfect-freehand'
import type { Pt, Stroke } from './glyphs.ts'

/** 稳定的伪随机数发生器（mulberry32） */
export function rng(seed: string) {
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

/** 在两点之间补点，让 perfect-freehand 有足够的样本做出粗细变化 */
function densify(s: Stroke, step: number): Stroke {
  const out: Stroke = []
  for (let i = 0; i < s.length; i++) {
    const a = s[i]
    out.push(a)
    const b = s[i + 1]
    if (!b) break
    const n = Math.floor(Math.hypot(b[0] - a[0], b[1] - a[1]) / step)
    for (let k = 1; k < n; k++) out.push([a[0] + ((b[0] - a[0]) * k) / n, a[1] + ((b[1] - a[1]) * k) / n])
  }
  return out
}

/** 手抖：沿着笔画加一点低频的偏移 */
function wobble(s: Stroke, amp: number, r: () => number): Stroke {
  const f1 = 0.5 + r() * 2
  const ph = r() * Math.PI * 2
  return s.map(([x, y], i) => {
    const t = i / Math.max(1, s.length - 1)
    const d = Math.sin(t * Math.PI * f1 + ph) * amp
    return [x + d * 0.6, y + d]
  })
}

function toPath(outline: number[][]): string {
  if (outline.length < 2) return ''
  const d = outline.reduce(
    (acc: (string | number)[], [x0, y0], i, arr) => {
      const [x1, y1] = arr[(i + 1) % arr.length]
      acc.push(x0.toFixed(1), y0.toFixed(1), ((x0 + x1) / 2).toFixed(1), ((y0 + y1) / 2).toFixed(1))
      return acc
    },
    ['M', outline[0][0].toFixed(1), outline[0][1].toFixed(1), 'Q'],
  )
  return `${d.join(' ')} Z`
}

export interface Inked {
  /** 填充用的轮廓（perfect-freehand 输出） */
  d: string
  /** 骨架线，写字动画用它做遮罩 */
  center: string
  length: number
}

export function ink(
  points: Stroke,
  size: number,
  r: () => number,
  opts: { wobble?: number; taper?: number; messy?: number } = {},
): Inked {
  const pts = wobble(densify(points, size * 0.3), opts.wobble ?? size * (0.12 + 0.5 * (opts.messy ?? 0)), r)
  let length = 0
  for (let i = 1; i < pts.length; i++) length += Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1])
  // perfect-freehand 的收尖对短笔非常狠（实测 8px 的笔设 1px 收尖，平均宽度就掉一半以上），
  // 所以只给长笔收尾巴，起笔一律不收：马克笔落笔本来就是钝的
  const taper = length > size * 5 ? Math.min(opts.taper ?? size * 2, length * 0.2) : 0
  const outline = getStroke(pts, {
    size,
    thinning: 0.32,
    smoothing: 0.5,
    // streamline 会让笔尖落后于点，短笔画会被吃掉一截，所以压得很低；
    // last: true 告诉它这一笔已经写完，最后一个点要算进去
    streamline: 0.08,
    last: true,
    simulatePressure: true,
    start: { taper: 0, cap: true },
    end: { taper, cap: true },
  })
  return {
    d: toPath(outline),
    center: `M ${pts.map(([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`).join(' L ')}`,
    length,
  }
}

/** Chaikin 平滑：把折角磨圆，越潦草磨得越多，笔画就从「写」变成「甩」 */
function chaikin(s: Stroke, times: number): Stroke {
  let out = s
  for (let t = 0; t < times && out.length > 2; t++) {
    const next: Stroke = [out[0]]
    for (let i = 0; i < out.length - 1; i++) {
      const [ax, ay] = out[i]
      const [bx, by] = out[i + 1]
      next.push([ax * 0.75 + bx * 0.25, ay * 0.75 + by * 0.25], [ax * 0.25 + bx * 0.75, ay * 0.25 + by * 0.75])
    }
    next.push(out[out.length - 1])
    out = next
  }
  return out
}

/** 笔画两头随机出头或缩回一点 */
function overshoot(s: Stroke, amount: number, r: () => number): Stroke {
  if (s.length < 2 || amount <= 0) return s
  const ext = (a: Pt, b: Pt, k: number): Pt => [a[0] + (a[0] - b[0]) * k, a[1] + (a[1] - b[1]) * k]
  const out = [...s]
  out[0] = ext(s[0], s[1], (r() - 0.4) * amount)
  out[out.length - 1] = ext(s[s.length - 1], s[s.length - 2], (r() - 0.3) * amount)
  return out
}

/**
 * 一个字落到纸上：位置、字号，加上随手写的变形。
 * `messy` 从 0（很工整）到 1（很潦草），同时控制：
 * 字的歪斜、大小、基线起伏、整体右倾，每一笔的错位和转动，折角磨圆的程度，笔画出头。
 * 字形坐标以字号为 1，(0.5, 0.5) 是字的中心。
 */
export function placeGlyph(strokes: Stroke[], x: number, y: number, s: number, r: () => number, messy: number): Stroke[] {
  const m = Math.min(1, Math.max(0, messy))
  const rot = (r() - 0.5) * (0.05 + 0.22 * m)
  const k = s * (1 + (r() - 0.5) * (0.08 + 0.3 * m))
  const dy = (r() - 0.5) * s * (0.04 + 0.16 * m)
  // 右倾：y 越往上 x 越往右
  const slant = 0.04 + 0.3 * m
  const cos = Math.cos(rot)
  const sin = Math.sin(rot)
  return strokes.map((stroke) => {
    // 每一笔自己再偏一点、转一点
    const sx = (r() - 0.5) * 0.08 * m
    const sy = (r() - 0.5) * 0.08 * m
    const sr = (r() - 0.5) * 0.35 * m
    const [cx, cy] = stroke.reduce(([a, b], [u, v]) => [a + u / stroke.length, b + v / stroke.length], [0, 0])
    const local = stroke.map(([u, v]): Pt => {
      const du = u - cx
      const dv = v - cy
      return [cx + du * Math.cos(sr) - dv * Math.sin(sr) + sx, cy + du * Math.sin(sr) + dv * Math.cos(sr) + sy]
    })
    const shaped = overshoot(chaikin(local, Math.round(m * 2.4)), 0.25 * m, r)
    return shaped.map(([u, v]): Pt => {
      const px = (u - 0.5 + (0.5 - v) * slant) * k
      const py = (v - 0.5) * k
      return [x + s * 0.5 + px * cos - py * sin, y + s * 0.5 + dy + px * sin + py * cos]
    })
  })
}

// —— 线条和形状：都是近似的手画轨迹，交给同一支笔 ——

export function line(a: Pt, b: Pt, r: () => number, sag = 0.02): Stroke {
  const len = Math.hypot(b[0] - a[0], b[1] - a[1])
  const nx = -(b[1] - a[1]) / (len || 1)
  const ny = (b[0] - a[0]) / (len || 1)
  const bow = (r() - 0.5) * 2 * sag * len
  const mid: Pt = [(a[0] + b[0]) / 2 + nx * bow, (a[1] + b[1]) / 2 + ny * bow]
  return [a, mid, b]
}

/** 圈住跨行的一句话：沿着几行的外缘走一圈 */
export function outline(boxes: { x: number; y: number; w: number; h: number }[], pad: number, r: () => number): Stroke {
  const right: Pt[] = []
  const left: Pt[] = []
  for (const b of boxes) {
    right.push([b.x + b.w + pad, b.y - pad / 2], [b.x + b.w + pad, b.y + b.h + pad / 2])
    left.unshift([b.x - pad, b.y + b.h + pad / 2], [b.x - pad, b.y - pad / 2])
  }
  const corners = [...right, ...left]
  corners.push(corners[0])
  const pts: Stroke = []
  for (let i = 0; i + 1 < corners.length; i++) pts.push(...line(corners[i], corners[i + 1], r, 0.008))
  return pts
}

/** 圈：一圈多一点，起点和终点错开，像真的随手圈起来 */
export function loop(cx: number, cy: number, rx: number, ry: number, r: () => number): Stroke {
  const start = r() * Math.PI * 2
  const turns = 1.08 + r() * 0.12
  const n = 40
  const pts: Stroke = []
  for (let i = 0; i <= n; i++) {
    const t = start + (i / n) * turns * Math.PI * 2
    const grow = 1 + (i / n) * 0.06
    pts.push([cx + Math.cos(t) * rx * grow, cy + Math.sin(t) * ry * grow])
  }
  return pts
}

/** 圆角框：四笔，拐角处略微出头 */
export function box(x: number, y: number, w: number, h: number, r: () => number): Stroke[] {
  const o = () => (r() - 0.3) * 6
  return [
    line([x + o(), y], [x + w + o(), y + o() * 0.3], r, 0.01),
    line([x + w, y + o()], [x + w + o() * 0.3, y + h + o()], r, 0.01),
    line([x + w + o(), y + h], [x + o(), y + h + o() * 0.3], r, 0.01),
    line([x, y + h + o()], [x + o() * 0.3, y + o()], r, 0.01),
  ]
}

/** 箭头：一笔干，两笔头 */
export function arrow(pts: Pt[], r: () => number): Stroke[] {
  const shaft = pts.length === 2 ? line(pts[0], pts[1], r, 0.04) : pts
  const end = shaft[shaft.length - 1]
  const prev = shaft[Math.max(0, shaft.length - 2)]
  const ang = Math.atan2(end[1] - prev[1], end[0] - prev[0])
  const L = 14
  const spread = 0.45 + r() * 0.15
  const head = (s: number): Stroke => [[end[0] - Math.cos(ang + s) * L, end[1] - Math.sin(ang + s) * L], end]
  return [shaft, head(spread), head(-spread)]
}

/** 划掉：一道略斜的线；一半的概率再补一道 */
export function strike(x: number, y: number, w: number, r: () => number): Stroke[] {
  const a = line([x - 4, y + 2], [x + w + 4, y - 3], r, 0.03)
  return r() > 0.5 ? [a, line([x, y + 6], [x + w, y + 2], r, 0.03)] : [a]
}
