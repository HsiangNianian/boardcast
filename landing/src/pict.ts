// Pictograms built from solid blocks and lines. Each part flies in from its own offset and settles when it
// comes into view; hovering greys them, like pressing a button.

import { $$, reduced, svgEl, svgParts } from './util'

export const ICONS: Record<string, string> = {
  board: `<rect class="s" x="9" y="9" width="46" height="46" stroke-width="7" data-from="0,-14"/>`,
  export: `<rect class="f" x="30" y="30" width="26" height="26" data-from="12,12"/><path class="s" d="M8 8 L36 36" stroke-width="7" data-from="-12,-12"/><path class="f" d="M8 8 h22 v8 h-14 v14 h-8z" data-from="-16,-16"/>`,
  replay:
    `<circle class="f" cx="32" cy="32" r="26" data-from="0,0" data-scale="0.4"/>` +
    Array.from({ length: 8 }, (_, i) => {
      const a = -Math.PI / 2 + (i * Math.PI) / 5
      return `<circle class="h" cx="${(32 + Math.cos(a) * 17).toFixed(1)}" cy="${(32 + Math.sin(a) * 17).toFixed(1)}" r="4" data-from="0,0" data-scale="0"/>`
    }).join('') +
    `<circle class="h" cx="32" cy="32" r="5" data-from="0,0" data-scale="0"/>`,
  doc: `<path class="f" d="M10 6 H40 L54 20 V58 H10 Z" data-from="0,14"/><path class="h" d="M40 6 V20 H54" data-from="10,-10"/><rect class="h" x="18" y="30" width="26" height="4" data-from="-20,0"/><rect class="h" x="18" y="40" width="18" height="4" data-from="-20,0"/>`,
  lasso: `<ellipse class="s" cx="34" cy="26" rx="22" ry="16" stroke-width="6" data-from="0,-12"/><rect class="r" x="10" y="40" width="12" height="12" data-from="-12,12"/><path class="s" d="M16 52 V60" stroke-width="5" data-from="0,10"/>`,
  mic: `<rect class="f" x="22" y="4" width="20" height="34" rx="10" data-from="0,-16"/><path class="s" d="M12 28 a20 20 0 0 0 40 0" stroke-width="5" data-from="0,10"/><rect class="f" x="29" y="48" width="6" height="10" data-from="0,14"/><rect class="f" x="18" y="56" width="28" height="5" data-from="0,18"/>`,
  speakers: `<circle class="f" cx="22" cy="24" r="14" data-from="-12,0"/><path class="f" d="M2 60 a20 18 0 0 1 40 0z" data-from="-12,10"/><circle class="s" cx="46" cy="20" r="9" stroke-width="5" data-from="12,0"/><path class="s" d="M34 46 a14 13 0 0 1 28 0" stroke-width="5" data-from="12,10"/>`,
}

export function initPictograms() {
  const parts = (svg: Element) => $$<SVGElement>('[data-from]', svg)
  const seen = new IntersectionObserver(
    (entries) =>
      entries.forEach((e) => {
        if (!e.isIntersecting) return
        e.target.classList.remove('parked')
        parts(e.target).forEach((p, i) => {
          p.style.transitionDelay = `${i * 60}ms`
          requestAnimationFrame(() => (p.style.transform = 'none'))
        })
        seen.unobserve(e.target)
      }),
    { threshold: 0.5 },
  )
  for (const svg of $$<SVGSVGElement>('svg.pict[data-icon]')) {
    svg.replaceChildren(...svgParts(ICONS[svg.dataset.icon ?? 'board']))
    if (reduced) continue
    for (const p of parts(svg)) {
      const [x, y] = (p.dataset.from ?? '0,0').split(',').map(Number)
      p.style.transformOrigin = '32px 32px'
      p.style.transform = `translate(${x}px, ${y}px) scale(${p.dataset.scale ?? 1})`
    }
    svg.classList.add('parked')
    seen.observe(svg)
  }
  // Small ones inside running text.
  for (const i of $$('[data-mini]')) {
    const svg = svgEl('svg', { class: 'pict', viewBox: '0 0 64 64' })
    svg.append(...svgParts(ICONS[i.dataset.mini ?? 'board']))
    i.replaceChildren(svg)
  }
}
