// 在图片里找出一块块内容（一行行字、一段段字）的位置，不用模型：
// 本地的小视觉模型读字很准，但给的框经常偏好几十像素，圈一段字会圈到上一行去；位置这件事交给普通的图像处理。
//
// 1. 背景色取最常见的亮度，和它差得多的像素算「墨」（白板、黑板、截图都适用）；
// 2. 很长的横线、竖线（表格线、流程图的框和箭头）先抹掉，免得把整张图连成一块；
// 3. 墨按小格子粗化，横向把字和字之间的空隙补上，连通的一片就是一行（或一个词）；
// 4. 行再按「上下挨着、左右有重叠、字号差不多」拼成段落（和 PDF 文字层用的是同一套规则）。
// 纯函数，输入是 ImageData 那样的像素数组，方便测试。

import type { Area } from '../engine/regions.ts'
import { blocks } from './blocks.ts'

export interface Pixels {
  width: number
  height: number
  /** RGBA，一个像素四个字节 */
  data: Uint8ClampedArray
}

/** 粗化用的格子边长（像素） */
const CELL = 3

export function segment(img: Pixels): Area[] {
  const { width: W, height: H, data } = img
  const lum = new Uint8Array(W * H)
  const hist = new Uint32Array(256)
  for (let i = 0; i < W * H; i++) {
    const a = data[i * 4 + 3] / 255
    // 透明的地方当白底
    const v = Math.round((0.299 * data[i * 4] + 0.587 * data[i * 4 + 1] + 0.114 * data[i * 4 + 2]) * a + 255 * (1 - a))
    lum[i] = v
    hist[v]++
  }
  let bg = 0
  for (let v = 1; v < 256; v++) if (hist[v] > hist[bg]) bg = v
  const ink = new Uint8Array(W * H)
  for (let i = 0; i < W * H; i++) ink[i] = Math.abs(lum[i] - bg) > 48 ? 1 : 0

  // —— 抹掉长直线：横向或竖向连续很长的一段墨（表格线、框、箭头）。
  // 手画的线会抖，按「上下（左右）各放宽 WOBBLE 像素」来看是不是连着的，抹的时候把这条带子里的墨都抹掉 ——
  const long = Math.max(40, Math.round(Math.min(W, H) * 0.08))
  const WOBBLE = 2
  const line = new Uint8Array(W * H)
  const near = (x: number, y: number, dx: number, dy: number) => {
    for (let k = -WOBBLE; k <= WOBBLE; k++) {
      const xx = x + dx * k
      const yy = y + dy * k
      if (xx >= 0 && xx < W && yy >= 0 && yy < H && ink[yy * W + xx]) return true
    }
    return false
  }
  for (let y = 0; y < H; y++) {
    let start = -1
    for (let x = 0; x <= W; x++) {
      const on = x < W && near(x, y, 0, 1)
      if (on && start < 0) start = x
      if (!on && start >= 0) {
        if (x - start >= long) for (let k = start; k < x; k++) line[y * W + k] = 1
        start = -1
      }
    }
  }
  for (let x = 0; x < W; x++) {
    let start = -1
    for (let y = 0; y <= H; y++) {
      const on = y < H && near(x, y, 1, 0)
      if (on && start < 0) start = y
      if (!on && start >= 0) {
        if (y - start >= long) for (let k = start; k < y; k++) line[k * W + x] = 1
        start = -1
      }
    }
  }
  // 一行字自己也可能横着连成长长一段（放宽以后笔画挨得近）：只有这一带又细（不超过 2*WOBBLE+3 像素高）才算线
  const thin = (i: number, dx: number, dy: number) => {
    const x = i % W
    const y = (i - x) / W
    let n = 0
    for (let k = -(WOBBLE + 4); k <= WOBBLE + 4; k++) {
      const xx = x + dx * k
      const yy = y + dy * k
      if (xx >= 0 && xx < W && yy >= 0 && yy < H && ink[yy * W + xx]) n++
    }
    return n <= WOBBLE * 2 + 3
  }
  const erase: number[] = []
  for (let i = 0; i < W * H; i++) if (line[i] && ink[i] && (thin(i, 0, 1) || thin(i, 1, 0))) erase.push(i)
  for (const i of erase) ink[i] = 0

  // —— 粗化成格子，横向补空隙 ——
  const gw = Math.ceil(W / CELL)
  const gh = Math.ceil(H / CELL)
  const grid = new Uint8Array(gw * gh)
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if (ink[y * W + x]) grid[Math.floor(y / CELL) * gw + Math.floor(x / CELL)] = 1
  // 字和字之间的空隙一般不到半个字宽；按一行字高的大概值补（截图正文十几像素高，补 4 格 = 12 像素）
  const gap = 4
  const filled = grid.slice()
  for (let y = 0; y < gh; y++) {
    let last = -Infinity
    for (let x = 0; x < gw; x++) {
      if (!grid[y * gw + x]) continue
      if (x - last > 1 && x - last <= gap + 1) for (let k = last + 1; k < x; k++) filled[y * gw + k] = 1
      last = x
    }
  }

  // —— 连通块 = 一行（或一个词） ——
  const seen = new Uint8Array(gw * gh)
  const lines: { x: number; y: number; w: number; h: number; n: number }[] = []
  const stack: number[] = []
  for (let s = 0; s < gw * gh; s++) {
    if (!filled[s] || seen[s]) continue
    let x0 = gw
    let y0 = gh
    let x1 = 0
    let y1 = 0
    let n = 0
    stack.push(s)
    seen[s] = 1
    while (stack.length) {
      const c = stack.pop()!
      const cx = c % gw
      const cy = (c - cx) / gw
      x0 = Math.min(x0, cx)
      x1 = Math.max(x1, cx)
      y0 = Math.min(y0, cy)
      y1 = Math.max(y1, cy)
      if (grid[c]) n++
      for (const d of [c - 1, c + 1, c - gw, c + gw]) {
        if (d < 0 || d >= gw * gh || seen[d] || !filled[d]) continue
        if ((d === c - 1 && cx === 0) || (d === c + 1 && cx === gw - 1)) continue
        seen[d] = 1
        stack.push(d)
      }
    }
    lines.push({ x: x0 * CELL, y: y0 * CELL, w: (x1 - x0 + 1) * CELL, h: (y1 - y0 + 1) * CELL, n })
  }
  // 太小、太细的是噪点、标点碎屑、没抹干净的线头
  const kept = lines.filter((l) => l.h >= CELL * 2 && l.w > CELL * 2 && l.n >= 3)
  const out = blocks(kept.map((l) => ({ text: '·', x: l.x, y: l.y, w: Math.min(l.w, W - l.x), h: Math.min(l.h, H - l.y) }))).map((b) => ({ ...b, text: '' }))
  // 整个落在别的块里面的小块（一个字的一笔被单独切出来）不要
  const within = (a: Area, b: Area) => a !== b && a.x >= b.x - 2 && a.y >= b.y - 2 && a.x + a.w <= b.x + b.w + 2 && a.y + a.h <= b.y + b.h + 2
  return out.filter((a) => !out.some((b) => within(a, b)))
}
