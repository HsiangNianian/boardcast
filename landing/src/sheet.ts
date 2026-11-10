// The drawing sheet around the page: rulers on two edges and a title block that follows the section in view.
// Also the large clock at the end, which shows the reader's own time.

import { $, $$, el } from './util'

export function initSheet() {
  const top = $('#ruler-top')
  const left = $('#ruler-left')
  'ABCDEFGHIJKL'.split('').forEach((c, i) => {
    const s = document.createElement('span')
    s.textContent = c
    s.style.left = `${((i + 0.5) / 12) * 100}%`
    top.appendChild(s)
  })
  for (let i = 0; i < 8; i++) {
    const s = document.createElement('span')
    s.textContent = String(i + 1)
    s.style.top = `${((i + 0.5) / 8) * 100}%`
    left.appendChild(s)
  }

  // The meeting's clock, running from when the page opened.
  const time = $('#tb-time')
  const t0 = Date.now()
  const base = 14 * 3600 + 2 * 60 + 4
  setInterval(() => {
    const s = base + Math.floor((Date.now() - t0) / 1000)
    time.textContent = [s / 3600, (s / 60) % 60, s % 60].map((v) => String(Math.floor(v)).padStart(2, '0')).join(':')
  }, 1000)

  const n = $('#tb-n')
  const title = $('#tb-title')
  const io = new IntersectionObserver(
    (entries) =>
      entries.forEach((e) => {
        if (!e.isIntersecting) return
        const [no, ...rest] = (e.target as HTMLElement).dataset.sheet!.split(' ')
        if (n.textContent === no) return
        n.replaceChildren(el('span', 'flip', no))
        title.replaceChildren(el('span', 'flip', rest.join(' ')))
      }),
    { rootMargin: '-45% 0px -45% 0px' },
  )
  $$('[data-sheet]').forEach((s) => io.observe(s))

  const now = $('#now')
  const tick = () => {
    const d = new Date()
    now.replaceChildren(String(d.getHours()).padStart(2, '0'), el('span', 'sep', ':'), String(d.getMinutes()).padStart(2, '0'))
  }
  tick()
  setInterval(tick, 5000)
}
