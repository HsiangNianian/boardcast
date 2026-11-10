// Small helpers shared by the page: motion preferences, easing, and hand-drawn lines as point lists.

export type Pt = [number, number]

export const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
export const clamp = (x: number, a = 0, b = 1) => Math.min(b, Math.max(a, x))
export const ease = (t: number) => 1 - Math.pow(1 - t, 3)
export const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, reduced ? 0 : ms))
export const $ = <T extends Element = HTMLElement>(sel: string, root: ParentNode = document) => root.querySelector(sel) as T
export const $$ = <T extends Element = HTMLElement>(sel: string, root: ParentNode = document) => [...root.querySelectorAll(sel)] as T[]

const NS = 'http://www.w3.org/2000/svg'
export function svgEl<K extends keyof SVGElementTagNameMap>(name: K, attrs: Record<string, string | number> = {}, parent?: Element) {
  const e = document.createElementNS(NS, name)
  for (const k in attrs) e.setAttribute(k, String(attrs[k]))
  parent?.appendChild(e)
  return e
}

/** An HTML element with a class and children, built as nodes rather than parsed from markup. */
export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = '', ...children: (string | Node)[]) {
  const e = document.createElement(tag)
  if (cls) e.className = cls
  e.append(...children)
  return e
}

/** The page's own constant SVG markup, such as a pictogram, parsed as XML into nodes for this document. */
export function svgParts(markup: string): Node[] {
  const doc = new DOMParser().parseFromString(`<svg xmlns="${NS}">${markup}</svg>`, 'image/svg+xml')
  return [...doc.documentElement.childNodes].map((n) => document.importNode(n, true))
}

export function rand(seed: number) {
  let s = seed % 2147483647 || 7
  return () => (s = (s * 16807) % 2147483647) / 2147483647
}

export const toD = (pts: Pt[]) => 'M' + pts.map((p) => p[0].toFixed(1) + ' ' + p[1].toFixed(1)).join(' L')
export const lengthOf = (pts: Pt[]) => pts.reduce((sum, p, i) => sum + (i ? Math.hypot(p[0] - pts[i - 1][0], p[1] - pts[i - 1][1]) : 0), 0)

/** A pen line through the points, with the slight wobble of a hand. */
export function hand(points: Pt[], wobble: number, seed: number): Pt[] {
  const r = rand(seed)
  const out: Pt[] = []
  for (let i = 0; i < points.length - 1; i++) {
    const [x0, y0] = points[i]
    const [x1, y1] = points[i + 1]
    const len = Math.hypot(x1 - x0, y1 - y0)
    const n = Math.max(2, Math.round(len / 14))
    const nx = -(y1 - y0) / (len || 1)
    const ny = (x1 - x0) / (len || 1)
    for (let k = i === 0 ? 0 : 1; k <= n; k++) {
      const t = k / n
      const j = (r() - 0.5) * wobble * Math.sin(Math.PI * t)
      out.push([x0 + (x1 - x0) * t + nx * j, y0 + (y1 - y0) * t + ny * j])
    }
  }
  return out
}

/** A loop drawn around something, a little more than once round, as a hand circles. */
export function loop(cx: number, cy: number, rx: number, ry: number, seed: number, turns = 1.12): Pt[] {
  const r = rand(seed)
  const out: Pt[] = []
  const n = 50
  for (let i = 0; i <= n * turns; i++) {
    const a = -2.4 + (i / n) * Math.PI * 2
    const k = 1 + (r() - 0.5) * 0.06 + (i / n) * 0.05
    out.push([cx + Math.cos(a) * rx * k, cy + Math.sin(a) * ry * k])
  }
  return out
}

export function star(cx: number, cy: number, R: number): Pt[] {
  const out: Pt[] = []
  for (let i = 0; i <= 5; i++) {
    const a = -Math.PI / 2 + (i * 4 * Math.PI) / 5
    out.push([cx + Math.cos(a) * R, cy + Math.sin(a) * R])
  }
  return out
}

/** Whether a closed shape holds a point, by counting crossings. */
export function encloses(shape: Pt[], p: Pt) {
  let inside = false
  for (let i = 0, j = shape.length - 1; i < shape.length; j = i++) {
    const a = shape[i]
    const b = shape[j]
    if (a[1] > p[1] !== b[1] > p[1] && p[0] < ((b[0] - a[0]) * (p[1] - a[1])) / (b[1] - a[1]) + a[0]) inside = !inside
  }
  return inside
}
