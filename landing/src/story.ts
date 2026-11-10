// The app, pinned while five chapters scroll past: listen, draw, correct, circle, replay. The board is
// drawn from the scroll position, so scrolling back erases it. Until the reader scrolls, it plays itself.

import { LINES, MEETING, clockAt } from './meeting'
import { $, $$, type Pt, clamp, el, hand, loop, reduced, star, svgEl, toD } from './util'

interface Part {
  node: SVGPathElement | SVGRectElement
  kind: 'stroke' | 'word'
  from: number
  to: number
  w?: number
  fade?: [number, number]
}
interface Item {
  x: number
  y: number
  w: number
}

// When each line starts typing, and when it has reached the board, in story progress.
const LINE_AT = [0.03, 0.09, 0.15, 0.41, 0.685, 0.76]
const DRAWN_AT = [0.25, 0.35, 0.4, 0.6, 0.74, 0.8]

export function initStory() {
  const svg = $<SVGSVGElement>('#board')
  const parts: Part[] = []
  const items: Record<string, Item> = {}
  let clipId = 0

  function stroke(pts: Pt[], cls: string, from: number, to: number) {
    const node = svgEl('path', { d: toD(pts), class: `stroke ${cls}`, pathLength: 1 }, svg)
    node.style.strokeDasharray = '1'
    const part: Part = { node, kind: 'stroke', from, to }
    parts.push(part)
    return part
  }
  // A word written left to right by widening its clip.
  function word(id: string, x: number, y: number, str: string, size: number, from: number, to: number) {
    const cid = `story-clip-${clipId++}`
    const clip = svgEl('clipPath', { id: cid }, svg)
    const rect = svgEl('rect', { x: x - 4, y: y - size * 1.1, width: 0, height: size * 1.6 }, clip)
    const t = svgEl('text', { x, y, class: 'hand', 'font-size': size, 'clip-path': `url(#${cid})` }, svg)
    t.textContent = str
    const w = t.getBBox().width
    items[id] = { x, y, w }
    parts.push({ node: rect, kind: 'word', w: w + 8, from, to })
  }

  word('t', 48, 70, 'Release checklist', 32, 0.2, 0.25)
  stroke(hand([[44, 82], [48 + items.t.w + 10, 79]], 3, 7), 'ink thick', 0.25, 0.27)
  stroke(hand([[52, 112], [62, 112]], 1, 2), 'ink', 0.27, 0.28)
  word('i1', 76, 120, 'Regression tests', 22, 0.28, 0.31)
  stroke(hand([[52, 162], [62, 162]], 1, 3), 'ink', 0.31, 0.32)
  word('i3', 76, 170, 'Ship Friday', 22, 0.32, 0.35)
  stroke(hand([[52, 212], [62, 212]], 1, 4), 'ink', 0.35, 0.36)
  word('i2', 76, 220, 'Canary 10% → 50%', 22, 0.36, 0.4)
  const i3 = items.i3
  stroke(hand([[i3.x - 6, i3.y - 9], [i3.x + i3.w + 8, i3.y - 6]], 3, 3), 'accent thick', 0.5, 0.515)
  stroke(hand([[i3.x - 4, i3.y - 3], [i3.x + i3.w + 6, i3.y - 1]], 3, 5), 'accent thick', 0.515, 0.53)
  const ax = i3.x + i3.w + 16
  stroke(hand([[ax, i3.y - 8], [ax + 100, i3.y - 8]], 2, 11), 'ink', 0.53, 0.555)
  stroke(hand([[ax + 90, i3.y - 15], [ax + 101, i3.y - 8], [ax + 90, i3.y - 1]], 1, 13), 'ink', 0.555, 0.565)
  word('i4', ax + 116, 170, 'Next Tuesday', 22, 0.565, 0.6)
  const i2 = items.i2
  stroke(loop(i2.x + i2.w / 2, i2.y - 8, i2.w / 2 + 26, 26, 23), 'lasso', 0.62, 0.68).fade = [0.74, 0.76]
  stroke(star(i2.x + i2.w + 26, i2.y - 8, 13), 'accent', 0.7, 0.74)
  word('q', 380, 104, 'Rollback plan?', 22, 0.77, 0.79)
  const q = items.q
  stroke(hand([[q.x - 14, q.y - 34], [q.x + q.w + 14, q.y - 32], [q.x + q.w + 12, q.y + 18], [q.x - 13, q.y + 17], [q.x - 14, q.y - 34]], 2.5, 17), 'ink', 0.79, 0.8)
  // The selection around "Ship Friday" while it is being corrected.
  const sel = svgEl('rect', { x: i3.x - 10, y: i3.y - 28, width: i3.w + 20, height: 40, class: 'sel' }, svg)

  const list = $('#lines')
  const lis = LINES.map((l) => {
    const li = document.createElement('li')
    li.append(el('b', '', `0${l.who}`), el('span'))
    list.appendChild(li)
    return li
  })
  const articles = $$('.chapter article')
  const tabs = $$('.tabs span')
  const meter = $$('#meter i')
  const marks = $('#marks')

  function render(p: number) {
    // In the last chapter the board is replayed: the scrub head drives it back through the meeting.
    const replaying = p >= 0.8 && p < 0.985
    const q = replaying ? 0.2 + 0.6 * clamp((p - 0.82) / 0.15) : p
    for (const part of parts) {
      const k = clamp((q - part.from) / (part.to - part.from))
      if (part.kind === 'word') part.node.setAttribute('width', String(part.w! * k))
      else {
        part.node.style.strokeDashoffset = String(1 - k)
        if (part.fade) part.node.style.opacity = String(0.8 * (1 - clamp((q - part.fade[0]) / (part.fade[1] - part.fade[0]))))
      }
    }
    // The transcript types each line in, newest on top.
    LINES.forEach((l, i) => {
      const k = clamp((q - LINE_AT[i]) / 0.04)
      lis[i].classList.toggle('gone', k <= 0)
      lis[i].querySelector('span')!.textContent = l.say.slice(0, Math.ceil(l.say.length * k))
      lis[i].classList.toggle('live', k > 0 && k < 1)
      lis[i].classList.toggle('drawn', q >= DRAWN_AT[i])
    })
    const ch = Math.min(4, Math.floor(p / 0.2))
    articles.forEach((a, i) => a.classList.toggle('on', i === ch))
    tabs.forEach((t, i) => {
      t.classList.toggle('on', i === ch)
      t.style.setProperty('--k', String(clamp((p - i * 0.2) / 0.2)))
    })
    const typing = LINES.some((_, i) => q > LINE_AT[i] && q < LINE_AT[i] + 0.04)
    const drawing = parts.some((pt) => q > pt.from && q < pt.to)
    const state = $('#state')
    state.textContent = drawing ? 'drawing' : typing ? 'listening' : q > 0.02 ? 'saved' : 'idle'
    state.classList.toggle('hot', drawing)
    const listening = p < 0.8
    const clock = $('#clock')
    clock.textContent = clockAt(clamp(q / 0.8) * MEETING * 0.9)
    clock.classList.toggle('off', !listening)
    $('#mic').classList.toggle('off', !listening)
    const micLabel = $('#mic-label')
    if (micLabel.firstChild?.textContent !== (listening ? 'stop' : 'listen')) micLabel.replaceChildren(listening ? 'stop' : 'listen', el('small', '', 'space'))
    meter.forEach((b, i) => (b.style.height = typing ? `${4 + Math.abs(Math.sin(performance.now() / 90 + i * 1.3)) * 20}px` : '3px'))
    // Correcting: the selection and its bar beside it.
    const correcting = !replaying && q > 0.43 && q < 0.53
    sel.style.opacity = correcting ? '1' : '0'
    marks.classList.toggle('on', correcting)
    $('.m-strike', marks).classList.toggle('hit', q > 0.47)
    const box = svg.getBoundingClientRect()
    const scale = Math.min(box.width / 560, box.height / 230)
    marks.style.left = `${36 + (i3.x + i3.w - 6) * scale}px`
    marks.style.top = `${34 + (i3.y - 62) * scale}px`
    $('#b-lasso').classList.toggle('active', q > 0.6 && q < 0.76)
    $('#replaybar').classList.toggle('on', p >= 0.8)
    $('#b-replay').classList.toggle('active', p >= 0.8)
    const r = clamp((p - 0.82) / 0.15)
    $('#rfill').style.width = `${r * 100}%`
    $('#rhead').style.left = `${r * 100}%`
    $('#rtime').textContent = clockAt(r * MEETING).slice(3)
    $('#coords').textContent = `x 0 / y 0 / 100% · ${replaying ? 'replay' : 'live'}`
  }

  const sec = $('#story')
  const span = () => sec.offsetHeight - innerHeight
  const progress = () => clamp(-sec.getBoundingClientRect().top / span())
  const tick = () => {
    render(reduced ? 1 : progress())
    requestAnimationFrame(tick)
  }
  tick()
  if (!reduced) autoplay(sec, span, progress)
}

// Once the app has pinned and the page is still, play the chapters by scrolling at a reading pace. Any
// wheel, touch, key or click hands the story back to the reader until they leave the section and return.
function autoplay(sec: HTMLElement, span: () => number, progress: () => number) {
  const badge = $('#auto')
  const PLAY = 4200
  const HOLD = 1400
  let playing = false
  let tookOver = false
  let settle = 0
  const pinned = () => {
    const r = sec.getBoundingClientRect()
    return r.top <= 1 && r.bottom >= innerHeight - 1
  }
  function stop(byReader: boolean) {
    if (playing && byReader) tookOver = true
    playing = false
    badge.classList.remove('on')
  }
  function play() {
    playing = true
    badge.classList.add('on')
    const chapterEnd = (p: number) => Math.min(1, (Math.floor(p / 0.2 + 1e-3) + 1) * 0.2)
    let from = progress()
    let to = chapterEnd(from)
    let t0 = performance.now()
    let holding = false
    function frame(now: number) {
      if (!playing) return
      if (!holding) {
        const k = Math.min(1, (now - t0) / (PLAY * Math.max(0.05, (to - from) / 0.2)))
        scrollTo(0, sec.offsetTop + span() * (from + (to - from) * k))
        if (k >= 1) {
          holding = true
          t0 = now
        }
      } else if (now - t0 >= HOLD) {
        if (to >= 1) return stop(false)
        holding = false
        from = to
        to = chapterEnd(to)
        t0 = now
      }
      requestAnimationFrame(frame)
    }
    requestAnimationFrame(frame)
  }
  for (const ev of ['wheel', 'touchstart', 'keydown', 'mousedown']) addEventListener(ev, () => stop(true), { passive: true })
  let lastY = scrollY
  setInterval(() => {
    const still = scrollY === lastY
    lastY = scrollY
    if (!pinned()) {
      if (!playing) tookOver = false
      settle = 0
      return
    }
    if (playing || tookOver || progress() > 0.97) return
    settle = still ? settle + 1 : 0
    if (settle >= 3) play()
  }, 120)
}
