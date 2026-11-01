// 回放 HTML 里的播放器：只读。几张板来回切，每张板可以从空板开始回放、拖进度、倍速；
// 右边是摘要和会议记录，回放时记录跟着走到当时那一句。

import { useEffect, useMemo, useRef, useState } from 'react'
import { board } from '../board/layout.ts'
import { replayTo } from '../engine/steps.ts'
import InkBoard from '../ink/InkBoard.tsx'
import ReplayBar, { SPEEDS } from '../ui/ReplayBar.tsx'
import { useT } from '../i18n/index.ts'
import type { ReplayData } from './data.ts'

const dateText = (t: number) => {
  const d = new Date(t)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

/** 摘要只用到「### 小节」和「- 条目」「- [ ] 待办」，不必带一整个 Markdown 渲染库 */
function Summary({ text }: { text: string }) {
  return (
    <div className="md">
      {text.split('\n').map((l, i) => {
        if (/^#{1,6}\s/.test(l)) return <h3 key={i}>{l.replace(/^#+\s*/, '')}</h3>
        const todo = /^\s*[-*]\s+\[( |x|X)\]\s*(.*)$/.exec(l)
        if (todo)
          return (
            <p key={i} className="todo">
              {todo[1] === ' ' ? '☐' : '☑'} {todo[2]}
            </p>
          )
        if (/^\s*[-*]\s+/.test(l)) return <p key={i} className="item">• {l.replace(/^\s*[-*]\s+/, '')}</p>
        return l.trim() ? <p key={i}>{l}</p> : null
      })}
    </div>
  )
}

export default function Player({ data }: { data: ReplayData }) {
  const [page, setPage] = useState(0)
  const steps = data.pages[page]?.steps ?? []
  const total = steps.length
  // 一打开先看最终的板书；点播放从空板开始
  const [at, setAt] = useState(total)
  const [playing, setPlaying] = useState(false)
  const [speed, setSpeed] = useState(1)
  const [tidy, setTidy] = useState(0)
  const atRef = useRef(at)
  atRef.current = at

  const graph = useMemo(() => (total ? replayTo(steps, at) : data.pages[page].graph), [steps, total, at, data, page])

  // 放回这张板最终的排版（没有就清空）
  const restore = (i: number) => board.restore(data.pages[i]?.layout)
  // 第一次渲染之前就放回去：白板的排版（子组件的 effect）比这里的 effect 先跑
  useState(() => restore(0))

  function go(i: number) {
    if (i === page) return
    restore(i)
    setPage(i)
    setAt(data.pages[i].steps?.length ?? 0)
    setPlaying(false)
    setTidy((t) => t + 1)
  }
  function seek(n: number) {
    n = Math.max(0, Math.min(total, Math.round(n)))
    if (n < atRef.current) restore(page)
    setAt(n)
  }
  function toggle() {
    if (playing) return setPlaying(false)
    if (at >= total) {
      restore(page)
      setAt(0)
    }
    setPlaying(true)
  }

  // 按当时的节奏播放（停顿压到 0.9–3 秒），再除以倍速
  useEffect(() => {
    if (!playing) return
    if (at >= total) {
      setPlaying(false)
      return
    }
    const gap = at > 0 ? steps[at].t - steps[at - 1].t : 0
    const wait = at === 0 ? 300 : Math.min(Math.max(gap, 900), 3000) / speed
    const t = setTimeout(() => seek(at + 1), wait)
    return () => clearTimeout(t)
    // seek 读的是 ref
  }, [playing, at, total, steps, speed])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === ' ') {
        e.preventDefault()
        toggle()
      } else if (e.key === 'ArrowLeft') seek(atRef.current - 1)
      else if (e.key === 'ArrowRight') seek(atRef.current + 1)
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  })

  // 会议记录：回放时只显示到当时那一句；最终状态显示全部
  const shownLines = total && at < total ? data.lines.slice(0, at ? steps[at - 1].line : 0) : data.lines
  const listEnd = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (total && at < total) listEnd.current?.scrollIntoView({ block: 'end' })
  }, [at, total])

  const t = useT()
  const many = data.pages.length > 1
  return (
    <div className="player">
      <header className="topbar">
        <b className="player-title">{data.title}</b>
        <span className="player-meta">{dateText(data.createdAt)} · {t.doc.replay}</span>
        <span className="spacer" />
        {many && (
          <nav className="player-pages" aria-label={t.pages.aria}>
            {data.pages.map((_, i) => (
              <button key={i} type="button" className={i === page ? 'on' : ''} onClick={() => go(i)}>
                {t.doc.boardShort(i + 1)}
              </button>
            ))}
          </nav>
        )}
      </header>
      <main className="board replaying">
        <InkBoard graph={graph} hot={new Set()} dark={false} messy={0.3} bottomInset={84} tidy={tidy} underlay={data.pages[page].underlay} base={data.pages[page].base} />
        {total > 0 ? (
          <ReplayBar
            at={at}
            total={total}
            playing={playing}
            speed={speed}
            elapsed={at ? steps[at - 1].t - steps[0].t : 0}
            duration={steps[total - 1].t - steps[0].t}
            onToggle={toggle}
            onSeek={seek}
            onSpeed={() => setSpeed(SPEEDS[(SPEEDS.indexOf(speed as 1) + 1) % SPEEDS.length])}
            onExit={() => {
              setPlaying(false)
              setAt(total)
            }}
          />
        ) : null}
      </main>
      <aside className="log">
        {data.summary && (
          <section className="summary">
            <div className="summary-head">
              <h3>{t.doc.summary}</h3>
            </div>
            <Summary text={data.summary} />
          </section>
        )}
        <header>
          <h2>{t.doc.transcript}</h2>
          <span className="count">{t.app.count(data.lines.filter((l) => !l.manual).length)}</span>
        </header>
        <div className="lines">
          {shownLines.map((l, i) => (
            <article key={i} className={`line${l.manual ? ' manual' : ''}${total && at < total && i === shownLines.length - 1 ? ' now' : ''}`}>
              {l.manual ? <span className="who edit">{t.app.editBadge}</span> : l.speaker ? <span className="who c0">{l.speaker.slice(0, 2)}</span> : <span />}
              <div className="body">
                <p>{l.text}</p>
              </div>
            </article>
          ))}
          <div ref={listEnd} />
        </div>
      </aside>
    </div>
  )
}
