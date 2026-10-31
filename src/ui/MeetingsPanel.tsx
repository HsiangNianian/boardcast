// 历史会议：最近改过的在前，可以打开、重命名、删除，也可以从这里开一场新会。

import { useEffect, useState } from 'react'
import type { Thumb } from '../board/thumb.ts'
import type { Hit } from '../engine/search.ts'
import { msg, useT } from '../i18n/index.ts'
import { type MeetingInfo, meetings } from '../meetings.ts'
import { Close, Plus } from './icons.tsx'
import { Mini } from './PageStrip.tsx'

interface Props {
  currentId: string
  onClose: () => void
  /** 打开另一场会议（当前这场由调用方先存好） */
  onOpen: (id: string, hit?: Hit) => void
  /** 开一场新会议；`keepCurrent` 为 false 时当前这场刚被删掉，不用再存 */
  onNew: (keepCurrent: boolean) => void
  /** 改了名字：当前这场要同步到界面上 */
  onRenamed: (id: string, title: string) => void
  /** 一场会有哪几张板（缩略图），选底板用 */
  boardsOf: (id: string) => Promise<(Thumb | undefined)[]>
  /** 拿这场会的第 `page` 张板当底板开新会 */
  onContinue: (id: string, page: number) => void
}

const pad = (n: number) => String(n).padStart(2, '0')
function when(t: number): string {
  const d = new Date(t)
  const today = new Date()
  const s = msg().meetings
  const day =
    d.toDateString() === today.toDateString()
      ? s.today
      : s.date(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getFullYear() === today.getFullYear())
  return `${day} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 命中的词高亮 */
function Marked({ hit }: { hit: Hit }) {
  const out: React.ReactNode[] = []
  let at = 0
  hit.marks.forEach(([a, b], i) => {
    out.push(hit.text.slice(at, a), <mark key={i}>{hit.text.slice(a, b)}</mark>)
    at = b
  })
  out.push(hit.text.slice(at))
  return <>{out}</>
}

export default function MeetingsPanel(p: Props) {
  const s = useT().meetings
  const [query, setQuery] = useState('')
  const [found, setFound] = useState<Map<string, Hit[]> | null>(null)
  // 边打字边搜，停一下才真的搜
  useEffect(() => {
    if (!query.trim()) {
      setFound(null)
      return
    }
    let stale = false
    const t = setTimeout(() => {
      void meetings
        .search(query)
        .then((r) => {
          if (!stale) setFound(new Map(r.map((x) => [x.info.id, x.hits])))
        })
        .catch((e) => setError(String(e)))
    }, 180)
    return () => {
      stale = true
      clearTimeout(t)
    }
  }, [query])
  const [list, setList] = useState<MeetingInfo[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [confirming, setConfirming] = useState<string | null>(null)

  const refresh = () =>
    void meetings
      .list()
      .then(setList)
      .catch((e) => setError(String(e)))
  useEffect(refresh, [])

  async function rename(id: string) {
    const title = draft.trim()
    setEditing(null)
    if (!title) return
    const m = await meetings.load(id)
    if (!m) return
    await meetings.save({ ...m, title, titled: true })
    p.onRenamed(id, title)
    refresh()
  }

  /** 接着开会：只有一张板就直接开，有几张先选一张 */
  const [picking, setPicking] = useState<{ id: string; boards: (Thumb | undefined)[] } | null>(null)
  async function continueOn(id: string) {
    const boards = await p.boardsOf(id).catch(() => [undefined])
    if (boards.length > 1) {
      setPicking({ id, boards })
      return
    }
    p.onContinue(id, 0)
    p.onClose()
  }

  async function remove(id: string) {
    setConfirming(null)
    await meetings.remove(id)
    if (id === p.currentId) p.onNew(false)
    refresh()
  }

  return (
    <div className="modal" role="dialog" aria-label={s.title} onClick={(e) => e.target === e.currentTarget && p.onClose()}>
      <div className="panel meetings">
        <header>
          <h2>{s.title}</h2>
          <div className="row">
            <button
              type="button"
              onClick={() => {
                p.onNew(true)
                p.onClose()
              }}
            >
              <Plus size={14} /> {s.newMeeting}
            </button>
            <button type="button" className="icon" onClick={p.onClose} aria-label={s.close}>
              <Close size={16} />
            </button>
          </div>
        </header>
        <p className="hint">{s.hint}</p>
        {error && <p className="meta err">{s.loadFailed(error)}</p>}
        {list && !list.length && <p className="hint">{s.empty}</p>}
        {!!list?.length && (
          <input
            className="meeting-search"
            type="search"
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={s.searchPlaceholder}
            aria-label={s.searchAria}
          />
        )}
        {found && !found.size && <p className="hint">{s.noResults}</p>}
        <ul className="meeting-list">
          {list?.filter((m) => !found || found.has(m.id)).map((m) => (
            <li key={m.id} className={m.id === p.currentId ? 'current' : ''}>
              {editing === m.id ? (
                <input
                  className="title-edit"
                  autoFocus
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onBlur={() => void rename(m.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.nativeEvent.isComposing) (e.target as HTMLInputElement).blur()
                    if (e.key === 'Escape') setEditing(null)
                  }}
                  aria-label={s.titleAria}
                />
              ) : (
                <button
                  type="button"
                  className="open"
                  onClick={() => {
                    if (m.id !== p.currentId) p.onOpen(m.id)
                    p.onClose()
                  }}
                >
                  <span className="title">{m.title}</span>
                  <span className="sub">
                    {when(m.updatedAt)} · {s.lines(m.lines)}{m.id === p.currentId && <em>{s.open}</em>}
                  </span>
                </button>
              )}
              {found?.get(m.id)?.length ? (
                <div className="meeting-hits">
                  {found.get(m.id)!.map((h, i) => (
                    <button
                      key={i}
                      type="button"
                      onClick={() => {
                        p.onOpen(m.id, h)
                        p.onClose()
                      }}
                    >
                      <small>{h.where === 'board' ? s.pageN((h.page ?? 0) + 1) : s.places[h.where]}</small>
                      <span>
                        <Marked hit={h} />
                      </span>
                    </button>
                  ))}
                </div>
              ) : null}
              <div className="ops">
                {confirming === m.id ? (
                  <>
                    <span className="sure">{s.confirmDelete}</span>
                    <button type="button" className="danger" onClick={() => void remove(m.id)}>
                      {s.delete}
                    </button>
                    <button type="button" className="ghost" onClick={() => setConfirming(null)}>
                      {s.cancel}
                    </button>
                  </>
                ) : (
                  <>
                    <button type="button" className="ghost" onClick={() => void continueOn(m.id)} title={s.continueTitle}>
                      {s.continue}
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() => {
                        setDraft(m.title)
                        setEditing(m.id)
                      }}
                    >
                      {s.rename}
                    </button>
                    <button type="button" className="ghost" onClick={() => setConfirming(m.id)}>
                      {s.delete}
                    </button>
                  </>
                )}
              </div>
              {picking?.id === m.id && (
                <div className="pick-board">
                  <span>{s.pickBoard}</span>
                  <div className="boards">
                    {picking.boards.map((b, i) => (
                      <button
                        key={i}
                        type="button"
                        title={s.pageN(i + 1)}
                        aria-label={s.pageN(i + 1)}
                        onClick={() => {
                          p.onContinue(m.id, i)
                          p.onClose()
                        }}
                      >
                        <Mini t={b} />
                        <small>{i + 1}</small>
                      </button>
                    ))}
                  </div>
                  <button type="button" className="ghost" onClick={() => setPicking(null)}>
                    {s.cancel}
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
