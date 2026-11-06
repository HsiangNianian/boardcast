import type { Page } from '../meetings.ts'
import { useT } from '../i18n/index.ts'
import Dialog from './Dialog.tsx'
import './import-task.css'

export interface ImportPreview {
  pages: Page[]
  note: string
  details: string[]
  title?: string
  undoable?: boolean
}

interface Target { name: string; meetingId: string; meetingTitle: string; page: number }
export type ImportTaskState = Target & (
  | { status: 'reading'; progress: string }
  | { status: 'ready'; preview: ImportPreview }
  | { status: 'error'; error: string; configureLocal?: boolean }
)

export default function ImportTaskPanel({ task, busy, saveError, onCancel, onRetry, onConfigure, onCommit }: {
  task: ImportTaskState
  busy: boolean
  saveError?: string
  onCancel: () => void
  onRetry: () => void
  onConfigure: () => void
  onCommit: (fresh: boolean) => void
}) {
  const t = useT()
  const s = t.importTask
  return <Dialog label={s.title} onClose={onCancel} dismissible={!busy}>
    <div className="panel import-task">
      <header><h2>{s.title}</h2><button type="button" className="ghost" disabled={busy} onClick={onCancel}>{t.workbench.importCancel}</button></header>
      <p><strong>{task.name}</strong></p>
      <p className="hint">{s.target(task.meetingTitle)} · {t.pages.board(task.page + 1)}</p>
      {task.status === 'reading' && <div role="status"><p>{task.progress}</p><progress aria-label={s.progress} /></div>}
      {task.status === 'error' && <><p role="alert">{task.error}</p><div className="actions">
        <button type="button" onClick={onRetry}>{s.retry}</button>
        {task.configureLocal && <button type="button" className="ghost" onClick={onConfigure}>{s.configureLocal}</button>}
      </div></>}
      {task.status === 'ready' && <>
        <p>{s.ready(task.preview.pages.length)}</p>
        {task.preview.details.length > 0 && <ul>{task.preview.details.map((detail) => <li key={detail}>{detail}</li>)}</ul>}
        {saveError && <p role="alert">{s.saveFailed(saveError)}</p>}
        <div className="actions">
          <button type="button" disabled={busy} onClick={() => onCommit(false)}>{t.app.importHere}</button>
          <button type="button" className="ghost" disabled={busy} onClick={() => onCommit(true)}>{t.app.importNewMeeting}</button>
        </div>
      </>}
    </div>
  </Dialog>
}
