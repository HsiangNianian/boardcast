// 首屏右边那块高密度板书，加上破框的那一笔：
// - 黄金中心：粗马克笔写的话题、多层条目和小图标、被两道红线划掉的旧词、弧线指过去的新词、一圈圈的强调和星标；
// - 右上、右下：墨色稍淡的分支，故意伸出画框被裁掉一半；
// - 主箭头：从话题出发横着刺进左栏，红马克笔把巨字里的「自己」重重圈住。
// 颜色只有两种：浓墨黑，和只出现在三处的朱砂红（「自己」的圈、划掉旧词的两道线、圈住新方案的那一圈）。
// 推杆只改变字迹的性格（歪斜、错落、连笔，主箭头和红圈更张扬），板上的东西一样不多、一样不少。
// 粗细推杆只改笔头宽度，字形不动。

import { DEFAULT_MESSY, DEFAULT_WEIGHT } from '../../../src/ink/defaults'
import { type Box, type Piece, type Pt, type Stroke, makePiece, rng } from '../ink/core'
import { measure, penFor, write } from '../ink/hand'
import { arrow, bow, cubic, dashes, head, icon, line, loop, quad, roundRect, strike } from '../ink/shapes'
import type { HeroGeo } from './geo'

export const INK = '#16171b'
export const RED = '#d0392a'
export const FAINT = '#4d5157'

export type Group = 'board' | 'far' | 'arrow' | 'self'

export interface Item {
  p: Piece
  group: Group
  /** 开场写字动画：第几毫秒开始写 */
  delay: number
  /** 属于哪个词块（鼠标碰到、对话提到时整块一起动） */
  block?: string
}

export interface BlockInfo {
  box: Box
  cx: number
  cy: number
}

export interface Board {
  items: Item[]
  /** 主箭头的尖端：往下滚时墨线从这里接着画 */
  tip: Pt
  /** 「自己」的那一圈（没推推杆时的样子） */
  ring: { cx: number; cy: number; rx: number; ry: number }
  blocks: Record<string, BlockInfo>
  topic: Pt
}

class Builder {
  items: Item[] = []
  constructor(
    readonly m: number,
    readonly k: number,
    readonly center: Pt,
    /** 粗细推杆：所有笔画的笔宽倍数 */
    readonly w = 1,
  ) {}

  /** 一段手写字；越潦草，整段也跟着歪一点、挪一点，笔也走得更快、更细 */
  text(id: string, str: string, x: number, y: number, size: number, color: string, group: Group, o: { pen?: number; alpha?: number; block?: string } = {}) {
    const r = rng(`${id}|tilt`)
    const tilt = (r() - 0.5) * 0.16 * this.m
    const dx = (r() - 0.5) * 14 * this.m * this.k
    const dy = (r() - 0.5) * 10 * this.m * this.k
    const w = write(str, x, y, size, id, this.m)
    const cx = x + w.w / 2
    const cy = y + size / 2
    const c = Math.cos(tilt)
    const s = Math.sin(tilt)
    for (const mk of w.marks) mk.pts = mk.pts.map(([px, py]): Pt => [cx + (px - cx) * c - (py - cy) * s + dx, cy + (px - cx) * s + (py - cy) * c + dy])
    const pen = (o.pen ?? penFor(size)) * (1 - 0.16 * this.m)
    const p = makePiece(id, color, w.marks, pen, this.m, { alpha: o.alpha, weight: this.w })
    this.add(p, group, o.block)
    return { w: w.w, p }
  }

  marks(id: string, strokes: Stroke[], pen: number, color: string, group: Group, o: { alpha?: number; wob?: number; block?: string } = {}) {
    const p = makePiece(id, color, strokes, pen, this.m, { ...o, weight: this.w })
    this.add(p, group, o.block)
    return p
  }

  private add(p: Piece, group: Group, block?: string) {
    const cx = (p.box.x0 + p.box.x1) / 2
    const cy = (p.box.y0 + p.box.y1) / 2
    const d = Math.hypot(cx - this.center[0], cy - this.center[1])
    const base = group === 'far' ? 380 : group === 'arrow' ? 820 : group === 'self' ? 1120 : 80
    this.items.push({ p, group, block, delay: base + d * (group === 'far' ? 0.5 : 0.62) })
  }
}

export function compose(g: HeroGeo, m = DEFAULT_MESSY, weight = DEFAULT_WEIGHT): Board {
  const W = g.W
  const bw = W - (g.stacked ? 0 : g.divX)
  const bh = g.baseY - g.boardTop
  const k = Math.max(0.5, Math.min(1.3, bw / 980, bh / 740))
  const l2 = g.lines[1]
  // 话题和「自己」同一高度，主箭头才能横着刺过去
  const ty = g.stacked ? g.boardTop + bh * 0.46 : (l2.inkTop + l2.inkBot) / 2
  const x0 = g.stacked ? W / 2 - 470 * k : g.divX
  const X = (u: number) => x0 + u * k
  const Y = (v: number) => ty + v * k
  const XR = (u: number) => W + u * k
  const YT = (v: number) => g.boardTop + v * k
  const YB = (v: number) => g.baseY + v * k
  const S = (s: number) => s * k

  const topicSize = S(76)
  const topicX = X(300)
  const center: Pt = [topicX + topicSize * 2, ty]
  const b = new Builder(m, k, center, weight)
  const r = rng('board')
  const pen = (s: number) => penFor(S(s))

  // —— 黄金中心 ——
  const topic = b.text('topic', '登录改版', topicX, ty - topicSize / 2, topicSize, INK, 'board', { pen: penFor(topicSize) * 1.15, block: 'topic' })
  const tR = topicX + topic.w
  b.marks('ul1', [line([topicX - S(10), Y(48)], [tR + S(8), Y(52)], r, 0.01)], penFor(topicSize) * 0.62, INK, 'board', { block: 'topic' })
  b.marks('ul2', [line([topicX + S(6), Y(60)], [tR - S(10), Y(64)], r, 0.01)], penFor(topicSize) * 0.5, INK, 'board', { block: 'topic' })
  b.text('star1', '★', tR + S(10), Y(-90), S(42), INK, 'board', { pen: pen(42) * 1.05, block: 'topic' })

  // 往上：旧词被两道红线划掉，饱满的弧线指向新词，新词被红笔圈住
  b.marks('c-up', [bow([X(440), Y(-48)], [X(420), Y(-128)], S(5))], pen(22), INK, 'board')
  const sms = b.text('sms', '短信验证码', X(330), Y(-178), S(34), INK, 'board', { block: 'sms' })
  b.marks('sms-x1', [strike(X(330), Y(-161), sms.w, S(3), r)], pen(36) * 1.05, RED, 'board', { block: 'sms' })
  b.marks('sms-x2', [strike(X(336), Y(-151), sms.w - S(8), -S(2.5), r)], pen(34), RED, 'board', { block: 'sms' })
  const arcFrom: Pt = [X(330) + sms.w + S(6), Y(-174)]
  const arcTo: Pt = [X(628), Y(-222)]
  b.marks('arc', arrow(quad(arcFrom, [X(560), Y(-280)], arcTo, 22), S(15), r), pen(30), INK, 'board')
  const qr = b.text('qr', '扫码登录', X(642), Y(-240), S(42), INK, 'board', { block: 'qr' })
  b.marks('qr-o', [loop(X(642) + qr.w / 2, Y(-219), qr.w / 2 + S(24), S(38), r, { turns: 1.22, start: 2.6, tilt: -0.06 })], pen(38), RED, 'board', { block: 'qr' })
  b.text('qr-star', '★', X(642) + qr.w + S(18), Y(-284), S(30), INK, 'board', { block: 'qr' })
  b.marks('phone', icon('phone', X(642) + qr.w + S(34), Y(-236), S(38), r), pen(26), INK, 'board', { block: 'qr' })

  // 往右：公式
  const f1 = b.text('f1', '0.04元 × 120万', X(668), Y(-16), S(26), INK, 'board', { block: 'formula' })
  b.text('f2', '= 4.8万 / 月', X(690), Y(22), S(26), INK, 'board', { block: 'formula' })
  b.marks('f-o', [loop(X(668) + f1.w / 2 + S(6), Y(17), f1.w / 2 + S(24), S(46), r, { turns: 1.06, start: 3.3 })], pen(25), INK, 'board', { block: 'formula' })
  b.marks('f-a', arrow(bow([tR + S(14), Y(8)], [X(656), Y(4)], S(2)), S(9), r), pen(22), INK, 'board')

  // 往下：条目
  const l1 = b.text('l1', '↳ 老用户先迁移', X(316), Y(98), S(32), INK, 'board', { block: 'l1' })
  b.marks('db', icon('db', X(316) + l1.w + S(12), Y(94), S(30), r), pen(24), INK, 'board', { block: 'l1' })
  b.text('l2', '↳ 灰度 10% → 50%', X(362), Y(150), S(26), INK, 'board', { block: 'l2' })
  const l3 = b.text('l3', '下周三上线', X(316), Y(206), S(34), INK, 'board', { block: 'l3' })
  const w3 = measure('下周三', S(34))
  b.marks('l3-o', [loop(X(316) + w3 / 2 - S(2), Y(224), w3 / 2 + S(9), S(29), r, { turns: 2.2, start: 3.6, grow: 0.14, tilt: -0.04 })], pen(30), INK, 'board', { block: 'l3' })
  b.text('l3-bang', '！', X(316) + l3.w - S(2), Y(202), S(34), INK, 'board', { block: 'l3' })
  const qq = b.text('q', '谁盯数据？', X(572), Y(200), S(30), INK, 'board', { block: 'q' })
  b.marks('person', icon('person', X(572) + qq.w + S(8), Y(194), S(30), r), pen(24), INK, 'board', { block: 'q' })
  b.marks('c-q', [bow([X(316) + l3.w + S(30), Y(222)], [X(564), Y(216)], -S(3))], pen(20), INK, 'board')

  // —— 出画：只留右上、右下两处，墨色稍淡、被画框裁掉一半 ——
  const far = { alpha: 0.5 }
  const grp = roundRect(XR(-300), YT(-40), S(420), S(170), S(16), r)
  b.marks('grp', dashes(grp, S(12), S(8)), pen(22), FAINT, 'far', far)
  b.text('g1', '下一期', XR(-276), YT(18), S(24), FAINT, 'far', far)
  b.text('g2', '海外手机号', XR(-250), YT(56), S(24), FAINT, 'far', far)
  b.text('g3', '风控规则', XR(-96), YT(92), S(24), FAINT, 'far', far)
  b.marks('g-a', arrow(bow([X(642) + qr.w + S(10), Y(-262)], [XR(-260), YT(132)], S(14)), S(10), r), pen(22), FAINT, 'far', far)
  b.text('bury', '埋点看板', XR(-64), Y(206), S(26), FAINT, 'far', far)
  b.marks('bury-a', dashes(bow([X(572) + qq.w + S(48), Y(214)], [XR(-74), Y(222)], S(6)), S(9), S(7)), pen(20), FAINT, 'far', far)
  const rb = roundRect(XR(-250), YB(-118), S(300), S(64), S(12), r)
  b.marks('rb', dashes(rb, S(10), S(7)), pen(20), FAINT, 'far', far)
  b.text('rb-t', '回滚预案', XR(-226), YB(-102), S(26), FAINT, 'far', far)
  b.marks('out1', [bow([X(572) + qq.w * 0.6, Y(238)], [XR(30), YB(-150)], S(20))], pen(20), FAINT, 'far', far)

  // —— 主箭头：横着刺进左栏，把「自己」圈起来。越潦草，弧度越张扬 ——
  const fs = g.fs
  const s = g.self
  const ring = {
    cx: (s.x0 + s.x1) / 2,
    cy: (s.y0 + s.y1) / 2 + fs * 0.01,
    rx: (s.x1 - s.x0) / 2 + fs * 0.13,
    ry: (s.y1 - s.y0) / 2 + fs * 0.16,
  }
  const rr = rng('ring')
  b.marks(
    'self-o',
    [loop(ring.cx, ring.cy, ring.rx * (1 + 0.04 * m), ring.ry * (1 + 0.12 * m), rr, { turns: 1.16 + 0.34 * m, start: -0.35, tilt: -0.07 - 0.12 * m, grow: 0.07 + 0.1 * m })],
    Math.max(5, fs * 0.075),
    RED,
    'self',
    { wob: 0.6 },
  )
  let tip: Pt
  let shaft: Stroke
  if (g.stacked) {
    // 上下排：从话题右上角起笔，贴着右边的空当往上爬，从右下方戳进那一圈
    const rx = W - 16
    tip = [ring.cx + ring.rx * 0.72, ring.cy + ring.ry * 0.78]
    const a: Pt = [topicX + topic.w + S(10), ty - topicSize * 0.3]
    shaft = [
      ...cubic(a, [a[0] + S(40), a[1] - S(10)], [rx, a[1] - S(40)], [rx, g.boardTop - 10], 20),
      ...cubic([rx, g.boardTop - 10], [rx, g.talk.y0 - 20], [tip[0] + 30, tip[1] + 50], tip, 24).slice(1),
    ]
  } else {
    tip = [ring.cx + ring.rx + fs * 0.03, ring.cy + fs * 0.03]
    const a: Pt = [topicX - S(16), ty + S(4)]
    const span = a[0] - tip[0]
    shaft = cubic(a, [a[0] - span * 0.35, a[1] - S(30) * (1 + 2.6 * m)], [tip[0] + span * 0.3, tip[1] - S(16) * (1 + 2.2 * m)], tip, 44)
  }
  const ar = rng('arrow')
  const arrowPen = penFor(topicSize) * 1.22
  b.marks('arrow', [shaft, ...head(shaft, Math.max(18, S(28)) * (1 + 0.4 * m), ar, 0.5 + 0.14 * m)], arrowPen, INK, 'arrow', { wob: 0.5 })

  // 词块的范围
  const blocks: Record<string, BlockInfo> = {}
  for (const it of b.items) {
    if (!it.block) continue
    const bb = it.p.box
    const cur = blocks[it.block]
    if (!cur) blocks[it.block] = { box: { ...bb }, cx: 0, cy: 0 }
    else {
      cur.box.x0 = Math.min(cur.box.x0, bb.x0)
      cur.box.y0 = Math.min(cur.box.y0, bb.y0)
      cur.box.x1 = Math.max(cur.box.x1, bb.x1)
      cur.box.y1 = Math.max(cur.box.y1, bb.y1)
    }
  }
  for (const bl of Object.values(blocks)) {
    bl.cx = (bl.box.x0 + bl.box.x1) / 2
    bl.cy = (bl.box.y0 + bl.box.y1) / 2
  }

  return { items: b.items, tip, ring, blocks, topic: center }
}
