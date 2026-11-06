// 一场会的几张板：白板左上角一排缩略图，点一下切过去，最后一个「+」新开一张。
// 板上东西多了，旁边冒出一句「新开一张？」的建议。

import { useEffect, useRef } from 'react'
import type { Thumb } from '../board/thumb.ts'
import { useT } from '../i18n/index.ts'
import { Plus } from './icons.tsx'

export function Mini({ t }: { t?: Thumb }) {
  if (!t) return <svg viewBox="0 0 160 100" aria-hidden />
  // 留一点边，免得贴着缩略图的边框
  const pad = 8
  return (
    <svg viewBox={`${-pad} ${-pad} ${t.w + 2 * pad} ${t.h + 2 * pad}`} preserveAspectRatio="xMidYMid meet" aria-hidden>
      {t.lines.map((pts, i) => (
        <polyline key={`l${i}`} points={pts.map((p) => p.join(',')).join(' ')} fill="none" strokeWidth={1} />
      ))}
      {t.boxes.map(([x, y, w, h], i) => (
        <rect key={`b${i}`} x={x} y={y} width={w} height={h} rx={1.5} fill="none" strokeWidth={1.4} />
      ))}
      {t.rows.map(([x, y, w, head], i) => (
        <line key={`r${i}`} x1={x} y1={y + 3} x2={x + w} y2={y + 3} strokeWidth={head ? 3.2 : 1.8} strokeLinecap="round" className={head ? 'head' : ''} />
      ))}
    </svg>
  )
}

export default function PageStrip(p: {
  thumbs: (Thumb | undefined)[]
  sourcePages: (number | undefined)[]
  current: number
  onGo: (i: number) => void
  onNew: () => void
  /** 现在不能翻页的原因（正在保存切换、正在回放） */
  blocked?: string
  /** 建议新开一张板 */
  suggest: boolean
  onDismiss: () => void
}) {
  const s = useT().pages
  // 板多了这一排要横着滚：切到哪张，就把哪张滚进来
  const nav = useRef<HTMLElement>(null)
  useEffect(() => {
    const strip = nav.current
    const page = strip?.querySelector<HTMLElement>('.page.on')
    if (!strip || !page) return
    if (page.offsetLeft < strip.scrollLeft) strip.scrollLeft = page.offsetLeft
    else if (page.offsetLeft + page.offsetWidth > strip.scrollLeft + strip.clientWidth)
      strip.scrollLeft = page.offsetLeft + page.offsetWidth - strip.clientWidth
    if (page.offsetTop < strip.scrollTop) strip.scrollTop = page.offsetTop
    else if (page.offsetTop + page.offsetHeight > strip.scrollTop + strip.clientHeight)
      strip.scrollTop = page.offsetTop + page.offsetHeight - strip.clientHeight
  }, [p.current, p.thumbs.length])
  return (
    <nav className="pages" aria-label={s.aria} ref={nav}>
      <select className="page-picker" aria-label={s.aria} value={p.current} onChange={(e) => p.onGo(Number(e.target.value))} disabled={!!p.blocked} title={p.blocked}>
        {p.thumbs.map((_, i) => <option key={i} value={i}>{s.board(i + 1)} / {p.thumbs.length}{s.sourcePage(p.sourcePages[i]) && ` · ${s.sourcePage(p.sourcePages[i])}`}</option>)}
      </select>
      {p.thumbs.map((t, i) => {
        const source = s.sourcePage(p.sourcePages[i])
        return (
        <button
          key={i}
          type="button"
          className={`page${i === p.current ? ' on' : ''}${source ? ' has-source' : ''}`}
          onClick={() => p.onGo(i)}
          disabled={!!p.blocked && i !== p.current}
          aria-current={i === p.current ? 'page' : undefined}
          aria-label={s.board(i + 1)}
          aria-description={source || undefined}
          title={p.blocked && i !== p.current ? p.blocked : `${s.board(i + 1)}${source ? ` · ${source}` : ''}`}
        >
          <Mini t={t} />
          <span className="page-number">{i + 1}</span>
          {source && <small className="page-source">{s.sourcePageShort(p.sourcePages[i]!)}</small>}
        </button>
      )})}
      <button type="button" className="page new" onClick={p.onNew} disabled={!!p.blocked} title={p.blocked ?? s.newTitle} aria-label={s.new}>
        <Plus size={16} />
      </button>
      {p.suggest && !p.blocked && (
        <div className="suggest" role="status">
          <span>{s.crowded}</span>
          <button type="button" className="small" onClick={p.onNew}>
            {s.newShort}
          </button>
          <button type="button" className="ghost small" onClick={p.onDismiss}>
            {s.dismiss}
          </button>
        </div>
      )}
    </nav>
  )
}
