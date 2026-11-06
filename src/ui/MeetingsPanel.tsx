// 历史会议：最近改过的在前，可以打开、重命名、删除，也可以从这里开一场新会。

import Dialog from './Dialog.tsx'
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { Thumb } from '../board/thumb.ts'
import { searchMeeting, type Hit, type Searchable } from '../engine/search.ts'
import type { SaveStatus } from '../engine/meetingSave.ts'
import { msg, useT } from '../i18n/index.ts'
import { type MeetingInfo, meetings } from '../meetings.ts'
import { Close, Plus } from './icons.tsx'
import { Mini } from './PageStrip.tsx'

interface Props {
  currentId: string
  current?: { info: MeetingInfo; content: Searchable; state: SaveStatus['state'] }
  /** null means a canceled action, without a failure message. */
  getFailure: () => string | null
  onClose: () => void
  /** 打开另一场会议（当前这场由调用方先存好） */
  onOpen: (id: string, hit?: Hit) => Promise<boolean>
  /** 开一场新会议；`keepCurrent` 为 false 时当前这场刚被删掉，不用再存 */
  onNew: (keepCurrent: boolean) => Promise<boolean>
  onDeleteCurrent: () => Promise<boolean>
  onRename: (id: string, title: string) => Promise<void>
  onReplay: (id: string) => Promise<boolean>
  onExport: (id: string) => Promise<boolean>
  /** 一场会有哪几张板（缩略图），选底板用 */
  boardsOf: (id: string) => Promise<(Thumb | undefined)[]>
  /** 拿这场会的第 `page` 张板当底板开新会 */
  onContinue: (id: string, page: number) => Promise<boolean>
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
  const [query, setQuery] = useState(() => { try { return localStorage.getItem('boardcast.meetings.query') ?? '' } catch { return '' } })
  useEffect(() => { try { localStorage.setItem('boardcast.meetings.query', query) } catch { /* Search remains usable without local storage. */ } }, [query])
  const [found, setFound] = useState<Map<string, Hit[]> | null>(null)
  const [searching, setSearching] = useState(false)
  const [reload, setReload] = useState(0)
  // 边打字边搜，停一下才真的搜
  useEffect(() => {
    setError(null)
    if (!query.trim()) {
      setFound(null)
      setSearching(false)
      return
    }
    let stale = false
    setSearching(true)
    const t = setTimeout(() => {
      void meetings
        .search(query)
        .then((r) => {
          if (!stale) setFound(new Map(r.map((x) => [x.info.id, x.hits])))
        })
        .catch((e) => { if (!stale) setError(String(e)) })
        .finally(() => { if (!stale) setSearching(false) })
    }, 180)
    return () => {
      stale = true
      clearTimeout(t)
    }
  }, [query, reload])
  const [list, setList] = useState<MeetingInfo[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [confirming, setConfirming] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const working = useRef(false)
  const editingTrigger = useRef<HTMLButtonElement | null>(null)
  const editingInput = useRef<HTMLInputElement | null>(null)
  useLayoutEffect(() => {
    if (busy) return
    if (editing) editingInput.current?.focus()
    else {
      editingTrigger.current?.focus()
      editingTrigger.current = null
    }
  }, [editing, busy])
  const readErrors = list?.filter((m) => m.readError) ?? []
  const readable = list?.filter((m) => !m.readError)
  const rows = readable && (p.current ? [p.current.info, ...readable.filter((m) => m.id !== p.currentId)] : readable)
  const hits = useMemo(() => {
    if (!query.trim() || !found) return null
    const result = new Map(found)
    if (p.current) {
      const currentHits = searchMeeting(p.current.content, query)
      if (currentHits) result.set(p.current.info.id, currentHits)
      else result.delete(p.current.info.id)
    }
    return result
  }, [query, found, p.current?.content])

  const refresh = () => setReload((n) => n + 1)
  useEffect(() => {
    let stale = false
    setError(null)
    void meetings
      .list()
      .then((items) => { if (!stale) setList(items) })
      .catch((e) => { if (!stale) setError(String(e)) })
    return () => { stale = true }
  }, [reload])

  async function run(action: () => Promise<void>) {
    if (working.current) return
    working.current = true
    setBusy(true)
    setError(null)
    try { await action() }
    catch (e) { setError(String(e)) }
    finally { working.current = false; setBusy(false) }
  }

  const leave = (action: () => Promise<boolean>) => run(async () => {
    if (await action()) p.onClose()
    else setError(p.getFailure())
  })

  async function rename(id: string) {
    const title = draft.trim()
    if (!title) { setError(s.titleRequired); editingInput.current?.focus(); return }
    await run(async () => {
      await p.onRename(id, title)
      setEditing(null)
      refresh()
    })
  }

  /** 接着开会：只有一张板就直接开，有几张先选一张 */
  const [picking, setPicking] = useState<{ id: string; boards: (Thumb | undefined)[] } | null>(null)
  const pickingTrigger = useRef<HTMLButtonElement | null>(null)
  const picker = useRef<HTMLDivElement | null>(null)
  useEffect(() => { if (picking && !busy) picker.current?.querySelector('button')?.focus() }, [picking, busy])
  async function continueOn(id: string, trigger: HTMLButtonElement) {
    await run(async () => {
      const boards = await p.boardsOf(id)
      if (boards.length > 1) { pickingTrigger.current = trigger; setPicking({ id, boards }) }
      else if (await p.onContinue(id, 0)) p.onClose()
      else setError(p.getFailure())
    })
  }

  async function remove(id: string) {
    await run(async () => {
      if (id === p.currentId) {
        if (!await p.onDeleteCurrent()) { setError(p.getFailure()); return }
      } else await meetings.remove(id)
      setConfirming(null)
      refresh()
    })
  }

  return (
    <Dialog label={s.title} onClose={p.onClose} dismissible={!busy}>
      <div className="panel meetings">
        <header>
          <h2>{s.title}</h2>
          <div className="row">
            <button
              type="button"
              disabled={busy}
              onClick={() => void leave(() => p.onNew(true))}
            >
              <Plus size={14} /> {s.newMeeting}
            </button>
            <button type="button" className="icon" disabled={busy} onClick={p.onClose} aria-label={s.close}>
              <Close size={16} />
            </button>
          </div>
        </header>
        <p className="hint">{s.hint}</p>
        {busy && <p className="meta" role="status">{s.working}</p>}
        {error && <div className="meta err" role="alert"><p>{error}</p><button type="button" className="secondary small" disabled={busy} onClick={refresh}>{s.refresh}</button></div>}
        {!!readErrors.length && <div className="meeting-read-errors meta err">
          <p role="alert">{s.readErrors(readErrors.length)}</p>
          <p>{s.readErrorsHint}</p>
          <details><summary>{s.readErrorDetails}</summary><ul>{readErrors.map((m) => <li key={m.id}>{m.readError}</li>)}</ul></details>
          <button type="button" className="secondary small" disabled={busy} onClick={refresh}>{s.refresh}</button>
        </div>}
        {!list && !error && <p className="hint" role="status">{s.loading}</p>}
        {rows && !rows.length && !readErrors.length && !error && <p className="hint">{s.empty}</p>}
        {!!rows?.length && (
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
        {searching && <p className="hint" role="status">{s.searching}</p>}
        {!searching && !error && !!rows?.length && hits && !hits.size && <p className="hint">{s.noResults}</p>}
        <fieldset className="meeting-items" disabled={busy} aria-busy={busy}>
        <ul className="meeting-list">
          {rows?.filter((m) => !hits || hits.has(m.id)).map((m) => (
            <li key={m.id} className={m.id === p.currentId ? 'current' : ''}>
              {editing === m.id ? (<div className="title-edit-row" onBlur={(e) => {
                if (!e.currentTarget.contains(e.relatedTarget as Node)) void rename(m.id)
              }}>
                <input
                  className="title-edit"
                  ref={editingInput}
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.nativeEvent.isComposing) (e.target as HTMLInputElement).blur()
                    if (e.key === 'Escape' && !e.nativeEvent.isComposing) {
                      e.preventDefault()
                      e.stopPropagation()
                      e.currentTarget.parentElement?.querySelector('button')?.focus()
                      setEditing(null)
                    }
                  }}
                  aria-label={s.titleAria}
                />
                <button type="button" className="small" onClick={() => void rename(m.id)}>{s.save}</button>
                <button type="button" className="ghost small" onPointerDown={(e) => e.preventDefault()} onClick={(event) => { event.currentTarget.focus(); setEditing(null); setError(null) }}>{s.cancel}</button>
              </div>) : (
                <button
                  type="button"
                  className="open"
                  onClick={() => {
                    if (m.id !== p.currentId) void leave(() => p.onOpen(m.id))
                    else p.onClose()
                  }}
                >
                  <span className="title">{m.title}</span>
                  <span className="sub">
                    {when(m.updatedAt)} · {s.lines(m.lines)} · {s.pages(m.pages ?? 1)}
                    {!!m.unfinished && <em>{s.unfinished(m.unfinished)}</em>}
                    {m.id === p.currentId && <em>{s.open}</em>}
                    {m.id === p.currentId && p.current && p.current.state !== 'saved' && <em>{s.unsaved}</em>}
                  </span>
                </button>
              )}
              {hits?.get(m.id)?.length ? (
                <div className="meeting-hits">
                  {hits.get(m.id)!.map((h, i) => (
                    <button
                      key={i}
                      type="button"
                      onClick={() => {
                        void leave(() => p.onOpen(m.id, h))
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
                    <span className="sure">{s.confirmDelete} <b>{m.title}</b><small>{s.deleteNotice}</small></span>
                    <button type="button" className="danger" onClick={() => void remove(m.id)}>
                      {s.delete}
                    </button>
                    <button type="button" className="ghost" onClick={() => setConfirming(null)}>
                      {s.cancel}
                    </button>
                  </>
                ) : (
                  <>
                    <button type="button" className="ghost" onClick={() => void leave(() => p.onReplay(m.id))}>{s.replay}</button>
                    <button type="button" className="ghost" onClick={() => void leave(() => p.onExport(m.id))}>{s.export}</button>
                    <button type="button" className="ghost" onClick={(event) => void continueOn(m.id, event.currentTarget)} title={s.continueTitle} aria-expanded={picking?.id === m.id}>
                      {s.continue}
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={(event) => {
                        editingTrigger.current = event.currentTarget
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
                <div className="pick-board" ref={picker} role="group" aria-label={s.pickBoard}>
                  <span>{s.pickBoard}</span>
                  <div className="boards">
                    {picking.boards.map((b, i) => (
                      <button
                        key={i}
                        type="button"
                        title={s.pageN(i + 1)}
                        aria-label={s.pageN(i + 1)}
                        onClick={() => {
                          void leave(() => p.onContinue(m.id, i))
                        }}
                      >
                        <Mini t={b} />
                        <small>{i + 1}</small>
                      </button>
                    ))}
                  </div>
                  <button type="button" className="ghost" onClick={() => { setPicking(null); pickingTrigger.current?.focus() }}>
                    {s.cancel}
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
        </fieldset>
      </div>
    </Dialog>
  )
}
