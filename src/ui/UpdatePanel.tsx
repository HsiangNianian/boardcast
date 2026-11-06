// 下载时可以返回会议；验签后由用户明确选择保存并重启。

import Dialog from './Dialog.tsx'
import { useEffect, useState } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { type UpdateInfo, llmEnsure, updates } from '../desktop.ts'
import { useT } from '../i18n/index.ts'

const RELEASES = 'https://github.com/HsiangNianian/boardcast/releases'
const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`

type Phase = 'download' | 'prepare' | 'install'
type Failure = { stage: Phase | 'signature'; message: string }

function failure(error: unknown, stage: Phase): Failure {
  if (error && typeof error === 'object' && 'stage' in error && 'message' in error
    && ['download', 'signature', 'install'].includes(String(error.stage))) {
    return { stage: error.stage as Failure['stage'], message: String(error.message) }
  }
  return { stage, message: error instanceof Error ? error.message : String(error) }
}

export default function UpdatePanel({ info, open, onBeforeInstall, onClose }: {
  info: UpdateInfo
  open: boolean
  onBeforeInstall: () => Promise<void>
  onClose: () => void
}) {
  const s = useT().update
  const [phase, setPhase] = useState<Phase | null>(null)
  const [downloaded, setDownloaded] = useState(false)
  const busy = phase !== null
  const installing = phase === 'prepare' || phase === 'install'
  const [got, setGot] = useState<{ done: number; total?: number } | null>(null)
  const [err, setErr] = useState<Failure | null>(null)

  useEffect(() => {
    const off = updates.onProgress(setGot).catch(() => () => {})
    return () => void off.then((f) => f()).catch(() => undefined)
  }, [])

  async function install() {
    if (busy) return
    let next: Phase = downloaded ? 'prepare' : 'download'
    setPhase(next)
    setErr(null)
    try {
      if (!downloaded) {
        setGot(null)
        await updates.download()
        setDownloaded(true)
        setPhase(null)
        return
      }
      next = 'prepare'
      setPhase(next)
      await onBeforeInstall()
      next = 'install'
      setPhase(next)
      await updates.install()
    } catch (error) {
      setErr(failure(error, next))
      setPhase(null)
      // Capture remains stopped after installation fails; only warm the local model again.
      if (next === 'install') void llmEnsure().catch(() => undefined)
    }
  }

  const pct = got?.total ? Math.min(100, Math.round((100 * got.done) / got.total)) : null
  if (!open) return null
  return (
    <Dialog label={s.aria} onClose={onClose} dismissible={!installing}>
      <div className="panel update">
        <header>
          <h2>
            {s.title(info.version)}
            <small>{s.current(info.current, info.date)}</small>
          </h2>
          <button className="ghost" onClick={onClose} disabled={installing}>
            {s.later}
          </button>
        </header>
        <div className="notes md">
          {info.notes ? (
            <Markdown remarkPlugins={[remarkGfm]} components={{ a: (p) => <a {...p} target="_blank" rel="noreferrer" /> }}>
              {info.notes}
            </Markdown>
          ) : (
            <p>
              {s.notesBefore}{' '}
              <a href={`${RELEASES}/tag/v${info.version}`} target="_blank" rel="noreferrer">
                GitHub Release
              </a>
              {s.notesAfter}
            </p>
          )}
        </div>
        {busy && (
          <div className="progress" aria-live="polite">
            <i style={{ width: `${pct ?? 0}%` }} />
            <span>
              {phase === 'prepare' ? s.preparing : phase === 'install' ? s.installing
                : got ? `${pct !== null ? `${pct}% · ` : ''}${mb(got.done)}${got.total ? ` / ${mb(got.total)}` : ''}` : s.starting}
            </span>
          </div>
        )}
        {err && <div role="alert">
          <p className="meta err">{s.failed[err.stage]}</p>
          <p className="hint">{err.message}</p>
          {err.stage === 'install' && <p className="hint">{s.recordingStopped}</p>}
        </div>}
        {downloaded && !busy && !err && <p className="meta" role="status">{s.ready}</p>}
        <div className="actions">
          <p className="hint">{s.saved}</p>
          <button onClick={() => void install()} disabled={busy}>
            {busy ? s.updating : err ? (downloaded ? s.retryInstall : s.retryDownload) : downloaded ? s.install : s.download}
          </button>
        </div>
      </div>
    </Dialog>
  )
}
