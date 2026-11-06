// 导出诊断信息：先把会带上的每个文件摆出来，看过了再存。

import Dialog from './Dialog.tsx'
import { useEffect, useState } from 'react'
import { diagnosticsInfo, isDesktop, llmStatus, updates, voice as speech } from '../desktop.ts'
import { type DiagFile, cleanLog, diagnosticErrors, diagnosticJson, makeZip } from '../engine/diagnostics.ts'
import { useT } from '../i18n/index.ts'
import { Close } from './icons.tsx'

interface Props {
  /** 收音的状态，由 App 给 */
  voice: Record<string, unknown>
  /** Where the time went in this meeting (numbers only, no content) */
  timing: unknown
  onClose: () => void
  onSaved: (ok: boolean, text: string) => void
}

async function collect(voice: Record<string, unknown>, timing: unknown): Promise<DiagFile[]> {
  const [shell, status, speechModels] = await Promise.all([
    isDesktop ? diagnosticsInfo().catch(() => null) : null,
    llmStatus()?.catch(() => null),
    isDesktop ? speech.models().catch((e) => ({ error: String(e) })) : null,
  ])
  const info = {
    version: isDesktop ? await updates.version().catch(() => 'unknown') : 'web',
    platform: shell ? { os: shell.os, arch: shell.arch } : navigator.platform,
    userAgent: navigator.userAgent,
    screen: `${innerWidth}x${innerHeight}@${devicePixelRatio}`,
    model: status ?? null,
    voice,
    speechModels,
  }
  const files: DiagFile[] = [
    { name: 'info.json', text: diagnosticJson(info) },
    { name: 'timing.json', text: diagnosticJson(timing) },
    { name: 'errors.json', text: diagnosticJson(diagnosticErrors()) },
  ]
  if (shell?.log) files.push({ name: 'llama-server.json', text: cleanLog(shell.log) })
  return files
}

export default function DiagnosticsPanel(p: Props) {
  const s = useT().diagnostics
  const [files, setFiles] = useState<DiagFile[] | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    void collect(p.voice, p.timing).then(setFiles)
    // 打开时收集一次，内容就是看到的这些
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  async function save() {
    if (!files || saving) return
    setSaving(true)
    setError(null)
    try {
      const { saveFile } = await import('../exporter.ts')
      const path = await saveFile('boardcast-diagnostics.zip', new Blob([makeZip(files) as BlobPart], { type: 'application/zip' }))
      if (path) {
        p.onSaved(true, s.saved(path))
        p.onClose()
      }
    } catch (e) {
      const message = s.saveFailed(e instanceof Error ? e.message : String(e))
      setError(message)
      p.onSaved(false, message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog label={s.title} onClose={p.onClose} dismissible={!saving}>
      <div className="panel diagnostics">
        <header>
          <h2>{s.title}</h2>
          <button type="button" className="icon" onClick={p.onClose} disabled={saving} aria-label={s.close}>
            <Close size={16} />
          </button>
        </header>
        <p className="hint">{s.hint}</p>
        {!files && <p className="hint">{s.collecting}</p>}
        {files?.map((f) => (
          <details key={f.name}>
            <summary>
              {f.name} <small>{s.chars(f.text.length)}</small>
            </summary>
            <pre>{f.text}</pre>
          </details>
        ))}
        {error && <p className="meta err" role="alert">{error}</p>}
        <div className="row end">
          <button type="button" className="ghost" onClick={p.onClose} disabled={saving}>
            {s.cancel}
          </button>
          <button type="button" disabled={!files || saving} onClick={() => void save()}>
            {saving ? s.saving : s.export}
          </button>
        </div>
      </div>
    </Dialog>
  )
}
