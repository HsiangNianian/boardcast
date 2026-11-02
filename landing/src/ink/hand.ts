// 一段字落到纸上：位置、字号，加上随手写的变形。
// messy 从 0（工整）到 1（狂草），同时控制：字的歪斜、大小错落、基线起伏、整体右倾，
// 每一笔的错位和转动、折角磨圆、笔画出头，以及汉字的连笔。
// 随机数按「第几个字、第几笔」固定抽好，推杆拖动时只是幅度在变，所以字是连续变形的。

import { DEFAULT_MESSY } from '../../../src/ink/defaults'
import { type Pt, type RawMark, type Stroke, clamp, densify, dist, rng, smooth } from './core'
import { glyph } from './glyphs'

interface StrokeRand {
  sx: number
  sy: number
  sr: number
  o0: number
  o1: number
  lig: number
  f1: number
  ph: number
}

/** 笔画两头随机出头或缩回一点 */
function overshoot(s: Stroke, amount: number, a: number, b: number): Stroke {
  if (s.length < 2 || amount <= 0) return s
  const ext = (p: Pt, q: Pt, k: number): Pt => [p[0] + (p[0] - q[0]) * k, p[1] + (p[1] - q[1]) * k]
  const out = s.slice()
  out[0] = ext(s[0], s[1], (a - 0.4) * amount)
  out[out.length - 1] = ext(s[s.length - 1], s[s.length - 2], (b - 0.3) * amount)
  return out
}

/** 两笔之间带出来的细丝：微微弯一下 */
function thread(a: Pt, b: Pt, size: number, r: number): Stroke {
  const mx = (a[0] + b[0]) / 2 + (r - 0.5) * size * 0.12
  const my = (a[1] + b[1]) / 2 - size * 0.04
  return [a, [mx, my], b]
}

export interface Written {
  marks: RawMark[]
  w: number
}

export function write(text: string, x: number, y: number, size: number, seed: string, messy = DEFAULT_MESSY): Written {
  const m = clamp(messy)
  const marks: RawMark[] = []
  let cx = x
  let i = 0
  let prev: Pt | null = null
  for (const ch of text) {
    const g = glyph(ch)
    const r = rng(`${seed}#${i++}`)
    const rot0 = r()
    const k0 = r()
    const dy0 = r()
    const adv0 = r()
    const lig0 = r()
    if (!g) {
      cx += size * 0.6
      prev = null
      continue
    }
    const rs: StrokeRand[] = g.strokes.map(() => ({ sx: r(), sy: r(), sr: r(), o0: r(), o1: r(), lig: r(), f1: 0.5 + r() * 2, ph: r() * Math.PI * 2 }))

    // 字内部的笔画扭曲到 0.78 以后几乎不再加：字整体可以更歪、更错落，但笔画不能叠死成一团黑
    const mi = m < 0.78 ? m : 0.78 + (m - 0.78) * 0.2
    const rot = (rot0 - 0.5) * (0.05 + 0.24 * m)
    const k = size * (1 + (k0 - 0.5) * (0.08 + 0.34 * m))
    const dy = (dy0 - 0.5) * size * (0.04 + 0.2 * m)
    // 右倾：越往上越往右
    const slant = 0.04 + 0.3 * m
    const cos = Math.cos(rot)
    const sin = Math.sin(rot)
    const placed = g.strokes.map((stroke, j) => {
      const R = rs[j]
      const sx = (R.sx - 0.5) * 0.06 * mi
      const sy = (R.sy - 0.5) * 0.06 * mi
      const sr = (R.sr - 0.5) * 0.26 * mi
      let ccx = 0
      let ccy = 0
      for (const [u, v] of stroke) {
        ccx += u / stroke.length
        ccy += v / stroke.length
      }
      const cs = Math.cos(sr)
      const sn = Math.sin(sr)
      let local: Stroke = stroke.map(([u, v]) => {
        const du = u - ccx
        const dv = v - ccy
        return [ccx + du * cs - dv * sn + sx, ccy + du * sn + dv * cs + sy]
      })
      local = overshoot(local, 0.24 * mi, R.o0, R.o1)
      if (mi > 0.01) local = smooth(densify(local, 0.06), Math.min(0.5, 0.6 * mi), 4)
      return local.map(([u, v]): Pt => {
        const px = (u - 0.5 + (0.5 - v) * slant) * k
        const py = (v - 0.5) * k
        return [cx + size * 0.5 + px * cos - py * sin, y + size * 0.5 + dy + px * sin + py * cos]
      })
    })

    // 字与字之间：很潦草的时候，上一个字的收笔牵到这个字的起笔
    if (g.han && prev && placed[0]?.length && m > 0.62) {
      const a = clamp(((m - 0.62) * 2.4 - lig0) / 0.18)
      if (a > 0 && dist(prev, placed[0][0]) < size * 1.15) marks.push({ pts: thread(prev, placed[0][0], size, rot0), thin: a * 0.85, f1: 1, ph: 0 })
    }
    for (let j = 0; j < placed.length; j++) {
      const R = rs[j]
      marks.push({ pts: placed[j], f1: R.f1, ph: R.ph, latin: g.latin })
      // 汉字连笔：越潦草，上一笔的收笔越容易带出一根细丝连到下一笔的起笔
      const next = placed[j + 1]
      if (g.han && next && m > 0.42) {
        const a = clamp(((m - 0.42) * 1.7 - R.lig) / 0.14)
        const p = placed[j][placed[j].length - 1]
        if (a > 0 && dist(p, next[0]) < size * 0.55) marks.push({ pts: thread(p, next[0], size, R.sx), thin: a * 0.75, f1: 1, ph: 0 })
      }
    }
    const last = placed[placed.length - 1]
    prev = g.han && last?.length ? last[last.length - 1] : null
    // 越潦草，字距越挤、越不均匀
    cx += g.advance * size * (0.96 - 0.1 * m + adv0 * (0.06 + 0.24 * m)) + size * 0.04
  }
  return { marks, w: cx - x }
}

/** 名义宽度（工整时），排版用 */
export function measure(text: string, size: number): number {
  let w = 0
  for (const ch of text) {
    const g = glyph(ch)
    w += g ? g.advance * size * 0.99 + size * 0.04 : size * 0.6
  }
  return w
}

export const penFor = (size: number) => Math.max(1.6, size * 0.1)
