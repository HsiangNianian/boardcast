// One meeting as four lines on a timeline: the sound, who spoke, what went on the board, and the marks.
// A cursor walks the meeting by itself until the reader points at it, and shows what was said then.

import { LINES, MEETING, clockAt } from './meeting'
import { $, clamp, el, reduced, svgEl } from './util'

export function initSignals() {
  const X = (sec: number) => (sec / MEETING) * 1000
  const span = (l: (typeof LINES)[number]) => [l.t - 4, l.t + 2 + l.say.length * 0.45]
  const talking = (sec: number) => LINES.some((l) => sec >= span(l)[0] && sec <= span(l)[1])
  const flat = { 'vector-effect': 'non-scaling-stroke' }

  // Voice: a fine waveform, loud where someone speaks.
  let d = ''
  for (let i = 0; i <= 1000; i += 2) {
    const amp = talking((i / 1000) * MEETING) ? 18 + 14 * Math.abs(Math.sin(i * 0.7)) : 2 + Math.random() * 2
    d += `${i ? 'L' : 'M'}${i} ${46 + Math.sin(i * 1.9) * amp * (Math.random() * 0.6 + 0.4)} `
  }
  svgEl('path', { d, class: 'line draw-in', pathLength: 1, ...flat }, $('#sig-voice'))

  // Who: one thin track per voice.
  const who = $('#sig-who')
  ;['A', 'B', 'C'].forEach((w, row) => {
    const y = 22 + row * 24
    svgEl('text', { x: -2, y: y + 3, 'text-anchor': 'end', class: 'pop' }, who).textContent = w
    svgEl('line', { x1: 0, x2: 1000, y1: y, y2: y, stroke: '#262626', ...flat }, who)
    for (const l of LINES.filter((l) => l.who === w)) {
      const [a, b] = span(l)
      svgEl('line', { x1: X(a), x2: X(b), y1: y, y2: y, class: 'seg pop', stroke: '#f2f2f2', ...flat }, who)
    }
  })

  // Board: a tick each time ink went down, with what was written.
  const board = $('#sig-board')
  const written: [number, string][] = [[34, 'checklist'], [112, 'regression'], [150, 'ship friday'], [197, 'canary 10→50%'], [342, 'next tuesday'], [630, 'rollback?']]
  written.forEach(([t, w], i) => {
    svgEl('line', { x1: X(t), x2: X(t), y1: 26, y2: 76, class: 'tick pop', ...flat }, board)
    svgEl('text', { x: X(t) + 6, y: i % 2 ? 70 : 34, class: 'pop' }, board).textContent = w
  })

  // Marks, made by hand, in the accent.
  const marks = $('#sig-marks')
  const made: [number, string, number][] = [[340, 'strike · ship friday', 38], [470, 'circle · canary', 38], [478, 'star', 70]]
  for (const [t, w, y] of made) {
    svgEl('rect', { x: X(t) - 3, y: 43, width: 6, height: 6, class: 'mark pop', transform: `rotate(45 ${X(t)} 46)` }, marks)
    svgEl('text', { x: X(t) + 9, y, class: 'mark-label pop' }, marks).textContent = w
  }

  const sig = $('#sig')
  const cursor = $('#sig-cursor')
  const said = $('#sig-said')
  const time = $('#sig-time')
  const track = () => $('.row svg', sig).getBoundingClientRect()
  function show(fx: number) {
    const rows = track()
    const f = clamp(fx)
    const sec = f * MEETING
    cursor.style.left = `${rows.left - sig.getBoundingClientRect().left + f * rows.width}px`
    $('.t', cursor).textContent = clockAt(sec)
    time.textContent = clockAt(sec)
    const last = [...LINES].reverse().find((l) => l.t - 4 <= sec)
    said.replaceChildren(el('b', '', last ? last.who : '—'), last ? last.say : 'nobody has spoken yet')
  }
  sig.addEventListener('pointermove', (e) => {
    const r = track()
    sig.classList.remove('auto')
    show((e.clientX - r.left) / r.width)
  })
  const seen = new IntersectionObserver(
    ([e]) => {
      if (!e.isIntersecting) return
      seen.disconnect()
      sig.classList.add('in')
      if (reduced) return
      // Until someone points, the cursor walks the meeting by itself.
      sig.classList.add('auto')
      let f = 0
      const walk = setInterval(() => {
        if (!sig.classList.contains('auto')) return clearInterval(walk)
        f = (f + 0.004) % 1
        show(f)
      }, 30)
    },
    { threshold: 0.4 },
  )
  seen.observe(sig)
}
