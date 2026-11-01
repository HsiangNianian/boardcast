// 左下角的对话：像旁边一直有人在讨论。前两句已经变淡，最新一句正一字一字冒出来。
// 说完以后，句子里的关键词被手绘虚线框圈住，拉一根细线斜跨过分界线，连到右边板上对应的那个词块，
// 那个词块轻轻浮起一下。鼠标划过任何一句，对应的词块也会浮起来。板上不会因此多出任何东西。

import { DEFAULT_MESSY } from '../../../src/ink/defaults'
import { type Box, type Piece, type Pt, clamp, drawPiece, easeInOut, makePiece, rng } from '../ink/core'
import { bow, cubic, dashes, roundRect } from '../ink/shapes'
import { INK } from './board'
import type { Rect } from './geo'

interface Say {
  who: string
  text: string
  kw: string
  block: string
}

const SCRIPT: Say[] = [
  { who: '阿杰', text: '这次就聊登录改版，别发散。', kw: '登录改版', block: 'topic' },
  { who: '小林', text: '老用户怎么办，得先迁移吧。', kw: '迁移', block: 'l1' },
  { who: 'Mia', text: '先灰度百分之十，没问题再放开。', kw: '灰度', block: 'l2' },
  { who: '阿杰', text: '行，那就下周三上线。', kw: '下周三', block: 'l3' },
  { who: 'Mia', text: '谁来盯数据？我先认领。', kw: '盯数据', block: 'q' },
  { who: '小林', text: '短信一条四分钱，量一大就扛不住。', kw: '四分钱', block: 'formula' },
  { who: '阿杰', text: '那就换成扫码登录。', kw: '扫码登录', block: 'qr' },
]

const CHAR_MS = 62
const MAX_LINES = 3

interface Link {
  box: Piece
  thread: Piece | null
  t0: number
  /** 开始淡出的时间 */
  out: number
}

export interface TalkHost {
  /** 词块在画布上的范围 */
  block(id: string): Box | null
  /** 其他词块（细线要绕开它们，绕不开就不画线） */
  blocks(): [string, Box][]
  /** 让词块浮起来；ms 给了就是一段时间后自己落下 */
  lift(id: string, on: boolean, ms?: number): void
}

export class Talk {
  private idx = 0
  private clock = 0
  private links: Link[] = []
  private typing: { el: HTMLElement; chars: HTMLElement[]; t0: number; say: Say } | null = null
  private nextAt = 0
  private stamp = 14 * 60 + 2

  constructor(
    private el: HTMLElement,
    private frame: HTMLElement,
    private host: TalkHost,
  ) {
    // 鼠标划过哪一句，板上对应的词块就浮起来
    el.addEventListener('pointerover', (e) => {
      const p = (e.target as HTMLElement).closest<HTMLElement>('.say')
      if (p?.dataset.block) host.lift(p.dataset.block, true)
    })
    el.addEventListener('pointerout', (e) => {
      const p = (e.target as HTMLElement).closest<HTMLElement>('.say')
      const to = (e.relatedTarget as HTMLElement | null)?.closest('.say')
      if (p?.dataset.block && to !== p) host.lift(p.dataset.block, false)
    })
  }

  /** 先摆上两句已经说过的，第三句马上开始冒 */
  start(now: number, instant = false) {
    this.el.textContent = ''
    this.idx = 0
    for (let i = 0; i < 2; i++) this.push(now, true)
    this.nextAt = now + (instant ? 0 : 700)
  }

  private time() {
    const h = Math.floor(this.stamp / 60)
    const mm = this.stamp % 60
    this.stamp += 1
    return `${h}:${String(mm).padStart(2, '0')}`
  }

  private push(now: number, done: boolean) {
    const say = SCRIPT[this.idx % SCRIPT.length]
    this.idx++
    // FLIP：旧的几句往上挪的时候是滑上去的
    const before = new Map<Element, number>()
    for (const c of this.el.children) before.set(c, c.getBoundingClientRect().top)

    const p = document.createElement('p')
    p.className = 'say'
    p.dataset.block = say.block
    const who = document.createElement('span')
    who.className = 'who'
    who.textContent = `${this.time()} ${say.who}`
    const txt = document.createElement('span')
    txt.className = 'txt'
    const chars: HTMLElement[] = []
    const at = say.text.indexOf(say.kw)
    let kw: HTMLElement | null = null
    ;[...say.text].forEach((ch, i) => {
      const c = document.createElement('span')
      c.className = done ? 'c on' : 'c'
      c.textContent = ch
      chars.push(c)
      if (i === at) {
        kw = document.createElement('span')
        kw.className = 'kw'
        txt.append(kw)
      }
      if (kw && i >= at && i < at + say.kw.length) kw.append(c)
      else txt.append(c)
    })
    p.append(who, txt)
    this.el.append(p)
    const kids = [...this.el.children] as HTMLElement[]
    while (kids.length > MAX_LINES) kids.shift()!.remove()
    const n = this.el.children.length
    ;[...this.el.children].forEach((c, i) => ((c as HTMLElement).dataset.age = String(n - 1 - i)))
    for (const [c, top] of before) {
      if (!c.isConnected) continue
      const el = c as HTMLElement
      const dy = top - el.getBoundingClientRect().top
      if (!dy) continue
      el.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], { duration: 620, easing: 'cubic-bezier(.2,.7,.2,1)' })
    }
    if (!done) {
      p.classList.add('typing')
      this.typing = { el: p, chars, t0: now, say }
    }
    for (const l of this.links) l.out = Math.min(l.out, now)
  }

  /** 关键词在画布上的位置 */
  private kwRect(p: HTMLElement): Rect | null {
    const kw = p.querySelector('.kw')
    if (!kw) return null
    const f = this.frame.getBoundingClientRect()
    const r = kw.getBoundingClientRect()
    const ox = f.left + this.frame.clientLeft
    const oy = f.top + this.frame.clientTop
    return { x0: r.left - ox, y0: r.top - oy, x1: r.right - ox, y1: r.bottom - oy }
  }

  private link(now: number, say: Say, k: Rect) {
    const r = rng(`talk${this.idx}`)
    const pad = 4
    const box = makePiece(`tb${this.idx}`, INK, dashes(roundRect(k.x0 - pad - 1, k.y0 - pad + 2, k.x1 - k.x0 + pad * 2 + 2, k.y1 - k.y0 + pad * 2 - 3, 5, r), 5, 3.5), 1.4, DEFAULT_MESSY, { gap: 0 })
    let thread: Piece | null = null
    const target = this.host.block(say.block)
    if (target) {
      // 从虚线框的下沿出发，先贴着这一句的下面往右走，过了句子再斜着爬过分界线：不压任何一句话
      const f = this.frame.getBoundingClientRect()
      const tr = this.el.getBoundingClientRect()
      const right = tr.right - f.left - this.frame.clientLeft + 14
      const a: Pt = [(k.x0 + k.x1) / 2, k.y1 + 3]
      const mid: Pt = [right, k.y1 + 9]
      const b: Pt = [target.x0 - 10, clamp(target.y1 - 10, target.y0 + 8, target.y1 - 8)]
      const pts = [
        ...cubic(a, [a[0], a[1] + 7], [a[0] + 40, mid[1]], [a[0] + 60, mid[1]], 10),
        ...cubic([a[0] + 60, mid[1]], [right - 60, mid[1] + 1], [right + 40, mid[1] - 4], [right + 70, mid[1] - 20], 14).slice(1),
        ...bow([right + 70, mid[1] - 20], b, -14, 22).slice(1),
      ]
      // 细线要从别的词块旁边过：会压到别的字，就只圈词、不拉线
      const hits = this.host.blocks().some(([id, bx]) => id !== say.block && pts.some(([x, y]) => x > bx.x0 - 6 && x < bx.x1 + 6 && y > bx.y0 - 6 && y < bx.y1 + 6))
      if (!hits) thread = makePiece(`tt${this.idx}`, INK, [pts], 1.25, DEFAULT_MESSY, { alpha: 0.62 })
    }
    this.links.push({ box, thread, t0: now, out: Infinity })
    this.host.lift(say.block, true, 2400)
  }

  update(now: number) {
    this.clock = now
    const t = this.typing
    if (t) {
      const n = Math.floor((now - t.t0) / CHAR_MS)
      for (let i = 0; i < t.chars.length && i <= n; i++) t.chars[i].classList.add('on')
      if (n >= t.chars.length) {
        t.el.classList.remove('typing')
        const k = this.kwRect(t.el)
        if (k) this.link(now, t.say, k)
        this.typing = null
        this.nextAt = now + 3800
      }
    } else if (now >= this.nextAt && this.nextAt) {
      this.push(now, false)
    }
    this.links = this.links.filter((l) => now - l.out < 700)
  }

  /** 虚线框和细线 */
  draw(ctx: CanvasRenderingContext2D) {
    const now = this.clock
    for (const l of this.links) {
      const t = now - l.t0
      const fade = l.out === Infinity ? 1 : 1 - clamp((now - l.out) / 600)
      drawPiece(ctx, l.box, easeInOut(clamp(t / 320)), fade)
      if (l.thread) drawPiece(ctx, l.thread, easeInOut(clamp((t - 220) / 520)), fade)
    }
  }

  /** 布局变了：按新位置重新连 */
  relayout(now: number) {
    const last = this.el.lastElementChild as HTMLElement | null
    if (!last || this.typing || !this.links.some((l) => l.out === Infinity)) return
    const k = this.kwRect(last)
    const say = SCRIPT[(this.idx - 1) % SCRIPT.length]
    this.links = []
    if (k) this.link(now - 5000, say, k)
  }
}
