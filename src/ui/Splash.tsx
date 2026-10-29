// 启动动画：带颗粒的彩色云雾里，「boardcast」被一笔一笔写出来，然后整层淡出露出白板。
// 点一下或按任意键跳过；系统设置了减少动态效果时只做一次淡出。

import { useEffect, useState } from 'react'
import Wordmark from './Wordmark.tsx'

const WRITE_MS = 1300
const HOLD_MS = 700
const EXIT_MS = 520

export default function Splash({ onDone }: { onDone: () => void }) {
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
  const [leaving, setLeaving] = useState(false)

  useEffect(() => {
    const t = setTimeout(() => setLeaving(true), reduced ? 400 : WRITE_MS + HOLD_MS)
    const skip = () => setLeaving(true)
    addEventListener('keydown', skip)
    return () => {
      clearTimeout(t)
      removeEventListener('keydown', skip)
    }
  }, [reduced])

  useEffect(() => {
    if (!leaving) return
    const t = setTimeout(onDone, EXIT_MS)
    return () => clearTimeout(t)
  }, [leaving, onDone])

  return (
    <div className={`splash${leaving ? ' out' : ''}`} onClick={() => setLeaving(true)} aria-hidden>
      <div className="clouds">
        <i className="c1" />
        <i className="c2" />
        <i className="c3" />
        <i className="c4" />
      </div>
      <svg className="grain" width="100%" height="100%">
        <filter id="splash-grain">
          <feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="2" stitchTiles="stitch" />
          <feColorMatrix type="saturate" values="0" />
        </filter>
        <rect width="100%" height="100%" filter="url(#splash-grain)" />
      </svg>
      <div className="mark">
        <Wordmark size={72} animate={reduced ? undefined : WRITE_MS} />
        <p>说话，板书自己长出来</p>
      </div>
    </div>
  )
}
