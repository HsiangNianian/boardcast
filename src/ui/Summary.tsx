// 会议摘要：平时按 Markdown 渲染出来看，点「改」切到原文编辑；可以复制 Markdown 原文或者去掉记号的纯文本。
// 模型写的内容不可信：react-markdown 默认不渲染原始 HTML，链接也只认安全的协议。

import { useEffect, useRef, useState } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { plainText } from '../engine/summary.ts'
import { useT } from '../i18n/index.ts'
import { Check, Close, Pen } from './icons.tsx'
import './summary.css'

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
    try {
      t.select()
      if (!document.execCommand('copy')) throw new Error('copy failed')
    } finally {
      t.remove()
    }
  }
}

export default function Summary({
  text,
  onChange,
  status,
  busy,
  onClear,
  onToast,
  pending,
  stale = false,
  canUndoClear = false,
  onUndoClear,
  onUseGenerated,
  onKeepCurrent,
  readOnly = false,
}: {
  text: string
  onChange: (text: string) => void
  /** 标题旁边的小字：进度、哪个模型写的 */
  status: string
  busy: boolean
  onClear: () => void
  onToast: (t: { ok: boolean; text: string }) => void
  pending?: { text: string; stale: boolean }
  stale?: boolean
  canUndoClear?: boolean
  onUndoClear: () => void
  onUseGenerated: () => void
  onKeepCurrent: () => void
  readOnly?: boolean
}) {
  const s = useT().summary
  const [editing, setEditing] = useState(false)
  const [copyFallback, setCopyFallback] = useState<string | null>(null)
  const fallback = useRef<HTMLTextAreaElement>(null)
  useEffect(() => { if (copyFallback !== null) { fallback.current?.focus(); fallback.current?.select() } }, [copyFallback])
  const put = (what: 'md' | 'text', body: string) =>
    copy(body).then(
      () => onToast({ ok: true, text: s.copied(what) }),
      () => { setCopyFallback(body); onToast({ ok: false, text: s.copyFailed }) },
    )

  return (
    <section className="summary" aria-label={s.aria}>
      <div className="summary-head">
        <h3>{s.title}</h3>
        <span role="status">{status}</span>
        {!!text && (
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
              disabled={readOnly}
              onClick={() => setEditing((e) => !e)}
              aria-label={editing ? s.done : s.edit}
              title={editing ? s.done : s.edit}
            >
              {editing ? <Check size={14} /> : <Pen size={14} />}
            </button>
            <button type="button" className="icon" disabled={readOnly} onClick={() => { setEditing(false); onClear() }} aria-label={s.clear} title={s.clear}>
              <Close size={14} />
            </button>
          </>
        )}
      </div>
      {stale && <p className="summary-notice" role="status">{s.stale}</p>}
      {canUndoClear && <p className="summary-notice" role="status">{s.cleared} <button type="button" className="ghost small" disabled={readOnly} onClick={onUndoClear}>{s.undoClear}</button></p>}
      {(text || editing) &&
        (editing ? (
          <textarea aria-label={s.edit} value={text} readOnly={readOnly} onChange={(e) => onChange(e.target.value)} rows={Math.min(14, text.split('\n').length + 1)} spellCheck={false} autoFocus />
        ) : (
          <div className="md" onDoubleClick={() => { if (!readOnly) setEditing(true) }}>
            <Markdown remarkPlugins={[remarkGfm]}>{text}</Markdown>
          </div>
        ))}
      {busy && !!text && <p className="summary-notice">{s.keepingDraft}</p>}
      {pending && <section className="summary-proposal" aria-label={s.generatedDraft}>
        <h4>{s.generatedDraft}</h4>
        <p>{s.chooseDraft}</p>
        {pending.stale && <p className="summary-notice">{s.stale}</p>}
        <div className="md"><Markdown remarkPlugins={[remarkGfm]}>{pending.text}</Markdown></div>
        <div className="summary-actions">
          <button type="button" className="small" disabled={readOnly} onClick={() => { setEditing(false); onUseGenerated() }}>{s.replace}</button>
          <button type="button" className="ghost small" disabled={readOnly} onClick={onKeepCurrent}>{s.keep}</button>
        </div>
      </section>}
      {copyFallback !== null && <div className="summary-copy-fallback" role="alert">
        <p>{s.copyFallback}</p>
        <textarea ref={fallback} aria-label={s.copyFallbackLabel} readOnly value={copyFallback} rows={Math.min(12, copyFallback.split('\n').length + 1)} onFocus={(e) => e.currentTarget.select()} />
        <button type="button" className="ghost small" onClick={() => setCopyFallback(null)}>{s.closeFallback}</button>
      </div>}
    </section>
  )
}
