// What a meeting leaves behind, each drawn as a pictogram made of dots. Pointing at a card scatters the
// dots; leaving gathers them again.

import { ICONS } from './pict'
import { $$, reduced } from './util'

interface Dot {
  x: number
  y: number
  sx: number
  sy: number
  ph: number
}

const GRID = 200
const STEP = 5

export function initDots() {
  for (const cv of $$<HTMLCanvasElement>('.dot-card canvas')) {
    const ctx = cv.getContext('2d')!
    const card = cv.closest('.dot-card')!
    const dots: Dot[] = []
    let W = 0
    let H = 0
    let scatter = 0
    let aim = 0
    let visible = false
    const img = new Image()
    img.src =
      'data:image/svg+xml;utf8,' +
      encodeURIComponent(
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="${GRID}" height="${GRID}"><style>.f{fill:#fff}.s{fill:none;stroke:#fff}.h{fill:#000}.r{fill:#fff}</style><rect width="64" height="64" fill="#000"/>${ICONS[cv.dataset.icon ?? 'board']}</svg>`,
      )
    img.onload = () => {
      const dpr = Math.min(2, devicePixelRatio || 1)
      W = cv.clientWidth
      H = cv.clientHeight
      cv.width = W * dpr
      cv.height = H * dpr
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      const o = document.createElement('canvas').getContext('2d')!
      o.canvas.width = o.canvas.height = GRID
      o.drawImage(img, 0, 0)
      const data = o.getImageData(0, 0, GRID, GRID).data
      const ox = W / 2 - GRID / 2
      const oy = H / 2 - GRID / 2 + 4
      for (let y = 0; y < GRID; y += STEP) {
        for (let x = 0; x < GRID; x += STEP) {
          if (data[(y * GRID + x) * 4] <= 128) continue
          const a = Math.random() * Math.PI * 2
          const r = 60 + Math.random() * 140
          dots.push({ x: ox + x, y: oy + y, sx: Math.cos(a) * r, sy: Math.sin(a) * r, ph: Math.random() * Math.PI * 2 })
        }
      }
      draw(performance.now())
    }
    card.addEventListener('pointerenter', () => (aim = 1))
    card.addEventListener('pointerleave', () => (aim = 0))
    // Only animate while the card is on screen.
    new IntersectionObserver(([e]) => {
      visible = e.isIntersecting
      if (visible) requestAnimationFrame(draw)
    }).observe(cv)
    function draw(now: number) {
      scatter += (aim - scatter) * 0.07
      const e = scatter * scatter * (3 - 2 * scatter)
      ctx.clearRect(0, 0, W, H)
      ctx.fillStyle = '#f2f2f2'
      for (const d of dots) {
        ctx.globalAlpha = (1 - scatter * 0.6) * (reduced ? 1 : 0.55 + 0.45 * Math.sin(now / 600 + d.ph))
        ctx.beginPath()
        ctx.arc(d.x + d.sx * e, d.y + d.sy * e, 1.7, 0, Math.PI * 2)
        ctx.fill()
      }
      if (visible && !reduced) requestAnimationFrame(draw)
    }
  }
}
