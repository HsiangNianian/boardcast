// The first sheet: a line is said, then a pen writes the headline in stroke order and circles a word.
// The words can be circled to jump to their sheet, and an eraser wipes the sheet to write it again.

import { penDraw, textStrokes } from './pen'
import { $, $$, type Pt, hand, loop, reduced, svgEl, wait } from './util'

const SAID = 'what if the whiteboard wrote itself?'
// Where circling each written word leads.
const LEADS: Record<string, string> = { talk: '#signals', board: '#story', draws: '#story@0.3', 'itself.': '#after' }

export function initHero() {
  const svg = $<SVGSVGElement>('#ink')
  const said = $('#said')
  const host = $('#hero')
  let run = 0

  async function write() {
    const me = ++run
    svg.replaceChildren()
    $$('.hit', host).forEach((h) => h.remove())
    const W = svg.clientWidth
    const H = svg.clientHeight
    svg.setAttribute('viewBox', `0 0 ${W} ${H}`)

    said.textContent = ''
    said.classList.add('typing')
    for (const ch of SAID) {
      if (me !== run) return
      said.textContent += ch
      await wait(32)
    }
    said.classList.remove('typing')
    await wait(250)

    // Two lines sized to the sheet, the second indented as handwriting drifts.
    const unit = textStrokes('board draws itself.', 0, 0, 100).width / 100
    const size = Math.min((W * 0.9) / (unit + 0.5), H * 0.42, 210)
    const x0 = Math.max(0, (W - (unit + 0.5) * size) / 2)
    const y1 = size * 0.95
    const y2 = y1 + size * 1.05
    const l1 = textStrokes('talk, and the', x0, y1, size, 3)
    const l2 = textStrokes('board draws itself.', x0 + size * 0.5, y2, size, 9)
    svg.setAttribute('aria-label', 'talk, and the board draws itself.')
    const speed = Math.max(1.6, size / 52)
    await penDraw(svg, l1.strokes, { cls: 'w big', speed })
    if (me !== run) return
    await penDraw(svg, l2.strokes, { cls: 'w big', speed })
    if (me !== run) return
    const it = l2.words[2]
    await penDraw(svg, [loop(it.x + it.w / 2, y2 - size * 0.3, it.w / 2 + size * 0.22, size * 0.48, 21, 1.14)], { cls: 'w a big', speed: speed * 1.4, lift: 0 })
    if (me !== run) return

    // Invisible targets over the written words, for circling.
    const box = svg.getBoundingClientRect()
    const hb = host.getBoundingClientRect()
    ;[...l1.words, ...l2.words].forEach((w, i) => {
      const word = w.text.replace(',', '')
      const lead = LEADS[word]
      if (!lead) return
      const y = i < l1.words.length ? y1 : y2
      const h = document.createElement('span')
      h.className = 'hit'
      h.dataset.circle = lead
      h.dataset.word = word
      Object.assign(h.style, { left: `${box.left - hb.left + w.x}px`, top: `${box.top - hb.top + y - size * 0.75}px`, width: `${w.w}px`, height: `${size * 0.8}px` })
      host.appendChild(h)
    })

    // A note inviting the reader to try it, with an arrow down to the words that can be circled.
    await wait(300)
    const noteSize = Math.max(18, size * 0.15)
    const note = textStrokes('drag a circle around any word', W - size * 3.2, y2 + size * 0.62, noteSize, 5)
    await penDraw(svg, note.strokes, { cls: 'w note', speed: 1.4, lift: 12 })
    const a: Pt = [W - size * 3.25, y2 + size * 0.58]
    const b: Pt = [a[0] - size * 0.7, a[1] + size * 0.35]
    await penDraw(svg, [hand([a, b], 4, 3), hand([[b[0] + 16, b[1] - 2], b], 1, 4), hand([[b[0] + 4, b[1] - 15], b], 1, 5)], { cls: 'w note', speed: 1.4, lift: 4 })
  }

  // Wipe the sheet with an eraser, then write it again.
  async function again() {
    if (reduced) return write()
    run++
    const eraser = $('#eraser')
    const box = svg.getBoundingClientRect()
    const hb = host.getBoundingClientRect()
    Object.assign(eraser.style, { display: 'block', top: `${box.top - hb.top}px`, height: `${box.height}px` })
    let clip = svg.querySelector('#wipe')
    if (!clip) {
      clip = svgEl('clipPath', { id: 'wipe' })
      svg.prepend(clip)
      svgEl('rect', { y: -2000, height: 6000, width: 10000 }, clip)
    }
    const rect = clip.firstElementChild as SVGRectElement
    $$(':scope > g', svg).forEach((g) => g.setAttribute('clip-path', 'url(#wipe)'))
    $$('.hit', host).forEach((h) => h.remove())
    const t0 = performance.now()
    await new Promise<void>((done) => {
      function frame(now: number) {
        const k = Math.min(1, (now - t0) / 900)
        const x = -140 + (box.width + 280) * (k * k * (3 - 2 * k))
        eraser.style.left = `${box.left - hb.left + x}px`
        // A slight up and down, like a hand scrubbing.
        eraser.style.transform = `translateY(${Math.sin(k * 30) * 6}px)`
        rect.setAttribute('x', String(x + 120))
        if (k < 1) requestAnimationFrame(frame)
        else done()
      }
      requestAnimationFrame(frame)
    })
    eraser.style.display = 'none'
    await wait(200)
    write()
  }

  $('#again').addEventListener('click', again)
  let resizing = 0
  addEventListener('resize', () => {
    clearTimeout(resizing)
    resizing = window.setTimeout(write, 300)
  })
  write()
}
