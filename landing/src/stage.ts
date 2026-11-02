// 往下滚：首屏那根主箭头的尖端顺着滚动往下「画」出一条墨线，穿过分界线进到右栏的点阵画板，串起三幕。
// - 笔尖跟着滚动走：滑得快就往下窜，停下就停住；画过的墨不会收回去；
// - 每一幕都在滑到屏幕正中之前画完：中间永远不会出现写了一半的字、没封口的框、悬着的断线；
// - 第一幕「连线成图」：散落漂浮、歪着的词块，被追上来的墨线串成结构图，顺着线的拉力弹回原位；
// - 第二幕「当场涂改」：旧方案被两道红线划掉，正下方一根短箭头，立起新词，红笔圈两圈、打星；
// - 第三幕「拖拽回弹」：「一场会」和四个分支，其中一块一直轻轻呼吸，角上一个小双向箭头，拽它，连线跟着弯、跟着晃，松手弹回去。
// 整条墨线和三幕都画在一张固定在视口上的画布里，按文档坐标摆。

import { DEFAULT_MESSY } from '../../src/ink/defaults'
import {
  type Piece,
  type Pt,
  type Stroke,
  clamp,
  densify,
  dist,
  drawPiece,
  fitCanvas,
  lerp,
  makePiece,
  outlineOf,
  reduceMotion,
  rng,
  sliceRange,
  smoothstep,
  toPath,
  wobble,
} from './ink/core'
import { measure, penFor, write } from './ink/hand'
import { type Icon, arrow, bow, box, cubic, head, icon, loop, strike } from './ink/shapes'
import { FAINT, INK, RED } from './hero/board'
import type { HeroApi } from './hero'

const MAIN_PEN = 3.4

// —— 墨线的一段 ——

interface Seg {
  /** 静态：一串画好的点（文档坐标） */
  pts?: Stroke
  cmax?: number[]
  cum?: number[]
  len?: number
  /** 动态：两头跟着节点走 */
  a?: () => Pt
  b?: () => Pt
  y0: number
  y1: number
  f: number
  pen: number
  head?: boolean
  /** 动态段两头是横着进出的（默认竖着） */
  side?: boolean
  seed: number
  path?: Path2D
}

/** 一串路标点连成顺滑的线；d 是在这一点的走向（不给就按前后两点估） */
type Way = { p: Pt; d?: Pt }
function chain(way: Way[], per = 26): Stroke {
  const n = way.length
  const dir = (i: number): Pt => {
    const d = way[i].d
    if (d) {
      const l = Math.hypot(d[0], d[1]) || 1
      return [d[0] / l, d[1] / l]
    }
    const a = way[Math.max(0, i - 1)].p
    const b = way[Math.min(n - 1, i + 1)].p
    const l = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1
    return [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
  }
  const out: Stroke = []
  for (let i = 0; i < n - 1; i++) {
    const a = way[i].p
    const b = way[i + 1].p
    const h = dist(a, b) * 0.4
    const da = dir(i)
    const db = dir(i + 1)
    const seg = cubic(a, [a[0] + da[0] * h, a[1] + da[1] * h], [b[0] - db[0] * h, b[1] - db[1] * h], b, per)
    out.push(...(i ? seg.slice(1) : seg))
  }
  return out
}

function staticSeg(line: Stroke, seed: number, o: Partial<Seg> = {}): Seg {
  const r = rng(`seg${seed}`)
  const pts = wobble(densify(line, 3), MAIN_PEN * 0.35, 0.8 + r() * 2.5, r() * 6)
  const cum = [0]
  const cmax = [pts[0][1]]
  for (let i = 1; i < pts.length; i++) {
    cum.push(cum[i - 1] + dist(pts[i - 1], pts[i]))
    cmax.push(Math.max(cmax[i - 1], pts[i][1]))
  }
  return { pts, cum, cmax, len: cum[cum.length - 1], y0: pts[0][1], y1: cmax[cmax.length - 1], f: 0, pen: MAIN_PEN, seed, ...o }
}

function dynSeg(a: () => Pt, b: () => Pt, y0: number, y1: number, seed: number, o: Partial<Seg> = {}): Seg {
  return { a, b, y0, y1, f: 0, pen: MAIN_PEN * 0.92, seed, ...o }
}

// —— 节点（手写的词块） ——

interface GNode {
  id: string
  home: Pt
  pos: Pt
  vel: Pt
  rot: number
  vrot: number
  w: number
  h: number
  pieces: Piece[]
  /** 散落时的偏移和歪斜 */
  fx: number
  fy: number
  frot: number
  ph: number
  reached: boolean
  /** 一直轻轻呼吸（暗示它能拽） */
  breathe?: boolean
}

function makeNode(id: string, text: string, size: number, home: Pt, o: { icon?: Icon; star?: boolean; sub?: string; grip?: boolean } = {}): GNode {
  const r = rng(id)
  const tw = measure(text, size)
  const ic = o.icon ? size * 1.05 : 0
  const icGap = o.icon ? size * 0.3 : 0
  const subSize = size * 0.5
  const subW = o.sub ? measure(o.sub, subSize) : 0
  const padX = size * 0.55
  const padY = size * 0.42
  const inner = Math.max(ic + icGap + tw, subW)
  const w = inner + padX * 2
  const h = size + padY * 2 + (o.sub ? subSize * 1.25 : 0)
  const x0 = -w / 2
  const y0 = -h / 2
  const pieces: Piece[] = []
  const m = DEFAULT_MESSY
  const tx = x0 + padX + (inner - ic - icGap - tw) / 2
  pieces.push(makePiece(`${id}b`, INK, box(x0, y0, w, h, r, 0.3), penFor(size) * 0.82, m))
  if (o.icon) pieces.push(makePiece(`${id}i`, INK, icon(o.icon, tx, y0 + padY - size * 0.02, ic, r), penFor(size) * 0.8, m))
  pieces.push(makePiece(`${id}t`, INK, write(text, tx + ic + icGap, y0 + padY, size, id, m).marks, penFor(size), m))
  if (o.sub) pieces.push(makePiece(`${id}s`, FAINT, write(o.sub, -subW / 2, y0 + padY + size * 1.12, subSize, `${id}s`, m).marks, penFor(subSize), m))
  if (o.star) pieces.push(makePiece(`${id}*`, INK, write('★', x0 + w - size * 0.3, y0 - size * 0.62, size * 0.8, `${id}*`, DEFAULT_MESSY).marks, penFor(size), DEFAULT_MESSY))
  if (o.grip) {
    // 框角一个小小的双向箭头：不用字，也知道能拽
    const a: Pt = [x0 + w + 4, y0 + 16]
    const b: Pt = [x0 + w + 22, y0 - 2]
    const shaft = bow(a, b, 1, 8)
    const L = 6
    pieces.push(makePiece(`${id}g`, INK, [shaft, ...head(shaft, L, r, 0.6), ...head([...shaft].reverse(), L, r, 0.6)], 1.6, DEFAULT_MESSY))
  }
  const sx = r() < 0.5 ? -1 : 1
  return {
    id,
    home,
    pos: [home[0], home[1]],
    vel: [0, 0],
    rot: 0,
    vrot: 0,
    w,
    h,
    pieces,
    fx: sx * (110 + r() * 150),
    fy: (r() - 0.5) * 90,
    frot: sx * (0.08 + r() * 0.18) * (r() < 0.25 ? -1 : 1),
    ph: r() * Math.PI * 2,
    reached: false,
  }
}

const rot = (p: Pt, a: number): Pt => [p[0] * Math.cos(a) - p[1] * Math.sin(a), p[0] * Math.sin(a) + p[1] * Math.cos(a)]
const top = (n: GNode): Pt => {
  const o = rot([0, -n.h / 2 - 5], n.rot)
  return [n.pos[0] + o[0], n.pos[1] + o[1]]
}
const bottom = (n: GNode): Pt => {
  const o = rot([0, n.h / 2 + 5], n.rot)
  return [n.pos[0] + o[0], n.pos[1] + o[1]]
}
/** 从框中心往 p 的方向，落在框边上的点 */
function edgeOf(n: GNode, p: Pt, pad = 8): Pt {
  const c = n.pos
  const dx = p[0] - c[0]
  const dy = p[1] - c[1]
  const hw = n.w / 2 + pad
  const hh = n.h / 2 + pad
  if (!dx && !dy) return c
  const t = Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity, 1)
  return [c[0] + dx * t, c[1] + dy * t]
}

/** f：写到哪儿了（框先画，再写字） */
function drawNode(ctx: CanvasRenderingContext2D, n: GNode, f = 1) {
  if (f <= 0) return
  ctx.save()
  ctx.translate(n.pos[0], n.pos[1])
  ctx.rotate(n.rot)
  const k = n.pieces.length
  n.pieces.forEach((p, i) => drawPiece(ctx, p, clamp(f * k - i * 0.7)))
  ctx.restore()
}

/** 一段在 [a, b] 这个区间里画完的东西（区间是「笔尖走到哪儿」的文档纵坐标） */
interface Timed {
  p: Piece
  a: number
  b: number
}

export function initStage(hero: HeroApi) {
  const canvas = document.querySelector<HTMLCanvasElement>('.stage')!
  let ctx = canvas.getContext('2d')!
  let dpr = 1
  const still = reduceMotion()
  let vw = innerWidth
  let vh = innerHeight

  let segs: Seg[] = []
  let act1: GNode[] = []
  let act1Edges: { seg: Seg; label?: Piece }[] = []
  let ops: Timed[] = []
  let revise: Timed[] = []
  let hub: GNode | null = null
  let sats: GNode[] = []
  let act3Edges: { a: GNode; b: GNode; mid: Pt; mv: Pt; bend: number; seed: number }[] = []
  let act3: { A: number; B: number } = { A: 0, B: 1 }
  /** 滚到哪儿，笔尖至少要到哪儿：保证每一幕到屏幕正中之前就画完 */
  let need: [number, number][] = []
  let tipY = -1
  let maxDoc = 0

  const drag = { node: null as GNode | null, off: [0, 0] as Pt, last: [0, 0] as Pt, lastT: 0, v: [0, 0] as Pt }

  function layout() {
    vw = innerWidth
    vh = innerHeight
    ;({ dpr } = fitCanvas(canvas, vw, vh))
    ctx = canvas.getContext('2d')!
    const docTop = (el: Element) => el.getBoundingClientRect().top + scrollY
    const e1 = document.querySelector<HTMLElement>('.act-connect')!
    const e2 = document.querySelector<HTMLElement>('.act-revise')!
    const e3 = document.querySelector<HTMLElement>('.act-drag')!
    const sw = document.querySelector<HTMLElement>('.swap')!
    const g = hero.geo()
    const b = hero.board()
    const tip = hero.tip()
    const narrow = g.stacked
    // 右栏（点阵画板）的范围
    const colX0 = narrow ? g.ox : g.ox + g.divX + 1
    const colX1 = g.ox + g.W
    const cx = (colX0 + colX1) / 2
    const colW = colX1 - colX0
    const s = clamp(colW / 900, 0.6, 1.2)
    const size = Math.round(28 * s)

    /** 每一幕内容的中心和可用高度；窄屏时说明文字压在上面，内容从说明下面开始 */
    const span = (el: HTMLElement) => {
      const T = docTop(el)
      const H = el.offsetHeight
      const cap = narrow ? el.querySelector('.cap')!.getBoundingClientRect().bottom + scrollY - T + 20 : 0
      return { T, H, cap: T + cap, C: T + cap + (H - cap) / 2, CH: Math.min((H - cap) * 0.86, vh * 0.74, 720) }
    }

    segs = []
    need = []
    let seed = 0

    // —— 第一幕：连线成图 ——
    const A1 = span(e1)
    const y1 = (f: number) => A1.C + A1.CH * f
    const lane = narrow ? colW * 0.24 : Math.min(colW * 0.21, 200 * s)
    const N = (id: string, text: string, x: number, f: number, o = {}) => makeNode(id, text, size, [x, y1(f)], o)
    const n1 = N('n1', '说话', cx, -0.46, { icon: 'mic' })
    const n2 = N('n2', '本机转写', cx, -0.27)
    const n3 = N('n3', narrow ? '快车道' : '快车道 · 规则', cx - lane, -0.05, narrow ? {} : { sub: '< 10 毫秒' })
    const n4 = N('n4', narrow ? '慢车道' : '慢车道 · 小模型', cx + lane, -0.05, narrow ? {} : { icon: 'cpu', sub: '本机 · 语法约束' })
    const n5 = N('n5', '图操作', cx, 0.16)
    const n6 = N('n6', '确定性排版', cx, 0.32)
    const n7 = N('n7', '手写板书', cx, 0.46, { star: true })
    act1 = [n1, n2, n3, n4, n5, n6, n7]
    // 散落的词只在右栏里飘
    for (const n of act1) {
      n.fx = clamp(n.home[0] + n.fx, colX0 + n.w / 2 + 18, colX1 - n.w / 2 - 18) - n.home[0]
      if (!n.reached) {
        n.pos = [n.home[0] + n.fx, n.home[1] + n.fy]
        n.rot = n.frot
      }
    }

    // 首屏：箭头尖 → 拐进分界线内侧 → 笔直往下出画框 → 越过分界线进右栏，落到第一个词块头上
    const ring = b.ring
    const exitX = narrow ? Math.min(tip[0], vw - 16) : g.ox + g.divX - 17
    let way: Way[]
    const edge = colX1 - 8
    /** 窄屏时，路过某一幕的说明文字：先贴右边滑下去 */
    const bypass = (T: number, capBottom: number): Way[] => (narrow ? [{ p: [edge, T + 10], d: [0, 1] }, { p: [edge, capBottom], d: [0, 1] }] : [])
    if (narrow) {
      const entry: Pt = [cx, n1.home[1] - n1.h / 2 - 70]
      way = [{ p: tip, d: [0, 1] }, { p: [exitX, g.oy + g.H + 20], d: [0, 1] }, ...bypass(A1.T, A1.cap), { p: entry, d: [0, 1] }]
      segs.push(staticSeg(chain(way), seed++))
      segs.push(dynSeg(() => entry, () => top(n1), entry[1], n1.home[1] - n1.h / 2, seed++))
    } else {
      // 贴着分界线一路往下，到第一幕那一行再拐弯，横着越过分界线，从侧面进第一个词块
      const turn: Pt = [colX0 + 34, n1.home[1]]
      way = [
        { p: tip, d: [0.15, 1] },
        { p: [exitX, g.oy + ring.cy + ring.ry * 0.95], d: [0, 1] },
        { p: [exitX, turn[1] - 90], d: [0, 1] },
        { p: turn, d: [1, 0] },
      ]
      segs.push(staticSeg(chain(way), seed++))
      segs.push(dynSeg(() => turn, () => edgeOf(n1, turn, 6), turn[1] - 8, turn[1] + 8, seed++, { side: true }))
    }
    act1Edges = []
    const E = (from: GNode, to: GNode, label?: string) => {
      const seg = dynSeg(() => bottom(from), () => top(to), from.home[1] + from.h / 2, to.home[1] - to.h / 2, seed++, { head: true })
      let lp: Piece | undefined
      if (label && !narrow) {
        const ls = Math.round(size * 0.55)
        const mx = (from.home[0] + to.home[0]) / 2
        const my = (from.home[1] + from.h / 2 + to.home[1] - to.h / 2) / 2
        const lw = measure(label, ls)
        const lx = Math.abs(mx - cx) < 4 ? mx + 18 : mx < cx ? mx - lw - 30 : mx + 30
        lp = makePiece(`lbl${seed}`, FAINT, write(label, lx, my - ls / 2, ls, `lbl${seed}`, DEFAULT_MESSY).marks, penFor(ls), DEFAULT_MESSY)
      }
      act1Edges.push({ seg, label: lp })
      segs.push(seg)
    }
    E(n1, n2)
    E(n2, n3, '提到旧节点')
    E(n2, n4, '读实体表 + 前两句')
    E(n3, n5, '铅笔稿')
    E(n4, n5, '只发图操作')
    E(n5, n6)
    E(n6, n7)
    // 图操作旁边写几行真的操作
    ops = []
    if (!narrow) {
      const opSize = Math.round(size * 0.5)
      ;['E 短信 → 扫码', 'M 划掉 · 短信', 'T 登录改版'].forEach((o, i) => {
        const x = n5.home[0] + n5.w / 2 + 30
        const y = n5.home[1] - opSize * 1.6 + i * opSize * 1.35
        const p = makePiece(`op${i}`, FAINT, write(o, x, y, opSize, `op${i}`, DEFAULT_MESSY).marks, penFor(opSize), DEFAULT_MESSY, { alpha: 0.85 })
        ops.push({ p, a: n5.home[1] - n5.h / 2 + i * 14, b: n5.home[1] + n5.h / 2 + 30 + i * 14 })
      })
    }
    const act1A = n1.home[1] - n1.h / 2 - 40
    const act1B = n7.home[1] + n7.h / 2 + 10

    // —— 第二幕：当场涂改。旧词在上，两道红线；正下方一根短箭头，立起新词，红笔圈两圈、打星 ——
    const A2 = span(e2)
    const oldSize = Math.round(54 * s * (narrow ? 0.82 : 1))
    const newSize = Math.round(62 * s * (narrow ? 0.82 : 1))
    const oldText = '上传云端识别'
    const newText = '全部本机识别'
    const ow = measure(oldText, oldSize)
    const nw = measure(newText, newSize)
    const gap = Math.round(104 * s)
    const blockH = oldSize + gap + newSize
    const oldY = A2.C - vh * 0.04 - blockH / 2
    const newY = oldY + oldSize + gap
    const ox2 = cx - ow / 2
    const nx2 = cx - nw / 2
    const rr = rng('revise')
    const m2 = DEFAULT_MESSY
    const ringRx = nw / 2 + 30 * s
    const ringRy = newSize / 2 + 24 * s
    const ringCy = newY + newSize * 0.5
    const ringBottom = ringCy + ringRy * 1.16 + 6
    const rA = oldY - 30
    // 墨线不戳进字里：从旧词左边绕下来，贴着新词的红圈外侧，兜到圈底下再往下走
    const sideX = narrow ? colX1 - 8 : cx - Math.max(ow / 2, ringRx) - 64 * s
    const under: Pt = [cx - 30 * s, ringBottom + 52 * s]
    const rB = under[1]
    const at = (f0: number, f1: number) => ({ a: lerp(rA, rB, f0), b: lerp(rA, rB, f1) })
    revise = [
      { p: makePiece('old', INK, write(oldText, ox2, oldY, oldSize, 'old', m2).marks, penFor(oldSize) * 1.05, m2), ...at(0, 0.26) },
      { p: makePiece('x1', RED, [strike(ox2, oldY + oldSize * 0.46, ow, 4, rr)], penFor(oldSize) * 1.3, m2), ...at(0.3, 0.38) },
      { p: makePiece('x2', RED, [strike(ox2 + 6, oldY + oldSize * 0.64, ow - 10, -3, rr)], penFor(oldSize) * 1.2, m2), ...at(0.4, 0.48) },
      {
        p: makePiece('down', INK, arrow(bow([cx + 4, oldY + oldSize + 16 * s], [cx - 2, newY - ringRy + newSize / 2 - 10 * s], 4 * s, 12), 14 * s, rr), penFor(40 * s), m2),
        ...at(0.5, 0.6),
      },
      { p: makePiece('new', INK, write(newText, nx2, newY, newSize, 'new', m2).marks, penFor(newSize) * 1.12, m2), ...at(0.6, 0.82) },
      {
        p: makePiece('newo', RED, [loop(cx, ringCy, ringRx, ringRy, rr, { turns: 2.08, start: 3.5, tilt: -0.05, grow: 0.12 })], penFor(newSize) * 1.05, m2),
        ...at(0.83, 0.95),
      },
      { p: makePiece('news', RED, write('★', cx + ringRx - 6 * s, ringCy - ringRy - 34 * s, Math.round(44 * s), 'news', m2).marks, penFor(44 * s) * 1.1, m2), ...at(0.95, 1) },
    ]
    // 第一幕最后一块 → 竖着落到旧词头上
    const n7b: Pt = [cx, n7.home[1] + n7.h / 2 + 60]
    segs.push(dynSeg(() => bottom(n7), () => n7b, n7.home[1] + n7.h / 2, n7b[1], seed++))
    const way2: Way[] = [
      { p: n7b, d: [0, 1] },
      ...bypass(A2.T, A2.cap),
      { p: [sideX, oldY + oldSize * 0.5], d: [0, 1] },
      { p: [sideX, ringCy], d: [0, 1] },
      { p: under, d: [1, 0.35] },
    ]

    // —— 第三幕：拖拽回弹 ——
    const A3 = span(e3)
    const c3: Pt = [cx, A3.C]
    hub = makeNode('hub', '一场会', Math.round(size * 1.25), c3, { star: true })
    const sat: [string, number, boolean][] = [
      ['PAD 板书模式', 215, false],
      ['黑板 · 粉笔', -35, false],
      ['PC 会议模式', 35, true],
      ['回放 · 任意时刻', 145, false],
    ]
    const Rx = narrow ? colW * 0.27 : Math.min(colW * 0.3, 300 * s)
    const Ry = Math.min(A3.CH * 0.32, 200 * s)
    const ss = Math.round(size * (narrow ? 0.72 : 0.82))
    sats = sat.map(([t, deg, grip], i) => {
      const a = (deg * Math.PI) / 180
      const n = makeNode(`sat${i}`, t, ss, [cx + Math.cos(a) * Rx, c3[1] + Math.sin(a) * Ry], { grip })
      n.breathe = grip
      return n
    })
    act3Edges = sats.map((n, i) => {
      const jr = rng(`e3${i}`)
      return { a: hub!, b: n, mid: [(c3[0] + n.home[0]) / 2, (c3[1] + n.home[1]) / 2], mv: [0, 0], bend: (jr() - 0.5) * 40, seed: i }
    })
    const hubTop: Pt = [cx, c3[1] - hub.h / 2 - 70]
    const h3 = hub
    act3 = { A: hubTop[1], B: Math.max(...sats.map((n) => n.home[1] + n.h / 2)) + 10 }
    segs.push(staticSeg(chain([...way2, ...bypass(A3.T, A3.cap), { p: hubTop, d: [0, 1] }]), seed++))
    segs.push(dynSeg(() => hubTop, () => top(h3), hubTop[1], c3[1] - h3.h / 2, seed++))

    // —— 往白板黑板去：从会的下沿出来，竖着穿出画框，落到下一屏的板子上沿，箭头朝下 ——
    const hubOut: Pt = [cx, c3[1] + h3.h / 2 + 60]
    segs.push(dynSeg(() => bottom(h3), () => hubOut, c3[1] + h3.h / 2, hubOut[1], seed++))
    const end: Pt = [cx, docTop(sw) + vh * 0.04]
    segs.push(staticSeg(chain([{ p: hubOut, d: [0, 1] }, { p: end, d: [0, 1] }]), seed++, { head: true }))
    maxDoc = end[1]

    // 每一幕在滑到屏幕正中（再提前一点）之前画完
    const done = (C: number) => C - vh * 0.5 - vh * 0.1
    let prev = -Infinity
    for (const [C, a, b0] of [
      [A1.C, act1A, act1B],
      [A2.C, rA, rB],
      [A3.C, act3.A, act3.B],
    ] as [number, number, number][]) {
      const d = done(C)
      const st = Math.max(prev + 1, d - vh * 0.5)
      need.push([st, a], [d, b0])
      prev = d
    }
    need.push([docTop(sw) - vh * 0.7, maxDoc])

    if (still) tipY = maxDoc + 1
  }

  // —— 笔尖跟着滚动走 ——
  function tipTarget() {
    const s = scrollY
    const start = hero.tip()[1]
    const base = Math.max(start, s + lerp(start, vh * 0.66, clamp(s / (vh * 0.35))))
    let req = -Infinity
    for (let i = 0; i < need.length; i++) {
      const [s0, y0] = need[i]
      if (s < s0) {
        if (i > 0) {
          const [sp, yp] = need[i - 1]
          req = lerp(yp, y0, clamp((s - sp) / (s0 - sp)))
          // 两幕之间：上一幕画完就停在那儿，等下一幕
          if (i % 2 === 0 && i < need.length - 1) req = yp
        }
        break
      }
      req = y0
    }
    return Math.max(base, req)
  }

  function revealStatic(seg: Seg, y: number) {
    if (y <= seg.y0) return 0
    const cm = seg.cmax!
    if (y >= cm[cm.length - 1]) return 1
    let lo = 0
    let hi = cm.length - 1
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (cm[mid] < y) lo = mid + 1
      else hi = mid
    }
    return seg.cum![lo] / seg.len!
  }

  function drawSeg(seg: Seg) {
    if (seg.f <= 0) return
    ctx.fillStyle = INK
    if (seg.pts) {
      if (seg.f >= 1) {
        if (!seg.path) {
          seg.path = toPath(outlineOf(seg.pts, seg.pen))
          if (seg.head) for (const h of head(seg.pts, 16, rng(`h${seg.seed}`), 0.5)) seg.path.addPath(toPath(outlineOf(densify(h, 2), seg.pen)))
        }
        ctx.fill(seg.path)
      } else {
        const part = sliceRange(seg.pts, 0, seg.f, seg.len)
        if (part.length > 1) ctx.fill(toPath(outlineOf(part, seg.pen, { taper: 0, last: false })))
      }
      return
    }
    const a = seg.a!()
    const b = seg.b!()
    const r = rng(`d${seg.seed}`)
    const d = Math.max(30, Math.abs((seg.side ? b[0] - a[0] : b[1] - a[1])) * 0.5)
    const c1: Pt = seg.side ? [a[0] + d, a[1]] : [a[0], a[1] + d]
    const c2: Pt = seg.side ? [b[0] - d * 0.6, b[1]] : [b[0], b[1] - d]
    const pts = wobble(densify(cubic(a, c1, c2, b, 28), 3), seg.pen * 0.3, 0.8 + r() * 2, r() * 6)
    const part = seg.f >= 1 ? pts : sliceRange(pts, 0, seg.f)
    if (part.length > 1) ctx.fill(toPath(outlineOf(part, seg.pen, seg.f >= 1 ? {} : { taper: 0, last: false })))
    if (seg.head && seg.f >= 1) for (const h of head(pts, 11, rng(`h${seg.seed}`), 0.5)) ctx.fill(toPath(outlineOf(densify(h, 2), seg.pen * 0.9)))
  }

  // —— 物理 ——
  function stepAct1(now: number, dt: number) {
    const t = now / 1000
    for (const n of act1) {
      n.reached = n.reached || tipY >= n.home[1] - n.h / 2 - 14
      const bob = n.reached ? 0 : 1
      const tx = n.home[0] + (n.fx + Math.sin(t * 0.9 + n.ph) * 10) * bob
      const ty = n.home[1] + (n.fy + Math.cos(t * 0.7 + n.ph) * 8) * bob
      const tr = n.reached ? 0 : n.frot + Math.sin(t * 0.6 + n.ph) * 0.03
      const K = n.reached ? 170 : 40
      const C = n.reached ? 11 : 9
      n.vel[0] += (K * (tx - n.pos[0]) - C * n.vel[0]) * dt
      n.vel[1] += (K * (ty - n.pos[1]) - C * n.vel[1]) * dt
      n.vrot += (K * (tr - n.rot) - C * n.vrot) * dt
      n.pos[0] += n.vel[0] * dt
      n.pos[1] += n.vel[1] * dt
      n.rot += n.vrot * dt
    }
  }

  function stepAct3(now: number, dt: number) {
    if (!hub) return
    const all = [hub, ...sats]
    const t = now / 1000
    const disp = new Map<GNode, Pt>(all.map((n) => [n, [n.pos[0] - n.home[0], n.pos[1] - n.home[1]]]))
    for (const n of all) {
      if (n === drag.node) continue
      // 呼吸的那一块：目标位置自己轻轻浮动
      const bx = n.breathe && !still ? Math.sin(t * 1.7) * 2.2 : 0
      const by = n.breathe && !still ? Math.sin(t * 2.3 + 1) * 3.2 : 0
      let fx = 160 * (bx - disp.get(n)![0])
      let fy = 160 * (by - disp.get(n)![1])
      // 被连着的邻居拽一下
      for (const e of act3Edges) {
        const o = e.a === n ? e.b : e.b === n ? e.a : null
        if (!o) continue
        const d = disp.get(o)!
        const k = o === drag.node ? 55 : 18
        fx += k * (d[0] - disp.get(n)![0])
        fy += k * (d[1] - disp.get(n)![1])
      }
      n.vel[0] += (fx - 9 * n.vel[0]) * dt
      n.vel[1] += (fy - 9 * n.vel[1]) * dt
      n.pos[0] += n.vel[0] * dt
      n.pos[1] += n.vel[1] * dt
      const tr = clamp(n.vel[0] * 0.0006, -0.22, 0.22) + (n.breathe && !still ? Math.sin(t * 1.3) * 0.012 : 0)
      n.vrot += (120 * (tr - n.rot) - 10 * n.vrot) * dt
      n.rot += n.vrot * dt
    }
    if (drag.node) {
      const n = drag.node
      const tr = clamp(drag.v[0] * 0.0005, -0.25, 0.25)
      n.vrot += (140 * (tr - n.rot) - 12 * n.vrot) * dt
      n.rot += n.vrot * dt
    }
    // 连线中间有点「肉」：跟着两头走，但慢半拍，会晃
    for (const e of act3Edges) {
      const tx = (e.a.pos[0] + e.b.pos[0]) / 2
      const ty = (e.a.pos[1] + e.b.pos[1]) / 2
      e.mv[0] += (70 * (tx - e.mid[0]) - 5.5 * e.mv[0]) * dt
      e.mv[1] += (70 * (ty - e.mid[1]) - 5.5 * e.mv[1]) * dt
      e.mid[0] += e.mv[0] * dt
      e.mid[1] += e.mv[1] * dt
    }
  }

  function drawAct3() {
    if (!hub) return
    const p = clamp((tipY - act3.A) / (act3.B - act3.A))
    if (p <= 0) return
    act3Edges.forEach((e, i) => {
      const f = smoothstep(0.18 + i * 0.08, 0.52 + i * 0.08, p)
      if (f <= 0) return
      const a = edgeOf(e.a, e.mid, 6)
      const b = edgeOf(e.b, e.mid, 6)
      const len = dist(a, b) || 1
      const nx = -(b[1] - a[1]) / len
      const ny = (b[0] - a[0]) / len
      const lag: Pt = [e.mid[0] - (e.a.pos[0] + e.b.pos[0]) / 2, e.mid[1] - (e.a.pos[1] + e.b.pos[1]) / 2]
      const c: Pt = [(a[0] + b[0]) / 2 + nx * e.bend + lag[0] * 2.2, (a[1] + b[1]) / 2 + ny * e.bend + lag[1] * 2.2]
      const r = rng(`e${e.seed}`)
      const pts: Stroke = []
      for (let k = 0; k <= 22; k++) {
        const t = k / 22
        pts.push([(1 - t) ** 2 * a[0] + 2 * (1 - t) * t * c[0] + t * t * b[0], (1 - t) ** 2 * a[1] + 2 * (1 - t) * t * c[1] + t * t * b[1]])
      }
      const pp = wobble(densify(pts, 3), 0.8, 0.8 + r() * 2, r() * 6)
      const part = f >= 1 ? pp : sliceRange(pp, 0, f)
      ctx.fillStyle = INK
      if (part.length > 1) ctx.fill(toPath(outlineOf(part, 2.4)))
    })
    drawNode(ctx, hub, smoothstep(0, 0.28, p))
    sats.forEach((n, i) => drawNode(ctx, n, smoothstep(0.34 + i * 0.12, 0.62 + i * 0.12, p)))
  }

  // —— 拖拽 ——
  const dragEl = document.querySelector<HTMLElement>('.act-drag')!
  const hit = (x: number, y: number) => {
    if (!hub || tipY < act3.B) return null
    for (const n of [...sats].reverse().concat(hub)) {
      const p = rot([x - n.pos[0], y - n.pos[1]], -n.rot)
      if (Math.abs(p[0]) < n.w / 2 + 10 && Math.abs(p[1]) < n.h / 2 + 10) return n
    }
    return null
  }
  dragEl.addEventListener('pointerdown', (e) => {
    const n = hit(e.pageX, e.pageY)
    if (!n) return
    e.preventDefault()
    dragEl.setPointerCapture(e.pointerId)
    drag.node = n
    drag.off = [e.pageX - n.pos[0], e.pageY - n.pos[1]]
    drag.last = [e.pageX, e.pageY]
    drag.lastT = performance.now()
    drag.v = [0, 0]
    dragEl.style.cursor = 'grabbing'
    wake()
  })
  dragEl.addEventListener('pointermove', (e) => {
    if (drag.node && dragEl.hasPointerCapture(e.pointerId)) {
      const now = performance.now()
      const dt = Math.max(1, now - drag.lastT) / 1000
      drag.v = [lerp(drag.v[0], (e.pageX - drag.last[0]) / dt, 0.4), lerp(drag.v[1], (e.pageY - drag.last[1]) / dt, 0.4)]
      drag.last = [e.pageX, e.pageY]
      drag.lastT = now
      drag.node.pos = [e.pageX - drag.off[0], e.pageY - drag.off[1]]
      wake()
      return
    }
    dragEl.style.cursor = hit(e.pageX, e.pageY) ? 'grab' : ''
  })
  const release = (e: PointerEvent) => {
    if (!drag.node) return
    if (dragEl.hasPointerCapture(e.pointerId)) dragEl.releasePointerCapture(e.pointerId)
    drag.node.vel = [clamp(drag.v[0], -2500, 2500), clamp(drag.v[1], -2500, 2500)]
    drag.node = null
    dragEl.style.cursor = 'grab'
    wake()
  }
  dragEl.addEventListener('pointerup', release)
  dragEl.addEventListener('pointercancel', release)

  // —— 主循环 ——
  let running = false
  let last = performance.now()
  function wake() {
    if (running) return
    running = true
    last = performance.now()
    requestAnimationFrame(frame)
  }

  const onScreen = (y: number, pad = 300) => y > scrollY - pad && y < scrollY + vh + pad

  function frame(now: number) {
    const dt = Math.min(1 / 30, (now - last) / 1000)
    last = now
    const target = tipTarget()
    if (tipY < 0) tipY = hero.tip()[1]
    if (target > tipY) tipY += (target - tipY) * (1 - Math.exp(-dt / 0.06))
    if (still) tipY = Math.max(tipY, maxDoc + 1)

    for (const seg of segs) {
      const f = seg.pts ? revealStatic(seg, tipY) : clamp((tipY - seg.y0) / Math.max(1, seg.y1 - seg.y0))
      if (f > seg.f) seg.f = f
    }
    stepAct1(now, dt)
    stepAct3(now, dt)

    // 画
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, vw, vh)
    ctx.setTransform(dpr, 0, 0, dpr, -scrollX * dpr, -scrollY * dpr)
    const v0 = scrollY - 200
    const v1 = scrollY + vh + 200
    for (const seg of segs) {
      const ylo = Math.min(seg.y0, seg.pts ? seg.pts[0][1] : seg.y0) - 300
      if (seg.y1 + 300 < v0 || ylo > v1) continue
      drawSeg(seg)
    }
    for (const e of act1Edges) if (e.label) drawPiece(ctx, e.label, clamp((tipY - e.seg.y1) / 90), 0.95)
    for (const o of ops) drawPiece(ctx, o.p, clamp((tipY - o.a) / (o.b - o.a)))
    for (const n of act1) if (onScreen(n.pos[1], 200)) drawNode(ctx, n)
    for (const it of revise) if (onScreen(it.a)) drawPiece(ctx, it.p, clamp((tipY - it.a) / (it.b - it.a)))
    drawAct3()

    // 还在动就继续
    const busy =
      Math.abs(target - tipY) > 0.5 ||
      drag.node !== null ||
      act1.some((n) => (!n.reached && onScreen(n.home[1])) || Math.hypot(n.vel[0], n.vel[1]) > 0.5) ||
      (hub !== null && onScreen(hub.home[1]) && [hub, ...sats].some((n) => n.breathe || Math.hypot(n.vel[0], n.vel[1], n.pos[0] - n.home[0], n.pos[1] - n.home[1]) > 0.3))
    const inRange = scrollY + vh > hero.tip()[1] - 100 && scrollY < maxDoc + vh
    if (busy && inRange) requestAnimationFrame(frame)
    else running = false
  }

  addEventListener('scroll', wake, { passive: true })
  let rt = 0
  addEventListener('resize', () => {
    clearTimeout(rt)
    rt = window.setTimeout(() => {
      layout()
      wake()
    }, 120)
  })
  hero.onLayout(() => {
    layout()
    wake()
  })
  layout()
  wake()
  return { layout }
}
