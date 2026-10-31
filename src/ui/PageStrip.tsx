// 一场会的几张板：白板左上角一排缩略图，点一下切过去，最后一个「+」新开一张。
// 板上东西多了，旁边冒出一句「新开一张？」的建议。

import { useEffect, useRef } from 'react'
import type { Thumb } from '../board/thumb.ts'
import { useT } from '../i18n/index.ts'
import { Plus } from './icons.tsx'

function Mini({ t }: { t?: Thumb }) {
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
  current: number
  onGo: (i: number) => void
  onNew: () => void
  /** 现在不能翻页的原因（模型还在画、正在回放） */
  blocked?: string
  /** 建议新开一张板 */
  suggest: boolean
  onDismiss: () => void
}) {
  const s = useT().pages
  // 板多了这一排要横着滚：切到哪张，就把哪张滚进来
  const nav = useRef<HTMLElement>(null)
  useEffect(() => {
    nav.current?.querySelector('.page.on')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [p.current, p.thumbs.length])
  return (
    <nav className="pages" aria-label={s.aria} ref={nav}>
      {p.thumbs.map((t, i) => (
        <button
          key={i}
          type="button"
          className={`page${i === p.current ? ' on' : ''}`}
          onClick={() => p.onGo(i)}
          disabled={!!p.blocked && i !== p.current}
          aria-current={i === p.current ? 'page' : undefined}
          aria-label={s.board(i + 1)}
          title={p.blocked && i !== p.current ? p.blocked : s.board(i + 1)}
        >
          <Mini t={t} />
          <span>{i + 1}</span>
        </button>
      ))}
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
