// 历史会议：最近改过的在前，可以打开、重命名、删除，也可以从这里开一场新会。

import { useEffect, useState } from 'react'
import { type MeetingInfo, meetings } from '../meetings.ts'
import { Close, Plus } from './icons.tsx'

interface Props {
  currentId: string
  onClose: () => void
  /** 打开另一场会议（当前这场由调用方先存好） */
  onOpen: (id: string) => void
  /** 开一场新会议；`keepCurrent` 为 false 时当前这场刚被删掉，不用再存 */
  onNew: (keepCurrent: boolean) => void
  /** 改了名字：当前这场要同步到界面上 */
  onRenamed: (id: string, title: string) => void
}

const pad = (n: number) => String(n).padStart(2, '0')
function when(t: number): string {
  const d = new Date(t)
  const today = new Date()
  const day =
    d.toDateString() === today.toDateString()
      ? '今天'
      : d.getFullYear() === today.getFullYear()
        ? `${d.getMonth() + 1} 月 ${d.getDate()} 日`
        : `${d.getFullYear()} 年 ${d.getMonth() + 1} 月 ${d.getDate()} 日`
  return `${day} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export default function MeetingsPanel(p: Props) {
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

  async function remove(id: string) {
    setConfirming(null)
    await meetings.remove(id)
    if (id === p.currentId) p.onNew(false)
    refresh()
  }

  return (
    <div className="modal" role="dialog" aria-label="会议" onClick={(e) => e.target === e.currentTarget && p.onClose()}>
      <div className="panel meetings">
        <header>
          <h2>会议</h2>
          <div className="row">
            <button
              type="button"
              onClick={() => {
                p.onNew(true)
                p.onClose()
              }}
            >
              <Plus size={14} /> 新会议
            </button>
            <button type="button" className="icon" onClick={p.onClose} aria-label="关闭">
              <Close size={16} />
            </button>
          </div>
        </header>
        <p className="hint">每场会议都会自动保存在这台电脑上，关掉应用再打开也能接着开。</p>
        {error && <p className="meta err">读取会议列表失败：{error}</p>}
        {list && !list.length && <p className="hint">还没有保存过的会议。说第一句话后就会自动保存。</p>}
        <ul className="meeting-list">
          {list?.map((m) => (
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
                  aria-label="会议名称"
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
                    {when(m.updatedAt)} · {m.lines} 句{m.id === p.currentId && <em>正在开</em>}
                  </span>
                </button>
              )}
              <div className="ops">
                {confirming === m.id ? (
                  <>
                    <span className="sure">删掉这场会议？</span>
                    <button type="button" className="danger" onClick={() => void remove(m.id)}>
                      删除
                    </button>
                    <button type="button" className="ghost" onClick={() => setConfirming(null)}>
                      取消
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() => {
                        setDraft(m.title)
                        setEditing(m.id)
                      }}
                    >
                      重命名
                    </button>
                    <button type="button" className="ghost" onClick={() => setConfirming(m.id)}>
                      删除
                    </button>
                  </>
                )}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
