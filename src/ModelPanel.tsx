// 模型管理（桌面版）：下载、切换、删除。没有模型时自动弹出。

import { useCallback, useEffect, useRef, useState } from 'react'
import { type Catalog, type LlmStatus, type ModelInfo, type Progress, models, onProgress } from './desktop.ts'
import { msg, useT } from './i18n/index.ts'
import SpeechModels from './ui/SpeechModels.tsx'

const gb = (n: number) => `${(n / 1e9).toFixed(1)} GB`
const mbps = (n: number) => `${(n / 1e6).toFixed(1)} MB/s`

function eta(p: Progress): string {
  if (!p.speed) return ''
  const s = Math.round((p.total - p.downloaded) / p.speed)
  return msg().models.eta(s)
}

export default function ModelPanel({ status, onClose, speechOnly = false }: { status: LlmStatus | null; onClose: () => void; speechOnly?: boolean }) {
  const s = useT().models
  const [catalog, setCatalog] = useState<Catalog | null>(null)
  const [progress, setProgress] = useState<Record<string, Progress>>({})
  const [error, setError] = useState<string | null>(null)
  const dialog = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const previous = document.activeElement
    dialog.current?.querySelector('button')?.focus()
    return () => { if (previous instanceof HTMLElement && previous.isConnected) previous.focus() }
  }, [])

  useEffect(() => {
    const keydown = (e: KeyboardEvent) => {
      const panel = dialog.current
      if (!panel) return
      if (e.key === 'Escape') { e.stopPropagation(); onClose(); return }
      if (e.key === 'Tab') {
        const controls = [...panel.querySelectorAll<HTMLElement>('button:not([disabled]), select:not([disabled]), a[href]')]
        const target = e.shiftKey ? controls.at(-1) : controls[0]
        if (target && (!panel.contains(document.activeElement) || document.activeElement === (e.shiftKey ? controls[0] : controls.at(-1)))) {
          e.preventDefault()
          target.focus()
        }
      }
      // Disabling the download button can leave focus on the document body.
      if (!panel.contains(e.target as Node)) e.stopPropagation()
    }
    document.addEventListener('keydown', keydown, true)
    return () => document.removeEventListener('keydown', keydown, true)
  }, [onClose])

  const refresh = useCallback(() => {
    void models
      .list()
      .then(setCatalog)
      .catch((e) => setError(String(e)))
  }, [])

  useEffect(() => {
    refresh()
    let off = () => {}
    void onProgress((p) => {
      setProgress((m) => ({ ...m, [p.id]: p }))
      if (p.state !== 'downloading' && p.state !== 'verifying') refresh()
    }).then((f) => (off = f))
    return () => off()
  }, [refresh])

  // 显卡信息是外壳启动时才探测的，状态变了就再拉一次清单（推荐可能会变）
  useEffect(refresh, [status?.state, refresh])

  const act = (f: () => Promise<void>) => {
    setError(null)
    void f()
      .then(refresh)
      .catch((e) => setError(String(e)))
  }

  const gpu = status?.gpus.find((g) => g.discrete) ?? status?.gpus[0]
  const busy = catalog?.downloading
  /** 还差多少空间（和外壳的检查一致：要另外留 0.3 GB 余量）；不知道剩余空间时不拦 */
  const lacking = (m: ModelInfo) =>
    catalog?.freeBytes === undefined ? 0 : Math.max(0, m.size - m.partial + 300_000_000 - catalog.freeBytes)

  return (
    <div ref={dialog} className="modal" role="dialog" aria-modal="true" aria-label={speechOnly ? s.speechSetupTitle : s.title}>
      <div className="panel">
        <header>
          <h2>{speechOnly ? s.speechSetupTitle : s.title}</h2>
          <button className="ghost" onClick={onClose}>
            {s.close}
          </button>
        </header>
        {speechOnly ? <p className="hint" role="status">{s.speechSetupHint}</p> : <p className="hint">
          {s.hint}
          {gpu ? s.gpu(gpu.name, gpu.totalMib ? Math.round(gpu.totalMib / 1024) : undefined) : s.noGpu}
        </p>}

        <SpeechModels />

        {catalog?.models.filter((m) => !speechOnly || m.kind === 'voice').map((m) => {
          const p = progress[m.id]
          const active = busy === m.id
          const pct = p && active ? p.downloaded / p.total : m.partial / m.size
          return (
            <div key={m.id} className={`model ${m.selected ? 'current' : ''}`}>
              <div className="row1">
                <b>{m.label}</b>
                {m.recommended && <span className="tag rec">{s.recommended}</span>}
                {m.selected && status?.state === 'ready' && <span className="tag on">{s.inUse}</span>}
                {(m.kind === 'voice' || m.kind === 'vision') && m.installed && <span className="tag on">{s.ready}</span>}
                <span className="size">{gb(m.size)}</span>
              </div>
              <div className="note">{m.note}</div>

              {(active || (m.partial > 0 && !m.installed)) && (
                <div className="bar" aria-label={s.progressAria}>
                  <i style={{ width: `${(pct * 100).toFixed(1)}%` }} />
                </div>
              )}
              {active && p && (
                <div className="meta">
                  {p.state === 'verifying'
                    ? (p.message ?? s.verifying)
                    : `${(pct * 100).toFixed(1)}% · ${mbps(p.speed)} · ${p.source ?? ''} · ${eta(p)}`}
                </div>
              )}
              {!active && p?.state === 'error' && <div className="meta err">{p.message}</div>}

              <div className="actions">
                {m.installed ? (
                  <>
                    {m.kind === 'llm' && !m.selected && (
                      <button onClick={() => act(() => models.select(m.id))} disabled={!!busy}>
                        {s.use}
                      </button>
                    )}
                    <button className="ghost" onClick={() => act(() => models.remove(m.id))} disabled={active}>
                      {s.delete}
                    </button>
                  </>
                ) : active ? (
                  <button className="ghost" onClick={() => act(() => models.cancel())}>
                    {s.pause}
                  </button>
                ) : (
                  <>
                    {lacking(m) > 0 && <span className="meta err">{s.lacking(gb(lacking(m)))}</span>}
                    <button onClick={() => act(() => models.download(m.id))} disabled={!!busy || lacking(m) > 0}>
                      {m.partial > 0 ? s.resume(gb(m.partial)) : s.download}
                    </button>
                  </>
                )}
              </div>
            </div>
          )
        })}

        {error && <p className="meta err">{error}</p>}
        {!speechOnly && <p className="fine">
          {s.sources}
          {catalog?.freeBytes !== undefined && s.free(gb(catalog.freeBytes))}
          {s.dir}
          <code>{catalog?.dir}</code>
        </p>}
      </div>
    </div>
  )
}
