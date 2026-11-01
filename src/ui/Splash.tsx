// 启动动画：纯色底上，「boardcast」用板书同一支笔一笔一笔写出来，下面一道细线走完，然后淡出露出白板。
// 字形加载好了才开始计时，免得字还没写完就退场；无论如何最多 3 秒。
// 点一下或按任意键跳过；系统设置了减少动态效果时只做一次淡出。

import { useEffect, useState } from 'react'
import { useT } from '../i18n/index.ts'
import Wordmark from './Wordmark.tsx'

const WRITE_MS = 1200
const HOLD_MS = 450
const EXIT_MS = 320
const MAX_MS = 3000

export default function Splash({ onDone }: { onDone: () => void }) {
  const t = useT()
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
  const [ready, setReady] = useState(false)
  const [leaving, setLeaving] = useState(false)

  // 跳过、兜底：不管字形加载得多慢，3 秒后一定退场
  useEffect(() => {
    const cap = setTimeout(() => setLeaving(true), MAX_MS)
    const skip = () => setLeaving(true)
    addEventListener('keydown', skip)
    return () => {
      clearTimeout(cap)
      removeEventListener('keydown', skip)
    }
  }, [])

  // 字写完再停一会儿
  useEffect(() => {
    if (!ready) return
    const t = setTimeout(() => setLeaving(true), reduced ? 300 : WRITE_MS + HOLD_MS)
    return () => clearTimeout(t)
  }, [ready, reduced])

  useEffect(() => {
    if (!leaving) return
    const t = setTimeout(onDone, EXIT_MS)
    return () => clearTimeout(t)
  }, [leaving, onDone])

  return (
    <div className={`splash${leaving ? ' out' : ''}`} onClick={() => setLeaving(true)} aria-hidden>
      <div className="mark">
        <Wordmark size={64} animate={reduced ? undefined : WRITE_MS} onReady={() => setReady(true)} />
        <span className={`rule${ready ? ' go' : ''}`} style={{ ['--write' as string]: `${WRITE_MS}ms` }} />
        <p>{t.splash}</p>
      </div>
    </div>
  )
}
