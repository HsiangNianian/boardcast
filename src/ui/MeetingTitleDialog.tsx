import { useLayoutEffect, useRef, useState } from 'react'
import { useT } from '../i18n/index.ts'
import Dialog from './Dialog.tsx'

export default function MeetingTitleDialog({ title, onSave, onClose }: {
  title: string; onSave: (title: string) => Promise<void>; onClose: () => void
}) {
  const s = useT().meetings
  const [draft, setDraft] = useState(title)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const working = useRef(false)
  const input = useRef<HTMLInputElement>(null)
  const submit = useRef<HTMLButtonElement>(null)
  useLayoutEffect(() => { if (!busy) input.current?.focus() }, [busy])

  function close() {
    if (working.current) return
    // End native IME editing before removing its focused input.
    submit.current?.focus()
    onClose()
  }

  async function save() {
    if (working.current) return
    const next = draft.trim()
    if (!next) { setError(s.titleRequired); input.current?.focus(); return }
    submit.current?.focus()
    working.current = true
    setBusy(true)
    setError(null)
    try { await onSave(next); onClose() }
    catch (e) { setError(String(e)) }
    finally { working.current = false; setBusy(false) }
  }

  return <Dialog label={s.renameTitle} onClose={close} dismissible={!busy}>
    <form className="panel meeting-title-dialog" onSubmit={(event) => { event.preventDefault(); void save() }}>
      <h2>{s.renameTitle}</h2>
      <label className="field">{s.titleAria}<input ref={input} value={draft} disabled={busy}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => { if (event.key === 'Enter' && event.nativeEvent.isComposing) event.preventDefault() }} /></label>
      {error && <p className="meta err" role="alert">{error}</p>}
      {busy && <p className="meta" role="status">{s.working}</p>}
      <div className="row">
        <button ref={submit} type="submit" disabled={busy}>{s.save}</button>
        <button type="button" className="secondary" disabled={busy} onPointerDown={(event) => event.preventDefault()} onClick={close}>{s.cancel}</button>
      </div>
    </form>
  </Dialog>
}
