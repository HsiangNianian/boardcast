// 收尾：一整屏留白。画框顶边正中垂下一根干净的墨线，到画面中央一分为二，变成两道轻巧的短箭头，
// 指向下面并排的两个入口。主入口外面预先有一道极淡的铅笔虚线圈；鼠标真正移过去，
// 一支看不见的笔飞快地用朱砂红绕它画两圈（下沿压得重、上沿提得轻，像一条斜着的带子）；
// 移到另一个，这边的红圈被擦掉，去圈那一个。

import { type Piece, type Pt, clamp, drawPiece, easeInOut, fitCanvas, makePiece, reduceMotion, rng } from './ink/core'
import { arrow, cubic, dashes, loop } from './ink/shapes'
import { INK, RED } from './hero/board'

interface Ring {
  el: HTMLElement
  p: Piece
  state: 'in' | 'out'
  t0: number
}

export function initFinale() {
  const sec = document.querySelector<HTMLElement>('.finale')!
  const canvas = sec.querySelector<HTMLCanvasElement>('.cta-ink')!
  const items = [...sec.querySelectorAll<HTMLElement>('.cta-item')]
  const still = reduceMotion()
  let ctx = canvas.getContext('2d')!
  let W = 0
  let H = 0
  let rings: Ring[] = []
  let lead: Piece[] = []
  let pencil: Piece | null = null
  let shown = still ? 1 : 0
  let n = 0

  const rect = (el: HTMLElement) => {
    const s = sec.getBoundingClientRect()
    const r = el.querySelector('.cta-label')!.getBoundingClientRect()
    const all = el.getBoundingClientRect()
    return { cx: all.left - s.left + all.width / 2, cy: all.top - s.top + all.height / 2, w: all.width, h: all.height, top: r.top - s.top }
  }

  function ringPts(el: HTMLElement, id: string, turns: number) {
    const r = rect(el)
    const rr = rng(id)
    return loop(r.cx, r.cy, r.w / 2 + 30, r.h / 2 + 18, rr, { turns, start: Math.PI * 0.88 + (rr() - 0.5) * 0.3, tilt: -0.07, grow: 0.09, n: Math.round(70 * turns) })
  }

  function size() {
    W = sec.clientWidth
    H = sec.clientHeight
    ctx = fitCanvas(canvas, W, H).ctx
    const g = parseFloat(getComputedStyle(sec.querySelector('.finale-frame')!).top) || 16
    // 垂下来的线：画框顶边正中 → 画面中央分叉 → 两道短箭头落到两个入口头上
    const [a, b] = items.map(rect)
    const stacked = Math.abs(a.cy - b.cy) > 20
    const topP: Pt = [W / 2, g + 1]
    const split: Pt = [W / 2, stacked ? a.top - 90 : Math.min(H * 0.5, a.top - 70)]
    const r = rng('lead')
    const main = cubic(topP, [W / 2 + 3, topP[1] + (split[1] - topP[1]) * 0.4], [W / 2 - 3, split[1] - (split[1] - topP[1]) * 0.3], split, 30)
    const toA: Pt = [a.cx + (stacked ? 0 : 18), a.top - 16]
    const toB: Pt = [b.cx - (stacked ? 0 : 18), b.top - 16]
    const branch = (to: Pt) => cubic(split, [split[0], split[1] + 30], [to[0], to[1] - 46], to, 24)
    lead = [
      makePiece('lead', INK, [main], 2.8, 0.25, { wob: 0.6 }),
      makePiece('leadA', INK, arrow(branch(toA), 10, r), 2.2, 0.25, { wob: 0.6 }),
      makePiece('leadB', INK, stacked ? [] : arrow(branch(toB), 10, r), 2.2, 0.25, { wob: 0.6 }),
    ]
    // 主入口外面一道极淡的铅笔虚线圈
    pencil = makePiece('pencil', '#7d8086', dashes(ringPts(items[0], 'pencil', 1.06), 7, 6), 1.5, 0.2, { alpha: 0.7, gap: 0 })
    const on = rings.find((x) => x.state === 'in')
    rings = []
    if (on) circle(on.el, true)
    draw(performance.now())
  }

  function make(el: HTMLElement) {
    const r = rect(el)
    const id = `cta${n++}`
    const pts = ringPts(el, id, 2.06)
    const ry = r.h / 2 + 18
    // 压感：下沿重、上沿轻
    const press = (_t: number, p: Pt) => 0.3 + 0.7 * clamp(((p[1] - r.cy) / ry + 1) / 2) ** 1.3
    return makePiece(id, RED, [pts], 5, 0.3, { press, wob: 0.5 })
  }

  function circle(el: HTMLElement, instant = false) {
    const now = performance.now()
    if (rings.some((r) => r.el === el && r.state === 'in')) return
    for (const r of rings)
      if (r.state === 'in') {
        r.state = 'out'
        r.t0 = now
      }
    rings.push({ el, p: make(el), state: 'in', t0: instant || still ? now - 1e4 : now })
    wake()
  }

  for (const el of items) {
    el.addEventListener('pointerenter', () => circle(el))
    el.addEventListener('focus', () => circle(el))
  }

  // 滚进来的时候，那根线跟着滚动垂下来；画过就不收回
  addEventListener(
    'scroll',
    () => {
      const r = sec.getBoundingClientRect()
      const p = clamp((innerHeight - r.top) / (innerHeight * 0.8))
      if (p > shown) {
        shown = p
        wake()
      }
    },
    { passive: true },
  )

  function draw(now: number) {
    ctx.clearRect(0, 0, W, H)
    // 线：0 → 0.6 主干，0.6 → 0.9 两道短箭头，最后淡淡的铅笔圈
    drawPiece(ctx, lead[0], clamp(shown / 0.6))
    drawPiece(ctx, lead[1], clamp((shown - 0.6) / 0.3))
    drawPiece(ctx, lead[2], clamp((shown - 0.62) / 0.3))
    if (pencil) drawPiece(ctx, pencil, clamp((shown - 0.82) / 0.18))
    let busy = false
    rings = rings.filter((r) => !(r.state === 'out' && now - r.t0 > 320))
    for (const r of rings) {
      if (r.state === 'in') {
        const f = easeInOut(clamp((now - r.t0) / 560))
        drawPiece(ctx, r.p, f)
        busy ||= f < 1
      } else {
        // 擦掉：从起笔那头开始追着笔尾擦
        drawPiece(ctx, r.p, 1, 1, clamp((now - r.t0) / 300) ** 1.6)
        busy = true
      }
    }
    return busy
  }

  let running = false
  function wake() {
    if (running) return
    running = true
    requestAnimationFrame(frame)
  }
  function frame(now: number) {
    if (draw(now)) requestAnimationFrame(frame)
    else running = false
  }

  new ResizeObserver(size).observe(sec)
  size()
}
