// 模型管理（桌面版）：下载、切换、删除。没有模型时自动弹出。

import { useCallback, useEffect, useState } from 'react'
import { type Catalog, type LlmStatus, type ModelInfo, type Progress, models, onProgress } from './desktop.ts'

const gb = (n: number) => `${(n / 1e9).toFixed(1)} GB`
const mbps = (n: number) => `${(n / 1e6).toFixed(1)} MB/s`

function eta(p: Progress): string {
  if (!p.speed) return ''
  const s = Math.round((p.total - p.downloaded) / p.speed)
  return s >= 60 ? `还要约 ${Math.ceil(s / 60)} 分钟` : `还要约 ${s} 秒`
}

export default function ModelPanel({ status, onClose }: { status: LlmStatus | null; onClose: () => void }) {
  const [catalog, setCatalog] = useState<Catalog | null>(null)
  const [progress, setProgress] = useState<Record<string, Progress>>({})
  const [error, setError] = useState<string | null>(null)

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
    <div className="modal" role="dialog" aria-label="本地模型">
      <div className="panel">
        <header>
          <h2>本地模型</h2>
          <button className="ghost" onClick={onClose} aria-label="关闭">
            关闭
          </button>
        </header>
        <p className="hint">
          模型在本机运行，不上传任何内容。
          {gpu
            ? ` 检测到显卡：${gpu.name}${gpu.totalMib ? `（${(gpu.totalMib / 1024).toFixed(0)} GB）` : ''}。`
            : ' 没有检测到可用的显卡，会用 CPU 运行。'}
        </p>

        {catalog?.models.map((m) => {
          const p = progress[m.id]
          const active = busy === m.id
          const pct = p && active ? p.downloaded / p.total : m.partial / m.size
          return (
            <div key={m.id} className={`model ${m.selected ? 'current' : ''}`}>
              <div className="row1">
                <b>{m.label}</b>
                {m.recommended && <span className="tag rec">推荐</span>}
                {m.selected && status?.state === 'ready' && <span className="tag on">正在使用</span>}
                {m.kind === 'voice' && m.installed && <span className="tag on">已就绪</span>}
                <span className="size">{gb(m.size)}</span>
              </div>
              <div className="note">{m.note}</div>

              {(active || (m.partial > 0 && !m.installed)) && (
                <div className="bar" aria-label="下载进度">
                  <i style={{ width: `${(pct * 100).toFixed(1)}%` }} />
                </div>
              )}
              {active && p && (
                <div className="meta">
                  {p.state === 'verifying'
                    ? (p.message ?? '校验中…')
                    : `${(pct * 100).toFixed(1)}% · ${mbps(p.speed)} · ${p.source ?? ''} · ${eta(p)}`}
                </div>
              )}
              {!active && p?.state === 'error' && <div className="meta err">{p.message}</div>}

              <div className="actions">
                {m.installed ? (
                  <>
                    {m.kind === 'llm' && !m.selected && (
                      <button onClick={() => act(() => models.select(m.id))} disabled={!!busy}>
                        使用这个
                      </button>
                    )}
                    <button className="ghost" onClick={() => act(() => models.remove(m.id))} disabled={active}>
                      删除
                    </button>
                  </>
                ) : active ? (
                  <button className="ghost" onClick={() => act(() => models.cancel())}>
                    暂停
                  </button>
                ) : (
                  <>
                    {lacking(m) > 0 && <span className="meta err">磁盘还差 {gb(lacking(m))}</span>}
                    <button onClick={() => act(() => models.download(m.id))} disabled={!!busy || lacking(m) > 0}>
                      {m.partial > 0 ? `继续下载（已下 ${gb(m.partial)}）` : '下载'}
                    </button>
                  </>
                )}
              </div>
            </div>
          )
        })}

        {error && <p className="meta err">{error}</p>}
        <p className="fine">
          下载源依次是 ModelScope、hf-mirror、Hugging Face，断点续传，下完用官方登记的 sha256 校验。
          {catalog?.freeBytes !== undefined && `模型目录所在的盘还剩 ${gb(catalog.freeBytes)}。`}模型目录：
          <code>{catalog?.dir}</code>
        </p>
      </div>
    </div>
  )
}
