// 模型管理（桌面版）：下载、切换、删除。

import { useCallback, useEffect, useRef, useState } from 'react'
import { type Catalog, type LlmStatus, type ModelInfo, type Progress, llmProblem, modelCopy, modelMessage, models, onProgress } from './desktop.ts'
import { msg, useT } from './i18n/index.ts'
import SpeechModels from './ui/SpeechModels.tsx'
import Dialog from './ui/Dialog.tsx'

const gb = (n: number) => `${(n / 1e9).toFixed(1)} GB`
const mbps = (n: number) => `${(n / 1e6).toFixed(1)} MB/s`

function eta(p: Progress): string {
  if (!(p.speed > 0 && p.total > 0)) return ''
  const s = Math.max(0, Math.round((p.total - p.downloaded) / p.speed))
  return msg().models.eta(s)
}

export default function ModelPanel({ status, onClose, onBeforeRemove, speechOnly = false }: { status: LlmStatus | null; onClose: () => void; onBeforeRemove: () => Promise<void>; speechOnly?: boolean }) {
  const s = useT().models
  const [catalog, setCatalog] = useState<Catalog | null>(null)
  const [progress, setProgress] = useState<Record<string, Progress>>({})
  const [error, setError] = useState<string | null>(null)
  const [removing, setRemoving] = useState<ModelInfo | null>(null)
  const [pending, setPending] = useState(false)
  const working = useRef(false)
  const refreshId = useRef(0)
  const refresh = useCallback(async () => {
    const id = ++refreshId.current
    const next = await models.list()
    if (id === refreshId.current) setCatalog(next)
  }, [])

  useEffect(() => {
    let disposed = false
    let off = () => {}
    void onProgress((p) => {
      if (disposed) return
      setProgress((m) => ({ ...m, [p.id]: p }))
      if (p.state !== 'downloading' && p.state !== 'verifying') void refresh().catch((e) => { if (!disposed) setError(String(e)) })
    }).then((f) => { if (disposed) f(); else off = f }).catch((e) => { if (!disposed) setError(String(e)) })
    return () => { disposed = true; refreshId.current++; off() }
  }, [refresh])

  // 显卡信息是外壳启动时才探测的，状态变了就再拉一次清单（推荐可能会变）
  useEffect(() => { void refresh().catch((e) => setError(String(e))) }, [status?.state, refresh])

  const act = async (f: () => Promise<void>) => {
    if (working.current) return false
    working.current = true
    setPending(true)
    setError(null)
    try { await f(); await refresh(); return true }
    catch (e) {
      // A failed removal may already have deleted some of the model's files.
      await refresh().catch(() => {})
      setError(String(e)); return false
    }
    finally { working.current = false; setPending(false) }
  }

  const gpu = status?.gpus.find((g) => g.discrete) ?? status?.gpus[0]
  const busy = catalog?.downloading
  /** 还差多少空间（和外壳的检查一致：要另外留 0.3 GB 余量）；不知道剩余空间时不拦 */
  const lacking = (m: ModelInfo) =>
    catalog?.freeBytes === undefined ? 0 : Math.max(0, m.size - m.partial + 300_000_000 - catalog.freeBytes)

  return (
    <Dialog label={speechOnly ? s.speechSetupTitle : s.title} onClose={onClose} dismissible={!pending}>
      <div className="panel model-panel">
        <header>
          <h2>{speechOnly ? s.speechSetupTitle : s.title}</h2>
          <button className="ghost" onClick={onClose} disabled={pending}>
            {s.close}
          </button>
        </header>
        {speechOnly ? <p className="hint" role="status">{s.speechSetupHint}</p> : <p className="hint">
          {s.hint}
          {gpu ? s.gpu(gpu.name, gpu.totalMib ? Math.round(gpu.totalMib / 1024) : undefined) : s.noGpu}
        </p>}

        {pending && !removing && <p className="meta" role="status">{s.working}</p>}
        {!catalog && !error && <p className="hint" role="status">{s.loading}</p>}
        {(['voice', 'llm', 'vision'] as const).filter((kind) => !speechOnly || kind === 'voice').map((kind) => <section key={kind} className="model-group" aria-label={s.groups[kind]}>
        <h3>{s.groups[kind]}</h3>
        {kind === 'voice' && <SpeechModels />}
        {catalog?.models.filter((m) => m.kind === kind).map((m) => {
          const copy = modelCopy(m)
          const p = progress[m.id]
          const active = busy === m.id
          const downloaded = active && p ? p.downloaded : m.partial
          const total = active && p ? p.total : m.size
          const pct = total > 0 ? Math.min(1, Math.max(0, downloaded / total)) : undefined
          const paired = catalog.models.find((other) => m.kind === 'vision' ? other.id === m.id.replace(/-vision$/, '') : other.id === `${m.id}-vision`)
          return (
            <div key={m.id} className={`model ${m.selected ? 'current' : ''}`}>
              <div className="row1">
                <b>{copy.label}</b>
                {m.recommended && <span className="tag rec">{s.recommended}</span>}
                {m.selected && <span className="tag on">{status?.state === 'ready' ? s.inUse : s.chosen}</span>}
                {m.kind === 'llm' && m.installed && !m.selected && <span className="tag">{s.installed}</span>}
                {(m.kind === 'voice' || m.kind === 'vision') && m.installed && <span className="tag on">{s.ready}</span>}
                <span className="size">{gb(m.size)}</span>
              </div>
              <p className="note">{s.purpose[m.kind]}</p>
              {paired && <p className="note">{s.paired(modelCopy(paired).label, paired.installed)}</p>}
              {m.selected && status?.state === 'starting' && <p className="meta" role="status">{s.starting}</p>}
              {m.selected && status?.state === 'error' && <p className="meta err" role="alert">{s.loadFailed} {llmProblem(status)}</p>}
              <details><summary>{s.details}</summary><p className="meta">{copy.note}</p><code>{m.id}</code></details>

              {(active || (m.partial > 0 && !m.installed)) && (
                <progress max={1} value={pct} aria-label={`${s.progressAria}: ${copy.label}`} />
              )}
              {active && <div className="meta">
                <span role="status">{p?.state === 'verifying' ? s.verifying : s.downloading}</span>
                <span> · {pct === undefined ? s.unknownTotal(gb(downloaded)) : `${(pct * 100).toFixed(1)}%`}{p?.speed ? ` · ${mbps(p.speed)} · ${eta(p)}` : ''}{p?.source ? ` · ${p.source}` : ''}</span>
                {p?.state === 'verifying' && p.message && <p>{modelMessage(p.message)}</p>}
              </div>}
              {!active && !m.installed && m.partial > 0 && <p className="meta">{s.paused}</p>}
              {p?.state === 'error' && <div className="meta err" role="alert"><p>{s.downloadFailed}</p>{p.message && <p>{modelMessage(p.message)}</p>}</div>}

              <div className="actions">
                {m.installed ? (
                  <>
                    {m.kind === 'llm' && (!m.selected || status?.state === 'error') && (
                      <button onClick={() => void act(() => models.select(m.id))} disabled={!!busy || pending}>
                        {m.selected ? s.retryLoad : s.use}
                      </button>
                    )}
                    <button className="ghost" onClick={() => { setError(null); setRemoving(m) }} disabled={!!busy || pending}>
                      {s.delete}
                    </button>
                  </>
                ) : active ? (
                  <button className="ghost" onClick={() => void act(() => models.cancel())} disabled={pending}>
                    {s.pause}
                  </button>
                ) : (
                  <>
                    {lacking(m) > 0 && <span className="meta err">{s.lacking(gb(lacking(m)))}</span>}
                    <button onClick={() => void act(async () => { setProgress((old) => { const next = { ...old }; delete next[m.id]; return next }); await models.download(m.id) })} disabled={!!busy || pending || lacking(m) > 0}>
                      {m.partial > 0 ? s.resume(gb(m.partial)) : s.download}
                    </button>
                  </>
                )}
              </div>
            </div>
          )
        })}
        </section>)}

        {busy && <p className="hint">{s.backgroundDownload}</p>}
        {error && !removing && <div className="meta err" role="alert"><p>{modelMessage(error)}</p><button type="button" disabled={pending} onClick={() => void act(async () => {})}>{s.reload}</button></div>}
        {!speechOnly && <p className="fine">
          {s.sources}
          {catalog?.freeBytes !== undefined && s.free(gb(catalog.freeBytes))}
          {s.dir}
          <code>{catalog?.dir}</code>
        </p>}
      </div>
      {removing && <Dialog label={s.confirmDelete(modelCopy(removing).label)} onClose={() => { setRemoving(null); setError(null) }} dismissible={!pending}>
        <div className="panel model-remove">
          <h2>{s.confirmDelete(modelCopy(removing).label)}</h2>
          <p>{s.deletePreparation}</p>
          <p>{s.deleteImpact[removing.kind]}</p>
          {removing.kind === 'llm' && catalog?.models.some((m) => m.id === `${removing.id}-vision` && m.installed) && <p>{s.keepVision}</p>}
          <p className="hint">{s.keepMeetings}</p>
          {pending && <p role="status">{s.working}</p>}
          {error && <p className="meta err" role="alert">{modelMessage(error)}</p>}
          <div className="row">
            <button type="button" className="danger" disabled={pending} onClick={async () => { if (await act(async () => { await onBeforeRemove(); await models.remove(removing.id) })) setRemoving(null) }}>{s.confirmRemove}</button>
            <button type="button" className="ghost" disabled={pending} onClick={() => { setRemoving(null); setError(null) }}>{s.cancel}</button>
          </div>
        </div>
      </Dialog>}
    </Dialog>
  )
}
