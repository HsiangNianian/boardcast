// 有新版本时的面板：版本号、更新说明（Release 正文，按 Markdown 渲染），一个「下载并重启」。
// 下载有进度；下好以后外壳校验签名、停掉收音和内置模型、安装、重启。失败的话把内置模型拉回来，接着开会。

import { useEffect, useState } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { type UpdateInfo, llmEnsure, updates } from '../desktop.ts'
import { useT } from '../i18n/index.ts'

const RELEASES = 'https://github.com/HsiangNianian/boardcast/releases'
const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`

export default function UpdatePanel({ info, onClose }: { info: UpdateInfo; onClose: () => void }) {
  const s = useT().update
  const [busy, setBusy] = useState(false)
  const [got, setGot] = useState<{ done: number; total?: number } | null>(null)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    const off = updates.onProgress(setGot)
    return () => void off.then((f) => f())
  }, [])

  async function install() {
    setBusy(true)
    setErr(null)
    try {
      await updates.install()
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e))
      setBusy(false)
      setGot(null)
      // 安装前会停掉内置模型；没装成就把它拉回来
      void llmEnsure().catch(() => undefined)
    }
  }

  const pct = got?.total ? Math.min(100, Math.round((100 * got.done) / got.total)) : null
  return (
    <div className="modal" role="dialog" aria-label={s.aria} onClick={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="panel update">
        <header>
          <h2>
            {s.title(info.version)}
            <small>{s.current(info.current, info.date)}</small>
          </h2>
          <button className="ghost" onClick={onClose} disabled={busy}>
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
              {got
                ? `${pct !== null ? `${pct}% · ` : ''}${mb(got.done)}${got.total ? ` / ${mb(got.total)}` : ''}${pct === 100 ? s.installing : ''}`
                : s.starting}
            </span>
          </div>
        )}
        {err && <p className="meta err">{err}</p>}
        <div className="actions">
          <p className="hint">{s.saved}</p>
          <button onClick={() => void install()} disabled={busy}>
            {busy ? s.updating : s.install}
          </button>
        </div>
      </div>
    </div>
  )
}
