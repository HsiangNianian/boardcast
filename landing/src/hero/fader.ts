// 底座上的推杆：「工整 ↔ 狂草」。带精密刻度，推子帽附近的刻度会被「放大」，像一台仪器上的推子。
// value 是推子帽的位置（立刻跟手）；板书那边用平滑过的值，所以字是被拖着一点点变形的。

import { clamp, fitCanvas } from '../ink/core'

export class Fader {
  value: number
  touched = false
  private ticks: HTMLCanvasElement
  private track: HTMLElement
  private cap: HTMLElement
  private grab = 0
  private drawn = -1
  private tween: { from: number; to: number; t0: number; dur: number }[] = []

  constructor(
    private root: HTMLElement,
    initial: number,
    private onInput: (v: number) => void,
  ) {
    this.value = initial
    this.ticks = root.querySelector('.fader-ticks')!
    this.track = root.querySelector('.fader-track')!
    this.cap = root.querySelector('.fader-cap')!

    root.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return
      e.preventDefault()
      this.touched = true
      this.tween = []
      root.setPointerCapture(e.pointerId)
      root.classList.add('dragging')
      const capR = this.cap.getBoundingClientRect()
      // 按在推子帽上就保持手和帽的相对位置，按在别处就跳过去
      this.grab = e.clientX >= capR.left - 4 && e.clientX <= capR.right + 4 ? e.clientX - (capR.left + capR.width / 2) : 0
      this.set(this.fromX(e.clientX - this.grab))
      root.focus({ preventScroll: true })
    })
    root.addEventListener('pointermove', (e) => {
      if (!root.hasPointerCapture(e.pointerId)) return
      this.set(this.fromX(e.clientX - this.grab))
    })
    const up = (e: PointerEvent) => {
      if (root.hasPointerCapture(e.pointerId)) root.releasePointerCapture(e.pointerId)
      root.classList.remove('dragging')
    }
    root.addEventListener('pointerup', up)
    root.addEventListener('pointercancel', up)
    root.addEventListener('keydown', (e) => {
      const step = e.shiftKey ? 0.1 : 0.02
      const map: Record<string, number> = {
        ArrowLeft: -step,
        ArrowDown: -step,
        ArrowRight: step,
        ArrowUp: step,
        PageDown: -0.1,
        PageUp: 0.1,
      }
      if (e.key in map) this.set(this.value + map[e.key])
      else if (e.key === 'Home') this.set(0)
      else if (e.key === 'End') this.set(1)
      else return
      this.touched = true
      this.tween = []
      e.preventDefault()
    })
    this.layout()
  }

  private fromX(x: number) {
    const r = this.track.getBoundingClientRect()
    return (x - r.left) / r.width
  }

  set(v: number) {
    this.value = clamp(v)
    this.root.setAttribute('aria-valuenow', String(Math.round(this.value * 100)))
    this.onInput(this.value)
  }

  /** 没人碰过的话，自己轻轻推一下再收回来，提示这东西能推 */
  hint(now: number) {
    if (this.touched) return
    const v = this.value
    this.tween = [
      { from: v, to: 0.46, t0: now, dur: 1300 },
      { from: 0.46, to: v, t0: now + 1700, dur: 1100 },
    ]
  }

  layout() {
    this.drawn = -1
    this.update(performance.now())
  }

  update(now: number) {
    if (this.tween.length) {
      const tw = this.tween[0]
      const t = clamp((now - tw.t0) / tw.dur)
      if (now >= tw.t0) {
        const e = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2
        this.set(tw.from + (tw.to - tw.from) * e)
        if (t >= 1) this.tween.shift()
      }
    }
    if (Math.abs(this.drawn - this.value) < 1e-4) return
    this.drawn = this.value
    const w = this.track.clientWidth
    const h = this.track.clientHeight
    this.cap.style.transform = `translateX(${(this.value * w).toFixed(2)}px)`
    const { ctx } = fitCanvas(this.ticks, w, h)
    ctx.clearRect(0, 0, w, h)
    const mid = h / 2
    ctx.font = '500 7.5px "JetBrains Mono Variable", ui-monospace, monospace'
    ctx.textAlign = 'center'
    ctx.textBaseline = 'top'
    for (let i = 0; i <= 100; i++) {
      const x = Math.round((i / 100) * (w - 1)) + 0.5
      const major = i % 10 === 0
      const half = i % 5 === 0
      const near = Math.exp(-(((i / 100 - this.value) / 0.035) ** 2))
      const len = (major ? 9 : half ? 6 : 3.5) * (1 + near * 0.9)
      ctx.globalAlpha = 0.42 + near * 0.58
      ctx.fillStyle = '#15161a'
      ctx.fillRect(x - 0.5, mid - 6 - len, 1, len)
      // 底下一排很短的反向刻度
      if (half) ctx.fillRect(x - 0.5, mid + 6, 1, major ? 4 : 2.5)
      if (major) {
        ctx.globalAlpha = 0.5 + near * 0.5
        ctx.fillText(String(i).padStart(2, '0'), x, mid + 11)
      }
    }
    ctx.globalAlpha = 1
  }
}
