// Circle anything on the page, as in the app: drag a loop around a word and the page goes to its sheet.
// Mouse and pen only, since a finger dragging on a phone means scrolling.

import { $, $$, type Pt, encloses, reduced, svgEl, toD } from './util'

const SKIP = 'a, button, input, kbd, .signals, .card, .dot-card, .app, .titleblock'

export function initLasso() {
  const layer = $<SVGSVGElement>('#lasso-layer')
  const toast = $('#toast')
  let pts: Pt[] | null = null
  let path: SVGPathElement | null = null
  let hideToast = 0

  function say(html: string) {
    toast.innerHTML = html
    toast.classList.add('on')
    clearTimeout(hideToast)
    hideToast = window.setTimeout(() => toast.classList.remove('on'), 2600)
  }

  addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.pointerType === 'touch' || (e.target as Element).closest(SKIP)) return
    e.preventDefault()
    pts = [[e.clientX, e.clientY]]
    path = svgEl('path', {}, layer)
    document.body.classList.add('drawing-lasso')
  })
  addEventListener('pointermove', (e) => {
    if (!pts || !path) return
    pts.push([e.clientX, e.clientY])
    path.setAttribute('d', toD(pts))
  })
  addEventListener('pointerup', () => {
    if (!pts || !path) return
    document.body.classList.remove('drawing-lasso')
    const shape = pts
    const drawn = path
    pts = null
    path = null
    const finish = () => {
      drawn.classList.add('out')
      setTimeout(() => drawn.remove(), 700)
    }
    const xs = shape.map((p) => p[0])
    const ys = shape.map((p) => p[1])
    if (Math.max(...xs) - Math.min(...xs) < 24 || Math.max(...ys) - Math.min(...ys) < 16) return finish()
    drawn.setAttribute('d', toD([...shape, shape[0]]))
    const hit = $$('[data-circle]').find((el) => {
      const r = el.getBoundingClientRect()
      return encloses(shape, [r.left + r.width / 2, r.top + r.height / 2])
    })
    const now = $('#tb-time').textContent
    if (!hit) {
      say(`<b>you</b>${now} · nothing circled. try a word in white.`)
      setTimeout(finish, 500)
      return
    }
    const [sel, frac] = hit.dataset.circle!.split('@')
    const target = $(sel)
    const name = hit.dataset.word ?? hit.textContent?.trim()
    say(`<b>you</b>${now} · circled “${name}” → sheet ${(target.dataset.sheet ?? '').split(' ')[0]}`)
    setTimeout(() => {
      finish()
      const y = target.getBoundingClientRect().top + scrollY + (frac ? (target.offsetHeight - innerHeight) * Number(frac) : -40)
      scrollTo({ top: y, behavior: reduced ? 'auto' : 'smooth' })
    }, 650)
  })
}
