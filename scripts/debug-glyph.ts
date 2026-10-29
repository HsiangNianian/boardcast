// 调试：把一个字的每一笔过一遍笔，看看每一笔画出来有多宽
//   node scripts/debug-glyph.ts 监
import { readFileSync } from 'node:fs'
import { getStroke } from 'perfect-freehand'
import { ink, place, rng } from '../src/ink/pen.ts'

const ch = process.argv[2] ?? '监'
const medians: number[][][] = JSON.parse(readFileSync(new URL(`../public/hanzi/${ch}.json`, import.meta.url), 'utf8'))
const r = rng(ch)

/** 轮廓多边形的面积 ÷ 长度 ≈ 平均笔宽 */
function width(points: number[][], len: number): number {
  let a = 0
  for (let i = 0; i < points.length; i++) {
    const [x0, y0] = points[i]
    const [x1, y1] = points[(i + 1) % points.length]
    a += x0 * y1 - x1 * y0
  }
  return Math.abs(a) / 2 / Math.max(len, 1e-6)
}

medians.forEach((m, i) => {
  const s = place(
    m.map((p) => [p[0] / 1024, (900 - p[1]) / 1024] as [number, number]),
    0,
    0,
    20,
    r,
  )
  const out = ink(s, 2.6, r)
  const raw = getStroke(s, { size: 2.6, last: true, simulatePressure: true, thinning: 0.32, streamline: 0.08 })
  console.log(i, 'len', out.length.toFixed(1), 'outline pts', raw.length, 'avg width', width(raw, out.length).toFixed(2))
})
