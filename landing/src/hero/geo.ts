// 量首屏：画框里每一块在哪儿、巨字每一行的墨迹上下沿、「自己」两个字的位置。
// 板书、主箭头、越界的枝杈全都按这些量出来的位置摆，所以换什么屏幕都能对上。

export interface Rect {
  x0: number
  y0: number
  x1: number
  y1: number
}

export interface GiantLine {
  text: Rect
  base: number
  inkTop: number
  inkBot: number
}

export interface HeroGeo {
  /** 画框内部尺寸（画布就这么大） */
  W: number
  H: number
  /** 画框内部左上角在文档里的位置 */
  ox: number
  oy: number
  /** 中间分界线 x（上下排时 = W） */
  divX: number
  /** 板子的上沿（上下排时在巨字下面） */
  boardTop: number
  /** 底座的上沿 */
  baseY: number
  stacked: boolean
  fs: number
  left: number
  lines: GiantLine[]
  self: Rect
  plate: Rect
  mark: Rect
  talk: Rect
}

const ink = document.createElement('canvas').getContext('2d')!

export function measureHero(frame: HTMLElement): HeroGeo {
  const fr = frame.getBoundingClientRect()
  const ox = fr.left + frame.clientLeft
  const oy = fr.top + frame.clientTop
  const rel = (r: DOMRect): Rect => ({ x0: r.left - ox, y0: r.top - oy, x1: r.right - ox, y1: r.bottom - oy })
  const q = <T extends Element>(s: string) => frame.querySelector(s) as T

  const col = q<HTMLElement>('.col-left')
  const colR = rel(col.getBoundingClientRect())
  const stacked = getComputedStyle(frame).gridTemplateColumns.split(' ').length < 2
  const giant = q<HTMLElement>('.giant')
  const cs = getComputedStyle(giant)
  const fs = parseFloat(cs.fontSize)
  ink.font = `900 ${fs}px ${cs.fontFamily}`

  const lines: GiantLine[] = []
  for (const span of frame.querySelectorAll<HTMLElement>('.gl')) {
    let bl = span.querySelector<HTMLElement>('.bl')
    if (!bl) {
      bl = document.createElement('i')
      bl.className = 'bl'
      bl.style.cssText = 'display:inline-block;width:0;height:0;vertical-align:baseline'
      span.append(bl)
    }
    const range = document.createRange()
    range.selectNodeContents(span)
    const text = rel(range.getBoundingClientRect())
    const base = bl.getBoundingClientRect().top - oy
    // 只量汉字的墨迹（标点会往下伸，量进来字缝就算错了）
    const str = (span.textContent ?? '').replace(/[，。、]/g, '')
    const mt = ink.measureText(str)
    lines.push({ text, base, inkTop: base - mt.actualBoundingBoxAscent, inkBot: base + mt.actualBoundingBoxDescent })
  }
  const selfR = rel(q<HTMLElement>('.self').getBoundingClientRect())
  const l2 = lines[1]
  const self: Rect = { x0: selfR.x0, x1: selfR.x1 - fs * 0.045, y0: l2.inkTop, y1: l2.inkBot }

  return {
    W: frame.clientWidth,
    H: frame.clientHeight,
    ox: ox + scrollX,
    oy: oy + scrollY,
    divX: stacked ? frame.clientWidth : colR.x1,
    boardTop: stacked ? colR.y1 : 0,
    baseY: rel(q<HTMLElement>('.base').getBoundingClientRect()).y0,
    stacked,
    fs,
    left: colR.x0 + parseFloat(getComputedStyle(col).paddingLeft),
    lines,
    self,
    plate: rel(q<HTMLElement>('.plate').getBoundingClientRect()),
    mark: rel(q<HTMLElement>('.mark').getBoundingClientRect()),
    talk: rel(q<HTMLElement>('.talk').getBoundingClientRect()),
  }
}
