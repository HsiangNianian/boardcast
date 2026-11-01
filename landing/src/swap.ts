// 白板 ↔ 黑板：同一块板书画两遍——温润白板上浓黑锐利的马克笔，和深色黑板上带颗粒、带粉尘的粉笔。
// 鼠标移进来变成一把宽大的板擦；按住横着抹过去，擦痕经过的地方材质整个反转。
// 从白板这边下手就擦出黑板，从黑板那边下手就擦回白板；中间的把手可以整片拉过去。

import { DEFAULT_MESSY } from '../../src/ink/defaults'
import { type Piece, type Pt, type Stroke, clamp, drawPiece, easeOut, fitCanvas, makePiece, rng } from './ink/core'
import { measure, penFor, write } from './ink/hand'
import { arrow, bow, box, line, loop, strike } from './ink/shapes'

type Role = 'ink' | 'red' | 'blue'
const MARKER: Record<Role, string> = { ink: '#1b1d22', red: '#d02e2e', blue: '#1f56c4' }
const CHALK: Record<Role, string> = { ink: '#f2f1ea', red: '#ff9c8a', blue: '#9cc9f5' }
const WHITE = '#fbfbf8'
const SLATE = '#22302a'

interface Part {
  role: Role
  p: Piece
}

/** 板上写的东西（设计尺寸 1600 × 900，画的时候按屏幕缩放、居中） */
/** 横屏按 1600 × 900 排；竖屏（portrait）把几块拆开竖着叠，设计尺寸 620 × 1420 */
export const PORTRAIT = { w: 620, h: 1560 }

function content(k: number, ox: number, oy: number, portrait = false): Part[] {
  const m = DEFAULT_MESSY
  // 当前这一块的摆法：设计坐标 (u, v) 先以 (u0, v0) 为原点缩放 bs，再挪到 (x0, y0)
  let tf = { u0: 0, v0: 0, x0: 0, y0: 0, bs: 1 }
  const blockAt = (u0: number, v0: number, x0: number, y0: number, bs = 1) => {
    tf = portrait ? { u0, v0, x0, y0, bs } : { u0: 0, v0: 0, x0: 0, y0: 0, bs: 1 }
  }
  const X = (u: number) => ox + (tf.x0 + (u - tf.u0) * tf.bs) * k
  const Y = (v: number) => oy + (tf.y0 + (v - tf.v0) * tf.bs) * k
  const S = (s: number) => s * k * tf.bs
  const r = rng('swap')
  const out: Part[] = []
  const T = (id: string, s: string, u: number, v: number, size: number, role: Role = 'ink', pen = 1) => {
    const w = write(s, X(u), Y(v), S(size), id, m)
    out.push({ role, p: makePiece(id, '#000', w.marks, penFor(S(size)) * pen, m) })
    return w.w
  }
  const M = (id: string, strokes: Stroke[], pen: number, role: Role = 'ink') => out.push({ role, p: makePiece(id, '#000', strokes, S(pen), m) })

  blockAt(100, 60, 0, 30)
  const tw = T('title', '这场会的结论', 130, 86, 56, 'ink', 1.1)
  M('t-u1', [line([X(122), Y(156)], [X(130) + tw + S(10), Y(160)], r, 0.01)], 4.4)
  M('t-u2', [line([X(136), Y(168)], [X(120) + tw, Y(172)], r, 0.01)], 3.4)

  const a = T('local', '全部在本机运行', 140, 206, 40)
  T('local*', '★', 140 + a / S(1) + 14, 192, 34, 'red')
  T('l1', '↳ 语音在本机识别', 170, 272, 28)
  T('l2', '↳ 板书由本机小模型写', 170, 318, 28)
  const l3 = T('l3', '↳ 不上传音频', 170, 364, 28)
  M('l3-u', [line([X(196), Y(400)], [X(170) + l3 + S(8), Y(398)], r, 0.01)], 3, 'red')
  const up = T('up', '上传录音', 200, 420, 24)
  M('up-x', [strike(X(200), Y(433), up, S(2), r), strike(X(204), Y(440), up - S(6), -S(2), r)], 2.6, 'red')

  // 流程
  blockAt(120, 470, 14, 1100, 0.62)
  const flow = ['说话', '转写', '小模型', '排版', '板书']
  let fx = 140
  const fy = 528
  const boxes: [number, number, number][] = []
  flow.forEach((s, i) => {
    const w = measure(s, S(30)) / S(1)
    T(`f${i}`, s, fx + 20, fy, 30)
    M(`fb${i}`, box(X(fx), Y(fy - 16), S(w + 40), S(62), r, m), 2.6)
    boxes.push([fx, fx + w + 40, fy])
    fx += w + 40 + 70
  })
  for (let i = 0; i < boxes.length - 1; i++) {
    const a0: Pt = [X(boxes[i][1] + 8), Y(fy + 15)]
    const b0: Pt = [X(boxes[i + 1][0] - 8), Y(fy + 15)]
    M(`fa${i}`, arrow(bow(a0, b0, S(2), 10), S(10), r), 2.4)
  }
  T('ops', '只发图操作', (boxes[2][1] + boxes[3][0]) / 2 - 60, fy - 54, 20, 'blue')
  if (!portrait) M('l3-arc', arrow(bow([X(170) + l3 + S(26), Y(380)], [X((boxes[2][0] + boxes[2][1]) / 2), Y(fy - 24)], -S(40), 24), S(13), r), 2.6, 'blue')

  // 右边：三支笔
  blockAt(1090, 90, 20, 520)
  const pw = T('pens', '三支笔', 1110, 104, 40)
  M('pens-u', [line([X(1104), Y(154)], [X(1110) + pw + S(10), Y(157)], r, 0.01)], 3)
  T('p1', '手写 · 马克笔', 1132, 182, 28)
  const p2 = T('p2', '黑板 · 粉笔', 1132, 228, 28)
  M('p2-o', [loop(X(1132) + p2 / 2, Y(242), p2 / 2 + S(22), S(26), r, { turns: 1.18, start: 3.2 })], 2.8, 'red')
  T('p3', '铅笔 · 可编辑', 1132, 274, 28)
  const f = T('p50', 'p50 ≈ 0.7 s', 1140, 376, 34)
  M('p50-o', [loop(X(1140) + f / 2, Y(394), f / 2 + S(28), S(40), r, { turns: 1.08, start: 3.6 })], 2.6, 'blue')
  T('p50s', '4B 模型 · 6 GB 显卡', 1150, 446, 18, 'blue')
  const os = T('os', 'Linux · Windows · macOS', 1110, 560, 24)
  M('os-b', box(X(1094), Y(546), os + S(32), S(52), r, m), 2.2)
  if (!portrait) M('os-a', arrow(bow([X(boxes[4][1] + 10), Y(fy + 10)], [X(1088), Y(572)], S(10), 14), S(10), r), 2.2)

  // 底下：一个问题
  blockAt(600, 680, 14, 1230, 0.66)
  const q = T('q', '云端大模型？', 620, 700, 30, 'red')
  M('q-a', arrow(bow([X(620) + q + S(12), Y(716)], [X(880), Y(716)], -S(8), 12), S(10), r), 2.4)
  T('qa', '可以接，发出去什么先说清楚', 892, 704, 24, 'blue')
  blockAt(1360, 620, 440, 1330)
  T('bigstar', '★', 1430, 640, 54, 'red')
  T('ok', '✓ 定了', 1380, 724, 30, 'red')
  return out
}

export function initSwap() {
  const section = document.querySelector<HTMLElement>('.swap')!
  const board = section.querySelector<HTMLElement>('.swap-board')!
  const view = board.querySelector<HTMLCanvasElement>('.swap-view')!
  const dustC = board.querySelector<HTMLCanvasElement>('.swap-dust')!
  const handle = board.querySelector<HTMLButtonElement>('.swap-handle')!
  const eraser = board.querySelector<HTMLElement>('.eraser')!

  const white = document.createElement('canvas')
  const chalk = document.createElement('canvas')
  const tmp = document.createElement('canvas')
  const mask = document.createElement('canvas')
  const stamp = document.createElement('canvas')
  const MS = 0.5 // 遮罩用半分辨率：边缘自然就软一点
  let W = 0
  let H = 0
  let dpr = 1
  let vctx = view.getContext('2d')!
  let dctx = dustC.getContext('2d')!
  const mctx = mask.getContext('2d', { willReadFrequently: true })!
  let split = 0.5
  let dirty = true

  function noise(size: number, density: number, max: number) {
    const c = document.createElement('canvas')
    c.width = c.height = size
    const x = c.getContext('2d')!
    const img = x.createImageData(size, size)
    for (let i = 0; i < size * size; i++) {
      const on = Math.random() < density
      img.data[i * 4 + 3] = on ? Math.random() * max : 0
    }
    x.putImageData(img, 0, 0)
    return c
  }

  function build() {
    W = board.clientWidth
    H = board.clientHeight
    ;({ dpr } = fitCanvas(view, W, H))
    vctx = view.getContext('2d')!
    dctx = fitCanvas(dustC, W, H).ctx
    const wc = fitCanvas(white, W, H).ctx
    const cc = fitCanvas(chalk, W, H).ctx
    fitCanvas(tmp, W, H)
    mask.width = Math.ceil(W * MS)
    mask.height = Math.ceil(H * MS)

    const portrait = W / H < 0.9
    const D = portrait ? PORTRAIT : { w: 1600, h: 900 }
    const k = Math.min(W / D.w, H / D.h) * 0.96
    const parts = content(k, (W - D.w * k) / 2, portrait ? H * 0.025 : (H - D.h * k) / 2 + 10, portrait)

    // —— 白板：浅暖白，斜着一道反光，还有很淡的旧笔迹残影 ——
    wc.fillStyle = WHITE
    wc.fillRect(0, 0, W, H)
    const sheen = wc.createLinearGradient(0, 0, W, H)
    sheen.addColorStop(0, 'rgba(255,255,255,0)')
    sheen.addColorStop(0.38, 'rgba(255,255,255,0.9)')
    sheen.addColorStop(0.46, 'rgba(236,236,230,0.35)')
    sheen.addColorStop(1, 'rgba(228,228,220,0.5)')
    wc.fillStyle = sheen
    wc.fillRect(0, 0, W, H)
    const gr = rng('ghost')
    for (let i = 0; i < 14; i++) {
      const x = gr() * W
      const y = gr() * H
      const p = makePiece(`g${i}`, '#8a8d92', [bow([x, y], [x + 80 + gr() * 200, y + (gr() - 0.5) * 40], (gr() - 0.5) * 30)], 3 + gr() * 4, DEFAULT_MESSY, { alpha: 0.06 })
      drawPiece(wc, p)
    }
    wc.shadowBlur = 0.8
    for (const { role, p } of parts) {
      p.color = MARKER[role]
      wc.shadowColor = MARKER[role]
      drawPiece(wc, p, 1, 0.96)
    }
    wc.shadowBlur = 0

    // —— 黑板：深绿灰，粉尘雾、旧擦痕；字是粉笔，带颗粒和一圈微光 ——
    cc.fillStyle = SLATE
    cc.fillRect(0, 0, W, H)
    const vig = cc.createRadialGradient(W / 2, H / 2, Math.min(W, H) * 0.2, W / 2, H / 2, Math.max(W, H) * 0.75)
    vig.addColorStop(0, 'rgba(255,255,255,0.03)')
    vig.addColorStop(1, 'rgba(0,0,0,0.28)')
    cc.fillStyle = vig
    cc.fillRect(0, 0, W, H)
    const sr = rng('smudge')
    cc.save()
    cc.filter = 'blur(18px)'
    for (let i = 0; i < 16; i++) {
      cc.strokeStyle = `rgba(235,240,232,${0.025 + sr() * 0.035})`
      cc.lineWidth = 50 + sr() * 120
      cc.lineCap = 'round'
      cc.beginPath()
      const x = sr() * W
      const y = sr() * H
      cc.moveTo(x, y)
      cc.bezierCurveTo(x + 200 * (sr() - 0.2), y + 80 * (sr() - 0.5), x + 380 * sr(), y + 60 * (sr() - 0.5), x + 260 + sr() * 300, y + (sr() - 0.5) * 80)
      cc.stroke()
    }
    cc.restore()
    const grain = noise(180, 0.5, 30)
    cc.fillStyle = cc.createPattern(grain, 'repeat')!
    cc.globalCompositeOperation = 'lighter'
    cc.fillRect(0, 0, W, H)
    cc.globalCompositeOperation = 'source-over'

    // 粉笔字先单独画，挖掉颗粒，再加一层微光
    const sc = document.createElement('canvas')
    const sx = fitCanvas(sc, W, H).ctx
    for (const { role, p } of parts) {
      p.color = CHALK[role]
      for (const mk of p.marks) {
        mk.size *= 1.12
        mk.path = undefined
      }
      drawPiece(sx, p, 1, 0.92)
    }
    sx.setTransform(1, 0, 0, 1, 0, 0)
    sx.globalCompositeOperation = 'destination-out'
    sx.fillStyle = sx.createPattern(noise(160, 0.55, 230), 'repeat')!
    sx.fillRect(0, 0, sc.width, sc.height)
    const streak = noise(120, 0.25, 160)
    const pat = sx.createPattern(streak, 'repeat')!
    pat.setTransform(new DOMMatrix().scale(3.2, 0.7).rotate(-12))
    sx.fillStyle = pat
    sx.fillRect(0, 0, sc.width, sc.height)
    cc.save()
    cc.setTransform(1, 0, 0, 1, 0, 0)
    cc.filter = `blur(${3 * dpr}px)`
    cc.globalAlpha = 0.45
    cc.drawImage(sc, 0, 0)
    cc.filter = 'none'
    cc.globalAlpha = 1
    cc.drawImage(sc, 0, 0)
    cc.restore()

    // —— 板擦的一下：宽 140、厚 56，纹路顺着擦的方向 ——
    stamp.width = Math.ceil(56 * MS)
    stamp.height = Math.ceil(140 * MS)
    const st = stamp.getContext('2d')!
    const img = st.createImageData(stamp.width, stamp.height)
    const lines = Array.from({ length: stamp.height }, () => Math.random())
    for (let y = 0; y < stamp.height; y++) {
      const ey = Math.min(1, y / (8 * MS), (stamp.height - 1 - y) / (8 * MS))
      const streakA = 1 - 0.65 * (lines[y] > 0.72 ? lines[y] : 0)
      for (let x = 0; x < stamp.width; x++) {
        const ex = Math.sin(Math.PI * clamp(x / (stamp.width - 1)))
        img.data[(y * stamp.width + x) * 4 + 3] = 255 * ey * ex ** 0.6 * streakA
      }
    }
    st.putImageData(img, 0, 0)

    reset()
  }

  function reset() {
    split = 0.5
    mctx.globalCompositeOperation = 'source-over'
    mctx.clearRect(0, 0, mask.width, mask.height)
    mctx.fillStyle = '#000'
    mctx.fillRect(Math.round(mask.width * split), 0, mask.width, mask.height)
    handle.style.left = `${split * 100}%`
    dirty = true
  }

  function composite() {
    if (!dirty) return
    dirty = false
    const t = tmp.getContext('2d')!
    t.setTransform(1, 0, 0, 1, 0, 0)
    t.globalCompositeOperation = 'copy'
    t.drawImage(white, 0, 0)
    t.globalCompositeOperation = 'destination-out'
    t.drawImage(mask, 0, 0, tmp.width, tmp.height)
    t.globalCompositeOperation = 'source-over'
    vctx.setTransform(1, 0, 0, 1, 0, 0)
    vctx.globalCompositeOperation = 'copy'
    vctx.drawImage(chalk, 0, 0)
    vctx.globalCompositeOperation = 'source-over'
    vctx.drawImage(tmp, 0, 0)
  }

  // —— 擦 ——
  type Dust = { x: number; y: number; vx: number; vy: number; life: number; max: number; r: number; a: number }
  let dust: Dust[] = []
  let mode: 'toChalk' | 'toWhite' = 'toChalk'
  let erasing = false
  let lastP: Pt | null = null
  let angle = 0

  function blackAt(x: number, y: number) {
    const d = mctx.getImageData(clamp(Math.floor(x * MS), 0, mask.width - 1), clamp(Math.floor(y * MS), 0, mask.height - 1), 1, 1).data
    return d[3] > 127
  }

  function rub(a: Pt, b: Pt) {
    const d = Math.hypot(b[0] - a[0], b[1] - a[1])
    const n = Math.max(1, Math.ceil(d / 5))
    mctx.globalCompositeOperation = mode === 'toChalk' ? 'source-over' : 'destination-out'
    mctx.globalAlpha = 0.34
    for (let i = 1; i <= n; i++) {
      const x = a[0] + ((b[0] - a[0]) * i) / n
      const y = a[1] + ((b[1] - a[1]) * i) / n
      mctx.save()
      mctx.translate(x * MS, y * MS)
      mctx.rotate(angle)
      mctx.drawImage(stamp, -stamp.width / 2, -stamp.height / 2)
      mctx.restore()
      // 粉笔灰
      if (Math.random() < 0.55) {
        const along = (Math.random() - 0.5) * 50
        const across = (Math.random() - 0.5) * 130
        const ca = Math.cos(angle)
        const sa = Math.sin(angle)
        dust.push({
          x: x + along * ca - across * sa,
          y: y + along * sa + across * ca,
          vx: (Math.random() - 0.5) * 20 + (b[0] - a[0]) * 0.6,
          vy: 6 + Math.random() * 22,
          life: 0,
          max: 0.8 + Math.random() * 1.8,
          r: 0.5 + Math.random() * 1.6,
          a: 0.2 + Math.random() * 0.5,
        })
      }
    }
    mctx.globalAlpha = 1
    dirty = true
  }

  function moveEraser(x: number, y: number, dx: number, dy: number) {
    if (Math.hypot(dx, dy) > 1.5) {
      // 板擦跟着方向轻轻转：长边始终和擦的方向垂直
      const target = clamp(Math.atan2(dy, dx), -Math.PI, Math.PI)
      let t = target
      if (t > Math.PI / 2) t -= Math.PI
      if (t < -Math.PI / 2) t += Math.PI
      angle += (clamp(t, -0.5, 0.5) - angle) * 0.25
    }
    eraser.style.transform = `translate(${x}px, ${y}px) rotate(${angle}rad) ${erasing ? 'scale(0.97)' : ''}`
  }

  const local = (e: PointerEvent): Pt => {
    const r = board.getBoundingClientRect()
    return [e.clientX - r.left, e.clientY - r.top]
  }

  board.addEventListener('pointerleave', () => {
    lastP = null
  })
  // 板擦平时藏着，按住才跟着光标出现，松手淡出去
  board.addEventListener('pointerdown', (e) => {
    if (e.target === handle || e.button !== 0) return
    const p = local(e)
    erasing = true
    moveEraser(p[0], p[1], 0, 0)
    board.classList.add('erasing')
    board.setPointerCapture(e.pointerId)
    mode = blackAt(p[0], p[1]) ? 'toWhite' : 'toChalk'
    lastP = p
    rub(p, p)
    wake()
  })
  board.addEventListener('pointermove', (e) => {
    const p = local(e)
    const prev = lastP ?? p
    moveEraser(p[0], p[1], p[0] - prev[0], p[1] - prev[1])
    if (erasing) {
      rub(prev, p)
      wake()
    }
    lastP = p
  })
  const stop = (e: PointerEvent) => {
    erasing = false
    board.classList.remove('erasing')
    if (board.hasPointerCapture(e.pointerId)) board.releasePointerCapture(e.pointerId)
  }
  board.addEventListener('pointerup', stop)
  board.addEventListener('pointercancel', stop)
  board.addEventListener('dblclick', (e) => {
    if (e.target === handle) return
    reset()
    wake()
  })

  // 把手：整片拉过去
  handle.addEventListener('pointerdown', (e) => {
    e.stopPropagation()
    handle.setPointerCapture(e.pointerId)
    handle.classList.add('dragging')
  })
  handle.addEventListener('pointermove', (e) => {
    if (!handle.hasPointerCapture(e.pointerId)) return
    const x = clamp(local(e)[0] / W, 0.02, 0.98)
    const a = Math.round(split * mask.width)
    const b = Math.round(x * mask.width)
    if (a === b) return
    mctx.globalAlpha = 1
    if (b > a) {
      // 往右拉：白板变多
      mctx.globalCompositeOperation = 'destination-out'
      mctx.fillRect(a, 0, b - a, mask.height)
    } else {
      mctx.globalCompositeOperation = 'source-over'
      mctx.fillStyle = '#000'
      mctx.fillRect(b, 0, a - b, mask.height)
    }
    for (let i = 0; i < 6; i++)
      dust.push({ x: x * W + (Math.random() - 0.5) * 8, y: Math.random() * H, vx: (Math.random() - 0.5) * 30, vy: 10 + Math.random() * 20, life: 0, max: 1 + Math.random(), r: 0.6 + Math.random() * 1.4, a: 0.3 + Math.random() * 0.4 })
    split = x
    handle.style.left = `${x * 100}%`
    dirty = true
    wake()
  })
  const hUp = (e: PointerEvent) => {
    handle.classList.remove('dragging')
    if (handle.hasPointerCapture(e.pointerId)) handle.releasePointerCapture(e.pointerId)
  }
  handle.addEventListener('pointerup', hUp)
  handle.addEventListener('pointercancel', hUp)
  handle.addEventListener('keydown', (e) => {
    const d = e.key === 'ArrowLeft' ? -0.05 : e.key === 'ArrowRight' ? 0.05 : 0
    if (!d) return
    e.preventDefault()
    const x = clamp(split + d, 0.02, 0.98)
    mctx.globalAlpha = 1
    const a = Math.round(split * mask.width)
    const b = Math.round(x * mask.width)
    if (b > a) {
      mctx.globalCompositeOperation = 'destination-out'
      mctx.fillRect(a, 0, b - a, mask.height)
    } else {
      mctx.globalCompositeOperation = 'source-over'
      mctx.fillStyle = '#000'
      mctx.fillRect(b, 0, a - b, mask.height)
    }
    split = x
    handle.style.left = `${x * 100}%`
    dirty = true
    wake()
  })

  // —— 滚动：板子从带边距的一张卡片，长成整整一屏 ——
  function onScroll() {
    const r = section.getBoundingClientRect()
    const vh = innerHeight
    const p = easeOut(clamp((vh - r.top) / vh))
    board.style.setProperty('--ci-y', `${(9 * (1 - p)).toFixed(3)}vh`)
    board.style.setProperty('--ci-x', `${(8 * (1 - p)).toFixed(3)}vw`)
    board.style.setProperty('--cr', `${(16 * (1 - p)).toFixed(2)}px`)
  }
  addEventListener('scroll', onScroll, { passive: true })
  onScroll()

  let running = false
  let last = performance.now()
  function wake() {
    if (running) return
    running = true
    last = performance.now()
    requestAnimationFrame(frame)
  }
  function frame(now: number) {
    const dt = Math.min(0.05, (now - last) / 1000)
    last = now
    composite()
    dctx.clearRect(0, 0, W, H)
    dust = dust.filter((d) => (d.life += dt) < d.max)
    for (const d of dust) {
      d.vy += 18 * dt
      d.vx *= 0.97
      d.x += d.vx * dt
      d.y += d.vy * dt
      dctx.globalAlpha = d.a * (1 - d.life / d.max) ** 1.5
      dctx.fillStyle = '#f4f4ee'
      dctx.beginPath()
      dctx.arc(d.x, d.y, d.r, 0, Math.PI * 2)
      dctx.fill()
    }
    dctx.globalAlpha = 1
    if (dust.length || dirty || erasing) requestAnimationFrame(frame)
    else running = false
  }

  let rt = 0
  const ro = new ResizeObserver(() => {
    clearTimeout(rt)
    rt = window.setTimeout(() => {
      build()
      wake()
    }, 150)
  })
  ro.observe(board)
  build()
  wake()
}
