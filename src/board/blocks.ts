// 把 PDF 文字层里一个个零散的文字片段拼成「一块块内容」：先按行拼，再把挨着的行拼成段落。
// 纯函数，不依赖 pdf.js，方便测试。坐标都是板书坐标（页面左上角是原点，y 向下）。

import type { Area } from '../engine/regions.ts'

/** 文字层里的一个片段：x 是左边，y 是这一行字的顶，h 是字高 */
export interface Piece {
  text: string
  x: number
  y: number
  w: number
  h: number
}

const latin = (ch: string | undefined) => !!ch && /[A-Za-z0-9]/.test(ch)
/** 两段拼在一起：英文、数字之间补空格，中文直接接上 */
const join = (a: string, b: string) => (latin(a.at(-1)) && latin(b[0]) ? `${a} ${b}` : a + b)

interface Line {
  text: string
  x: number
  y: number
  w: number
  h: number
}

export function blocks(pieces: Piece[]): Area[] {
  const ps = pieces.filter((p) => p.text.trim() && p.w > 0 && p.h > 0).sort((a, b) => a.y - b.y || a.x - b.x)

  // —— 拼行：竖直方向差不多、水平方向挨着（隔太远就是另一栏） ——
  const lines: Line[] = []
  for (const p of ps) {
    const hit = lines.find((l) => Math.abs(l.y + l.h / 2 - (p.y + p.h / 2)) < Math.min(l.h, p.h) * 0.5 && p.x >= l.x - 1 && p.x - (l.x + l.w) < Math.max(l.h, p.h) * 1.5)
    if (hit) {
      hit.text = join(hit.text, p.text.trim())
      const x1 = Math.max(hit.x + hit.w, p.x + p.w)
      const y0 = Math.min(hit.y, p.y)
      hit.h = Math.max(hit.y + hit.h, p.y + p.h) - y0
      hit.y = y0
      hit.w = x1 - hit.x
    } else lines.push({ text: p.text.trim(), x: p.x, y: p.y, w: p.w, h: p.h })
  }
  lines.sort((a, b) => a.y - b.y || a.x - b.x)

  // —— 拼段落：下一行紧挨着上一行（行距不到一行字高）、左右有重叠、字号差不多 ——
  const out: (Area & { lh: number })[] = []
  for (const l of lines) {
    const hit = out.find(
      (b) =>
        l.y - (b.y + b.h) < Math.max(b.lh, l.h) * 0.9 &&
        l.y >= b.y + b.h - b.lh * 0.5 &&
        l.x < b.x + b.w &&
        l.x + l.w > b.x &&
        Math.max(b.lh, l.h) / Math.min(b.lh, l.h) < 1.3,
    )
    if (hit) {
      hit.text = join(hit.text, l.text)
      const x0 = Math.min(hit.x, l.x)
      hit.w = Math.max(hit.x + hit.w, l.x + l.w) - x0
      hit.x = x0
      hit.h = l.y + l.h - hit.y
    } else out.push({ text: l.text, x: l.x, y: l.y, w: l.w, h: l.h, lh: l.h })
  }
  return out.map(({ text, x, y, w, h }) => ({ text, x, y, w, h }))
}
