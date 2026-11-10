// A pen that writes in stroke order. The letters come from EMS Tech, a single-stroke font: each glyph is a
// few centerlines, so the pen can follow them the way a hand would rather than revealing filled letters.

import font from './emstech.json'
import { type Pt, lengthOf, rand, reduced, svgEl, toD } from './util'

const FONT = font as unknown as { em: number; g: Record<string, [string, number]> }

export interface Word {
  text: string
  x: number
  w: number
}

/** A line of text as pen strokes in page units, with where each word sits. */
export function textStrokes(str: string, x: number, y: number, size: number, seed = 1) {
  const k = size / FONT.em
  const r = rand(seed)
  const strokes: Pt[][] = []
  const words: Word[] = []
  let cx = x
  let wordStart = x
  let word = ''
  for (const ch of str + ' ') {
    if (ch === ' ' && word) {
      words.push({ text: word, x: wordStart, w: cx - wordStart })
      word = ''
    }
    const g = FONT.g[ch] ?? FONT.g[' ']
    if (ch !== ' ') {
      if (!word) wordStart = cx
      word += ch
    }
    // The glyphs only use M and L: every M starts a new stroke.
    for (const part of g[0].split('M').filter((t) => t.trim())) {
      const n = part.replace(/L/g, ' ').trim().split(/\s+/).map(Number)
      const pts: Pt[] = []
      for (let i = 0; i + 1 < n.length; i += 2) {
        pts.push([cx + n[i] * k + (r() - 0.5) * size * 0.012, y - n[i + 1] * k + (r() - 0.5) * size * 0.012])
      }
      if (pts.length > 1) strokes.push(pts)
    }
    cx += g[1] * k * 0.97
  }
  return { strokes, words, width: cx - x }
}

export interface PenOptions {
  cls?: string
  /** Page units per millisecond. */
  speed?: number
  /** The pause between strokes, as a distance the pen travels in the air. */
  lift?: number
}

/** Draw strokes one after another at the pen's speed, with a visible tip. Resolves to the group drawn. */
export function penDraw(svg: SVGSVGElement, strokes: Pt[][], { cls = 'w', speed = 1.6, lift = 40 }: PenOptions = {}): Promise<SVGGElement> {
  const g = svgEl('g', {}, svg)
  const paths = strokes.map((pts) => ({ p: svgEl('path', { class: cls }, g), pts, len: lengthOf(pts), full: false }))
  if (reduced) {
    paths.forEach((x) => x.p.setAttribute('d', toD(x.pts)))
    return Promise.resolve(g)
  }
  const tip = svgEl('g', {}, svg)
  svgEl('circle', { class: 'tip', r: 6 }, tip)
  svgEl('circle', { class: 'tip-dot', r: 1.6 }, tip)
  const total = paths.reduce((s, x) => s + x.len + lift, 0)
  return new Promise((done) => {
    const t0 = performance.now()
    function frame(now: number) {
      let dist = (now - t0) * speed
      let at: Pt | null = null
      for (const x of paths) {
        if (dist >= x.len + lift) {
          if (!x.full) {
            x.p.setAttribute('d', toD(x.pts))
            x.full = true
          }
          dist -= x.len + lift
          continue
        }
        if (dist <= 0) break
        // Partway through this stroke: draw up to the pen.
        const out: Pt[] = [x.pts[0]]
        let left = Math.min(dist, x.len)
        for (let i = 1; i < x.pts.length && left > 0; i++) {
          const a = x.pts[i - 1]
          const b = x.pts[i]
          const seg = Math.hypot(b[0] - a[0], b[1] - a[1])
          if (left >= seg) {
            out.push(b)
            left -= seg
          } else {
            const t = left / seg
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
            left = 0
          }
        }
        x.p.setAttribute('d', toD(out))
        at = out[out.length - 1]
        break
      }
      if (at) tip.setAttribute('transform', `translate(${at[0]} ${at[1]})`)
      if ((now - t0) * speed < total) requestAnimationFrame(frame)
      else {
        tip.remove()
        paths.forEach((x) => x.p.setAttribute('d', toD(x.pts)))
        done(g)
      }
    }
    requestAnimationFrame(frame)
  })
}
