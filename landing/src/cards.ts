// The cards of "during the meeting": each one shows, small, what the app really does.

import { $, $$, hand, loop, reduced, toD, wait } from './util'

export function initCards() {
  // Circle two things, then link them, over and over.
  const l1 = $<SVGPathElement>('#l1')
  const l2 = $<SVGPathElement>('#l2')
  const lk = $<SVGPathElement>('#lk')
  l1.setAttribute('d', toD(loop(160, 60, 120, 34, 5)))
  l2.setAttribute('d', toD(loop(380, 118, 64, 30, 8)))
  lk.setAttribute('d', toD(hand([[270, 82], [330, 106]], 3, 4)))
  ;(async () => {
    if (reduced) return
    for (;;) {
      for (const p of [l1, l2, lk]) {
        p.style.transition = 'none'
        p.style.strokeDasharray = '1'
        p.style.strokeDashoffset = '1'
        p.style.opacity = '1'
      }
      l1.getBoundingClientRect()
      await wait(300)
      l1.style.transition = 'stroke-dashoffset .9s ease'
      l1.style.strokeDashoffset = '0'
      await wait(900)
      l2.style.transition = 'stroke-dashoffset .8s ease'
      l2.style.strokeDashoffset = '0'
      await wait(1000)
      lk.style.transition = 'stroke-dashoffset .45s ease'
      lk.style.strokeDashoffset = '0'
      await wait(600)
      l1.style.transition = l2.style.transition = 'opacity .6s'
      l1.style.opacity = l2.style.opacity = '0'
      await wait(2600)
    }
  })()

  // Voices take turns.
  for (const w of $$('.voice .wave')) {
    for (let i = 0; i < 14; i++) {
      const b = document.createElement('i')
      b.style.animationDelay = `${(i * 0.11) % 0.7}s`
      w.appendChild(b)
    }
  }
  const voices = $$('.voice')
  let who = 0
  if (!reduced) {
    setInterval(() => {
      who = (who + 1) % voices.length
      voices.forEach((v, i) => v.classList.toggle('quiet', i !== who))
    }, 1700)
  }

  // Replay: a scrubber walks along the meeting's speech.
  const bars = $('#bars')
  const label = $('#bt')
  const n = 44
  for (let i = 0; i < n; i++) {
    const b = document.createElement('i')
    b.style.height = `${16 + Math.abs(Math.sin(i * 1.7) * 66 + Math.cos(i * 0.6) * 18)}%`
    bars.appendChild(b)
  }
  let pos = 8
  const step = () => {
    pos = (pos + 1) % n
    ;[...bars.children].forEach((b, i) => b.classList.toggle('past', i <= pos))
    const s = Math.round((pos / (n - 1)) * 2910)
    label.textContent = `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
  }
  step()
  if (!reduced) setInterval(step, 240)
}
