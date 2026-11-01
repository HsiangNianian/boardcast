// 首屏总调度：量尺寸 → 排板书 → 开场写字 → 推杆变形 → 对话一直往下说 → 词块的呼吸。
// - 鼠标划过左下角某一句对话：右边对应的词块轻轻浮起，笔触加深一圈；
// - 鼠标掠过右边的核心词块：只有碰到的那一块弹性地「呼吸」一下，移开就安静下来。

import { type Pt, clamp, drawPiece, fitCanvas, reduceMotion } from '../ink/core'
import { type Board, type Item, compose } from './board'
import { Fader } from './fader'
import { DEFAULT_MESSY, DEFAULT_WEIGHT } from '../../../src/ink/defaults'
import { type HeroGeo, measureHero } from './geo'
import { Talk } from './talk'

const SPEED = 2.4 // 开场写字的笔速（像素 / 毫秒）

export interface HeroApi {
  geo(): HeroGeo
  board(): Board
  /** 主箭头尖端在文档里的坐标 */
  tip(): Pt
  onLayout(fn: () => void): void
}

/** 一根小弹簧 */
class Spring {
  x: number
  v = 0
  constructor(public target: number) {
    this.x = target
  }
  step(dt: number, k: number, c: number) {
    this.v += (k * (this.target - this.x) - c * this.v) * dt
    this.x += this.v * dt
  }
  get rest() {
    return Math.abs(this.x - this.target) < 0.001 && Math.abs(this.v) < 0.01
  }
}

interface BlockState {
  lift: Spring
  scale: Spring
  bold: Spring
  hover: boolean
  linked: boolean
  until: number
}

export function initHero(): HeroApi {
  const hero = document.querySelector<HTMLElement>('.hero')!
  const frame = hero.querySelector<HTMLElement>('.frame')!
  const canvas = frame.querySelector<HTMLCanvasElement>('.hero-ink')!
  const valEl = frame.querySelector<HTMLOutputElement>('.val-messy')!
  const wValEl = frame.querySelector<HTMLOutputElement>('.val-weight')!
  const still = reduceMotion()

  let geo = measureHero(frame)
  let ctx = fitCanvas(canvas, geo.W, geo.H).ctx
  const cache = document.createElement('canvas')
  let cctx = fitCanvas(cache, geo.W, geo.H).ctx
  let cacheOk = false

  let target = DEFAULT_MESSY
  let messy = target
  let built = -1
  // 粗细推杆：推子位置 0..1 → 笔宽倍数 0.5×..2×（指数刻度，1× 正好在中间；再粗汉字就糊成一团了）
  const toWeight = (v: number) => 0.5 * 4 ** v
  let wTarget = DEFAULT_WEIGHT
  let weight = wTarget
  let wBuilt = weight
  let board: Board = compose(geo, messy, weight)
  const introAt = still ? -1e9 : performance.now() + 120
  let introDone = still
  let idleSince = 0
  const listeners: (() => void)[] = []

  const states = new Map<string, BlockState>()
  const state = (id: string) => {
    let s = states.get(id)
    if (!s) {
      s = { lift: new Spring(0), scale: new Spring(1), bold: new Spring(0), hover: false, linked: false, until: 0 }
      states.set(id, s)
    }
    return s
  }

  const fader = new Fader(frame.querySelector('.fader-messy')!, target, (v) => {
    target = v
  })
  const wFader = new Fader(frame.querySelector('.fader-weight')!, Math.log(DEFAULT_WEIGHT / 0.5) / Math.log(4), (v) => {
    wTarget = toWeight(v)
  })
  const talk = new Talk(frame.querySelector('.talk')!, frame, {
    block: (id) => board.blocks[id]?.box ?? null,
    blocks: () => Object.entries(board.blocks).map(([id, b]) => [id, b.box]),
    lift: (id, on, ms) => {
      const s = state(id)
      if (ms) s.until = performance.now() + ms
      else s.linked = on
    },
  })
  talk.start(performance.now() + 900, still)

  const rebuild = () => {
    board = compose(geo, messy, weight)
    built = messy
    wBuilt = weight
    cacheOk = false
  }

  const progress = (it: Item, t: number) => (introDone ? 1 : clamp((t - it.delay) / Math.max(160, Math.min(950, it.p.len / SPEED))))

  /** 哪些词块正在动：这些每一帧现画，其余的贴缓存 */
  const moving = () => [...states.entries()].filter(([, s]) => !(s.lift.rest && s.scale.rest && s.bold.rest && s.lift.x === 0 && s.scale.x === 1))

  const drawItems = (c: CanvasRenderingContext2D, t: number, skip?: Set<string>) => {
    for (const it of board.items) {
      if (it.block && skip?.has(it.block)) continue
      drawPiece(c, it.p, progress(it, t))
    }
  }

  const drawBlock = (c: CanvasRenderingContext2D, id: string, s: BlockState, t: number) => {
    const bl = board.blocks[id]
    if (!bl) return
    const lift = s.lift.x
    const sc = s.scale.x
    const bold = Math.max(0, s.bold.x) * 1.3
    const items = board.items.filter((it) => it.block === id)
    // 浮起来时底下有一点点影子
    if (lift < -0.3) {
      c.save()
      c.translate(0, 3)
      for (const it of items) drawPiece(c, it.p, progress(it, t), clamp(-lift / 4) * 0.08, 0, { color: '#000' })
      c.restore()
    }
    c.save()
    c.translate(bl.cx, bl.cy + lift)
    c.scale(sc, sc)
    c.translate(-bl.cx, -bl.cy)
    for (const it of items) drawPiece(c, it.p, progress(it, t), 1, 0, { bold })
    c.restore()
  }

  let visible = true
  new IntersectionObserver(([e]) => {
    visible = e.isIntersecting
    if (visible) requestAnimationFrame(loop)
  }).observe(hero)

  let last = performance.now()
  function loop(now: number) {
    const dt = Math.min(0.05, (now - last) / 1000)
    last = now
    fader.update(now)
    wFader.update(now)
    // 板书跟着推子走，但带一点阻尼：像是被拖着变形
    const ease = 1 - Math.exp(-dt / 0.07)
    messy += (target - messy) * ease
    if (Math.abs(target - messy) < 0.0008) messy = target
    weight += (wTarget - weight) * ease
    if (Math.abs(wTarget - weight) < 0.002) weight = wTarget
    if (Math.abs(messy - built) > 0.0012 || Math.abs(weight - wBuilt) > 0.004) {
      rebuild()
      idleSince = now
    }
    valEl.value = String(Math.round(messy * 100))
    wValEl.value = `${Math.round(weight * 100)}%`

    // 词块：对话提到 / 划过对话 → 浮起、笔触加深；鼠标碰到 → 弹一下、轻轻呼吸
    for (const s of states.values()) {
      const linked = s.linked || now < s.until
      s.lift.target = linked ? -4 : 0
      s.bold.target = linked || s.hover ? 1 : 0
      s.scale.target = s.hover ? 1.045 + 0.012 * Math.sin(now / 320) : linked ? 1.015 : 1
      s.lift.step(dt, 170, 16)
      s.bold.step(dt, 120, 20)
      s.scale.step(dt, s.hover ? 260 : 200, s.hover ? 10 : 14)
    }

    const t = now - introAt
    if (!introDone && board.items.every((it) => progress(it, t) >= 1)) {
      introDone = true
      idleSince = now
      setTimeout(() => fader.hint(performance.now()), 2400)
    }
    const active = moving()
    const skip = new Set(active.map(([id]) => id))
    // 静下来以后，整块板烤成一张图，每帧只贴图；有词块在动时，整块现画（动的那几块单独画）
    ctx.clearRect(0, 0, geo.W, geo.H)
    if (introDone && messy === target && weight === wTarget && now - idleSince > 60 && !active.length) {
      if (!cacheOk) {
        cctx.clearRect(0, 0, geo.W, geo.H)
        drawItems(cctx, t)
        cacheOk = true
      }
      ctx.drawImage(cache, 0, 0, geo.W, geo.H)
    } else drawItems(ctx, t, skip)
    for (const [id, s] of active) drawBlock(ctx, id, s, t)

    talk.update(now)
    talk.draw(ctx)
    if (visible) requestAnimationFrame(loop)
  }
  requestAnimationFrame(loop)

  // 鼠标碰到哪个词块
  frame.addEventListener('pointermove', (e) => {
    if (still || e.pointerType === 'touch') return
    const r = canvas.getBoundingClientRect()
    const x = e.clientX - r.left
    const y = e.clientY - r.top
    let hit: string | null = null
    for (const [id, b] of Object.entries(board.blocks)) {
      if (x > b.box.x0 - 6 && x < b.box.x1 + 6 && y > b.box.y0 - 6 && y < b.box.y1 + 6) hit = id
    }
    for (const [id, s] of states) if (id !== hit) s.hover = false
    if (hit) {
      const s = state(hit)
      if (!s.hover) s.scale.v += 0.6 // 碰到的那一下，先弹一弹
      s.hover = true
    }
  })
  frame.addEventListener('pointerleave', () => {
    for (const s of states.values()) s.hover = false
  })

  let rt = 0
  let sized = `${frame.clientWidth}x${frame.clientHeight}`
  const relayout = () => {
    const now = `${frame.clientWidth}x${frame.clientHeight}`
    if (now === sized) return
    sized = now
    geo = measureHero(frame)
    ctx = fitCanvas(canvas, geo.W, geo.H).ctx
    cctx = fitCanvas(cache, geo.W, geo.H).ctx
    rebuild()
    fader.layout()
    wFader.layout()
    talk.relayout(performance.now())
    for (const fn of listeners) fn()
  }
  const ro = new ResizeObserver(() => {
    clearTimeout(rt)
    rt = window.setTimeout(relayout, 60)
  })
  ro.observe(frame)

  return {
    geo: () => geo,
    board: () => board,
    tip: () => [board.tip[0] + geo.ox, board.tip[1] + geo.oy],
    onLayout: (fn) => listeners.push(fn),
  }
}
