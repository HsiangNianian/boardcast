// 白板底部的一条输入栏：[收音 + 实时波形] │ 说话人 · 打字 │ 发送。
// 说话人只占一个小标签：点一下轮换，或者在句首写「@名字 」直接换人（新名字也行）。
// 收起时只剩收音按钮和一行提示；说话时的临时字幕浮在栏上方。

import { useEffect, useRef, useState } from 'react'
import { ArrowUp, ChevronDown, ChevronUp, Mic, Stop } from './icons.tsx'

interface Props {
  /** 发一句话；speaker 是这句的说话人（句首的 @名字 已经拆出来了） */
  onSend: (text: string, speaker: string) => void
  speaker: string
  setSpeaker: (s: string) => void
  /** 已经出现过的说话人，加上默认的几个；顺序决定颜色 */
  speakers: string[]
  // 收音（只有桌面版有）
  canListen: boolean
  listening: boolean
  level: number
  partial: string
  micName?: string
  voiceErr: string | null
  toggleMic: () => void
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
const N = 32

/** 最近一段音量画成一排细竖条，新的在右边 */
function Waveform({ level }: { level: number }) {
  const [hist, setHist] = useState<number[]>(() => Array(N).fill(0))
  useEffect(() => {
    setHist((h) => [...h.slice(1), loud(level)])
  }, [level])
  return (
    <svg className="waveform" viewBox={`0 0 ${N * 3} 24`} aria-hidden>
      {hist.map((v, i) => {
        const h = Math.max(1.5, v * 22)
        return <rect key={i} x={i * 3} y={12 - h / 2} width={1.6} height={h} rx={0.8} />
      })}
    </svg>
  )
}

/** 句首的「@名字 」：换说话人。名字里不能有空格，最长 12 个字 */
export function splitSpeaker(line: string): { speaker?: string; text: string } {
  const m = /^[@＠](\S{1,12})\s+([\s\S]+)$/.exec(line.trim())
  return m ? { speaker: m[1], text: m[2].trim() } : { text: line.trim() }
}

export default function Composer(p: Props) {
  const [open, setOpen] = useState(() => load('boardcast.composer', 'open') === 'open')
  const [text, setText] = useState('')
  const area = useRef<HTMLTextAreaElement>(null)
  const color = (s: string) => `c${Math.max(0, p.speakers.indexOf(s)) % 6}`

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

  // 输入框跟着内容长高，最多四五行
  useEffect(() => {
    const el = area.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 120)}px`
  }, [text, open])

  function send() {
    const { speaker, text: t } = splitSpeaker(text)
    if (!t) return
    const who = speaker ?? p.speaker
    if (speaker && speaker !== p.speaker) p.setSpeaker(speaker)
    p.onSend(t, who)
    setText('')
  }

  // 点说话人标签轮换到下一个
  const nextSpeaker = () => {
    const i = p.speakers.indexOf(p.speaker)
    p.setSpeaker(p.speakers[(i + 1) % p.speakers.length])
  }

  return (
    <div className={`dock ${color(p.speaker)}${open ? '' : ' closed'}`}>
      {p.partial ? (
        <p className="caption" aria-live="polite">
          {p.speaker.trim() && <b>{p.speaker.trim()}</b>}
          {p.partial}
        </p>
      ) : (
        p.voiceErr && <p className="caption err">{p.voiceErr}</p>
      )}

      <div className="bar">
        {p.canListen && (
          <div className={`listen${p.listening ? ' on' : ''}`}>
            <button
              type="button"
              className="mic"
              onClick={p.toggleMic}
              aria-pressed={p.listening}
              aria-label={p.listening ? '停止收音' : '开始收音'}
              title={p.listening ? `正在收音：${p.micName ?? ''}` : '开始收音'}
            >
              {p.listening ? <Stop size={14} /> : <Mic size={17} />}
            </button>
            {p.listening && <Waveform level={p.level} />}
          </div>
        )}

        {open ? (
          <form
            className="write"
            onSubmit={(e) => {
              e.preventDefault()
              send()
            }}
          >
            <button
              type="button"
              className={`who ${color(p.speaker)}`}
              onClick={nextSpeaker}
              title="当前说话人：点一下换下一个，也可以在句首写 @名字"
              aria-label={`说话人 ${p.speaker}，点一下换下一个`}
            >
              {p.speaker.slice(0, 2)}
            </button>
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
              placeholder={p.listening ? '正在听，也可以接着打字' : '说一句会上的话，回车写上白板；@名字 换人'}
              aria-label="说的话"
              autoFocus
            />
          </form>
        ) : (
          <button type="button" className="write idle" onClick={() => setOpen(true)}>
            <span>{p.listening ? `正在听 · ${p.micName ?? ''}` : '说一句会上的话…'}</span>
            <kbd>/</kbd>
          </button>
        )}

        {open && (
          <button type="button" className="send" onClick={send} disabled={!text.trim()} aria-label="写上白板" title="写上白板（回车）">
            <ArrowUp size={16} />
          </button>
        )}

        <button type="button" className="fold" onClick={() => setOpen(!open)} aria-label={open ? '收起' : '展开'} title={open ? '收起' : '展开'}>
          {open ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
        </button>
      </div>
    </div>
  )
}
