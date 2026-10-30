// 品牌字：用板书同一支笔、同一套连写体写出「boardcast」，启动时一笔一笔写出来。

import { useEffect, useState } from 'react'
import { glyph } from '../ink/glyphs.ts'
import { type Inked, ink, placeGlyph, rng } from '../ink/pen.ts'

interface Written {
  strokes: Inked[]
  width: number
  height: number
}

const cache = new Map<string, Promise<Written>>()

function write(text: string, size: number): Promise<Written> {
  const key = `${text}@${size}`
  let p = cache.get(key)
  if (!p) {
    p = (async () => {
      const r = rng(`wordmark:${text}`)
      const pen = Math.max(1.6, size * 0.075)
      const strokes: Inked[] = []
      let x = pen
      for (const ch of text) {
        const g = await glyph(ch, 'script')
        if (!g) continue
        for (const s of placeGlyph(g.strokes, x, pen, size, r, 0.18)) strokes.push(ink(s, pen, r, { messy: 0.1 }))
        x += g.advance * size
      }
      return { strokes, width: x + pen * 2, height: size * 1.3 + pen * 2 }
    })()
    cache.set(key, p)
  }
  return p
}

interface Props {
  size?: number
  /** 一笔一笔写出来；总时长大约这么多毫秒 */
  animate?: number
  className?: string
  /** 字形准备好、开始写的那一刻（启动动画从这时候开始计时） */
  onReady?: () => void
}

export default function Wordmark({ size = 28, animate, className, onReady }: Props) {
  const [w, setW] = useState<Written | null>(null)
  useEffect(() => {
    let alive = true
    void write('boardcast', size).then((x) => {
      if (!alive) return
      setW(x)
      onReady?.()
    })
    return () => {
      alive = false
    }
    // onReady 只在字形第一次准备好时调用，不跟着回调变化重跑
  }, [size])
  if (!w) return <span className={className} style={{ display: 'inline-block', width: size * 4.6, height: size * 1.3 }} />

  // 按笔画长度分配时间，长的一笔写得久一点
  const total = w.strokes.reduce((a, s) => a + s.length, 0) || 1
  let at = 0
  return (
    <svg
      className={className}
      width={w.width}
      height={w.height}
      viewBox={`0 0 ${w.width} ${w.height}`}
      role="img"
      aria-label="boardcast"
      fill="currentColor"
    >
      {w.strokes.map((s, i) => {
        if (!animate) return <path key={i} d={s.d} />
        const dur = (s.length / total) * animate
        const delay = at
        at += dur
        const id = `wm-${size}-${i}`
        return (
          <g key={i}>
            <mask id={id} maskUnits="userSpaceOnUse">
              <path
                d={s.center}
                fill="none"
                stroke="#fff"
                strokeWidth={size * 0.3}
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeDasharray={s.length + 1}
                strokeDashoffset={s.length + 1}
                style={{ animation: `bc-write ${dur.toFixed(0)}ms linear ${delay.toFixed(0)}ms forwards` }}
              />
            </mask>
            <path d={s.d} mask={`url(#${id})`} />
          </g>
        )
      })}
    </svg>
  )
}
