// 悬浮在白板下方正中的输入框：收起时是一颗药丸，展开后是可以打字、收音、换说话人、放示例的卡片。

import { useEffect, useRef, useState } from 'react'
import cases from '../../eval/cases.json'
import { ArrowUp, ChevronDown, ChevronUp, Mic, Play, Plus, Stop } from './icons.tsx'

interface Props {
  onSend: (text: string) => void
  speaker: string
  setSpeaker: (s: string) => void
  /** 已经出现过的说话人，加上默认的几个 */
  speakers: string[]
  // 收音（只有桌面版有）
  canListen: boolean
  listening: boolean
  level: number
  partial: string
  micName?: string
  voiceErr: string | null
  toggleMic: () => void
  // 现在是哪个模型在画
  modelLabel: string
  ready: boolean
  // 示例会议
  demo: string
  setDemo: (id: string) => void
  play: () => void
  playing: boolean
}

function load(key: string, fallback: string) {
  try {
    return localStorage.getItem(key) ?? fallback
  } catch {
    return fallback
  }
}

/** 说话时 RMS 大概在 0.02～0.2，放大一点再开根号，小声也看得见 */
const loud = (level: number) => Math.min(1, Math.sqrt(level * 8))

function Wave({ level, on }: { level: number; on: boolean }) {
  const v = on ? loud(level) : 0
  return (
    <span className="wave" aria-hidden>
      {[0.55, 0.85, 1, 0.75, 0.5].map((k, i) => (
        <i key={i} style={{ transform: `scaleY(${(0.18 + 0.82 * v * k).toFixed(2)})` }} />
      ))}
    </span>
  )
}

export default function Composer(p: Props) {
  const [open, setOpen] = useState(() => load('boardcast.composer', 'open') === 'open')
  const [text, setText] = useState('')
  const [adding, setAdding] = useState(false)
  const area = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    try {
      localStorage.setItem('boardcast.composer', open ? 'open' : 'closed')
    } catch {
      /* 存不了就算了 */
    }
  }, [open])

  // 「/」随时把焦点拉回输入框
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement
      if (e.key !== '/' || t.closest('input, textarea, select, [contenteditable]')) return
      e.preventDefault()
      setOpen(true)
      requestAnimationFrame(() => area.current?.focus())
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  }, [])

  // 输入框跟着内容长高，最多五六行
  useEffect(() => {
    const el = area.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 140)}px`
  }, [text, open])

  function send() {
    const t = text.trim()
    if (!t) return
    p.onSend(t)
    setText('')
  }

  const micButton = p.canListen && (
    <button
      type="button"
      className={`mic-btn${p.listening ? ' on' : ''}`}
      onClick={p.toggleMic}
      aria-pressed={p.listening}
      aria-label={p.listening ? '停止收音' : '开始收音'}
      title={p.listening ? '停止收音' : '开始收音'}
      style={{ ['--lv' as string]: loud(p.level).toFixed(2) }}
    >
      {p.listening ? <Stop size={16} /> : <Mic size={18} />}
    </button>
  )

  const caption = p.partial && (
    <div className="caption" aria-live="polite">
      {p.speaker.trim() && <b>{p.speaker.trim()}</b>}
      <span>{p.partial}</span>
    </div>
  )

  if (!open) {
    return (
      <div className="composer closed">
        {caption}
        <div className="pill">
          {micButton || <span className="orb" aria-hidden />}
          <button type="button" className="pill-text" onClick={() => setOpen(true)}>
            {p.listening ? (
              <>
                <Wave level={p.level} on />
                <span>正在听… {p.micName}</span>
              </>
            ) : (
              <span>{p.canListen ? '点麦克风说话，或者打字…' : '说一句话，板书自己长出来…'}</span>
            )}
          </button>
        </div>
        <button type="button" className="tab" onClick={() => setOpen(true)} aria-label="展开输入框">
          <ChevronUp size={14} />
        </button>
      </div>
    )
  }

  return (
    <div className="composer">
      {caption}
      <form
        className="card"
        onSubmit={(e) => {
          e.preventDefault()
          send()
        }}
      >
        <textarea
          ref={area}
          rows={1}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            // 输入法正在选字时的回车不算发送
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault()
              send()
            }
          }}
          placeholder={p.listening ? '正在听，也可以接着打字…' : '说一句会上的话，回车发送'}
          aria-label="说的话"
          autoFocus
        />
        <div className="row-bar">
          {micButton}
          {p.canListen && (
            <span className="listen-state">
              <Wave level={p.level} on={p.listening} />
              <small>{p.voiceErr ? <em>{p.voiceErr}</em> : p.listening ? p.micName : '没在收音'}</small>
            </span>
          )}
          <span className="spacer" />
          <span className={`engine${p.ready ? '' : ' off'}`} title="出板书的模型，在设置里换">
            <i />
            {p.modelLabel}
          </span>
          <button type="submit" className="send" disabled={!text.trim()} aria-label="发送">
            <ArrowUp size={17} />
          </button>
        </div>
      </form>
      <div className="tray">
        <span className="label">说话人</span>
        <div className="chips" role="radiogroup" aria-label="说话人">
          {p.speakers.map((s) => (
            <button
              key={s}
              type="button"
              role="radio"
              aria-checked={p.speaker === s}
              className={p.speaker === s ? 'on' : ''}
              onClick={() => p.setSpeaker(s)}
            >
              {s}
            </button>
          ))}
          {adding ? (
            <input
              className="chip-input"
              autoFocus
              placeholder="名字"
              onBlur={(e) => {
                const v = e.target.value.trim()
                if (v) p.setSpeaker(v)
                setAdding(false)
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.nativeEvent.isComposing) (e.target as HTMLInputElement).blur()
                if (e.key === 'Escape') setAdding(false)
              }}
            />
          ) : (
            <button type="button" className="add" onClick={() => setAdding(true)} aria-label="加一个说话人">
              <Plus size={13} />
            </button>
          )}
        </div>
        <span className="sep" />
        <span className="label">示例</span>
        <select value={p.demo} onChange={(e) => p.setDemo(e.target.value)} disabled={p.playing} aria-label="示例会议">
          {cases.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
        <button type="button" className="play" onClick={p.play} disabled={p.playing || !p.ready} title="把这段示例会议一句句念给白板">
          <Play size={12} />
          {p.playing ? '播放中' : '播放'}
        </button>
      </div>
      <button type="button" className="tab" onClick={() => setOpen(false)} aria-label="收起输入框">
        <ChevronDown size={14} />
      </button>
    </div>
  )
}
