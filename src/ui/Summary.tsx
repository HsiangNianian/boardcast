// 会议摘要：平时按 Markdown 渲染出来看，点「改」切到原文编辑；可以复制 Markdown 原文或者去掉记号的纯文本。
// 模型写的内容不可信：react-markdown 默认不渲染原始 HTML，链接也只认安全的协议。

import { useState } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { plainText } from '../engine/summary.ts'
import { useT } from '../i18n/index.ts'
import { Check, Close, Pen } from './icons.tsx'

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    // 有些 WebView 没开剪贴板接口：退回老办法
    const t = document.createElement('textarea')
    t.value = text
    t.style.position = 'fixed'
    t.style.opacity = '0'
    document.body.append(t)
    t.select()
    const ok = document.execCommand('copy')
    t.remove()
    if (!ok) throw new Error('copy failed')
  }
}

export default function Summary({
  text,
  onChange,
  status,
  busy,
  onClear,
  onToast,
}: {
  text: string
  onChange: (text: string) => void
  /** 标题旁边的小字：进度、哪个模型写的 */
  status: string
  busy: boolean
  onClear: () => void
  onToast: (t: { ok: boolean; text: string }) => void
}) {
  const s = useT().summary
  const [editing, setEditing] = useState(false)
  const put = (what: 'md' | 'text', body: string) =>
    copy(body).then(
      () => onToast({ ok: true, text: s.copied(what) }),
      () => onToast({ ok: false, text: s.copyFailed }),
    )

  return (
    <section className="summary" aria-label={s.aria}>
      <div className="summary-head">
        <h3>{s.title}</h3>
        <span>{status}</span>
        {!busy && (
          <>
            <button type="button" className="ghost small" onClick={() => void put('md', text)} title={s.copyMdTitle}>
              {s.copyMd}
            </button>
            <button type="button" className="ghost small" onClick={() => void put('text', plainText(text))} title={s.copyTextTitle}>
              {s.copyText}
            </button>
            <button
              type="button"
              className="icon"
              onClick={() => setEditing((e) => !e)}
              aria-label={editing ? s.done : s.edit}
              title={editing ? s.done : s.edit}
            >
              {editing ? <Check size={14} /> : <Pen size={14} />}
            </button>
            <button type="button" className="icon" onClick={onClear} aria-label={s.clear} title={s.clear}>
              <Close size={14} />
            </button>
          </>
        )}
      </div>
      {!busy &&
        (editing ? (
          <textarea value={text} onChange={(e) => onChange(e.target.value)} rows={Math.min(14, text.split('\n').length + 1)} spellCheck={false} autoFocus />
        ) : (
          <div className="md" onDoubleClick={() => setEditing(true)}>
            <Markdown remarkPlugins={[remarkGfm]}>{text}</Markdown>
          </div>
        ))}
    </section>
  )
}
