import type { Piece } from './compose.ts'

export const ERASE_MS = 520

/** Styling/hover changes keep their ink; moved, rewritten or removed strokes leave a temporary copy. */
export function changedInk(before: Piece[], after: Piece[]): Piece[] {
  const next = new Map(after.map((p) => [p.key, p]))
  return before.filter((p) => {
    if (p.under) return false
    const q = next.get(p.key)
    return !q || p.strokes.length !== q.strokes.length || p.strokes.some((s, i) => s.d !== q.strokes[i].d)
  })
}

/** Sweep overlapping bands across the actual ink bounds; only the mask moves, no eraser object. */
export function eraseSweep(piece: Piece) {
  const points = piece.strokes.flatMap((s) => [...s.center.matchAll(/([\d.+-]+)\s+([\d.+-]+)/g)].map((m) => [Number(m[1]), Number(m[2])]))
  if (!points.length) return null
  const bounds = points.reduce(([x0, y0, x1, y1], [x, y]) => [Math.min(x0, x), Math.min(y0, y), Math.max(x1, x), Math.max(y1, y)], [Infinity, Infinity, -Infinity, -Infinity])
  const x = bounds[0] - 18, y = bounds[1] - 18
  const w = bounds[2] - x + 18, h = bounds[3] - y + 18
  const count = Math.max(1, Math.ceil(h / 26))
  const step = h / count
  const path: [number, number][] = []
  for (let i = 0; i < count; i++) {
    const yy = y + step * (i + 0.5)
    path.push([i % 2 ? x + w : x, yy], [i % 2 ? x : x + w, yy])
  }
  const length = path.reduce((n, p, i) => n + (i ? Math.hypot(p[0] - path[i - 1][0], p[1] - path[i - 1][1]) : 0), 0)
  return { x, y, w, h, d: path.map(([x, y], i) => `${i ? 'L' : 'M'}${x} ${y}`).join(' '), length }
}
