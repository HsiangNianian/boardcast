// 导出诊断信息：先把会带上的每个文件摆出来，看过了再存。

import { useEffect, useState } from 'react'
import { diagnosticsInfo, isDesktop, llmStatus, updates } from '../desktop.ts'
import { type DiagFile, cleanLog, makeZip, recentErrors, scrub } from '../engine/diagnostics.ts'
import { Close } from './icons.tsx'

interface Props {
  /** 收音的状态，由 App 给 */
  voice: Record<string, unknown>
  onClose: () => void
  onSaved: (ok: boolean, text: string) => void
}

async function collect(voice: Record<string, unknown>): Promise<DiagFile[]> {
  const shell = isDesktop ? await diagnosticsInfo().catch(() => null) : null
  const status = await llmStatus()?.catch(() => null)
  const info = {
    version: isDesktop ? await updates.version().catch(() => 'unknown') : 'web',
    platform: shell ? { os: shell.os, arch: shell.arch } : navigator.platform,
    userAgent: navigator.userAgent,
    screen: `${innerWidth}x${innerHeight}@${devicePixelRatio}`,
    model: status ?? null,
    voice,
  }
  const files: DiagFile[] = [
    { name: 'info.json', text: scrub(JSON.stringify(info, null, 2)) },
    { name: 'errors.txt', text: recentErrors().join('\n') || '（没有报错）' },
  ]
  if (shell?.log) files.push({ name: 'llama-server.log', text: cleanLog(shell.log) })
  return files
}

export default function DiagnosticsPanel(p: Props) {
  const [files, setFiles] = useState<DiagFile[] | null>(null)
  useEffect(() => {
    void collect(p.voice).then(setFiles)
    // 打开时收集一次，内容就是看到的这些
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  async function save() {
    if (!files) return
    const { saveFile } = await import('../exporter.ts')
    try {
      const path = await saveFile('boardcast-diagnostics.zip', new Blob([makeZip(files) as BlobPart], { type: 'application/zip' }))
      if (path) {
        p.onSaved(true, `诊断信息存好了：${path}`)
        p.onClose()
      }
    } catch (e) {
      p.onSaved(false, `保存失败：${e instanceof Error ? e.message : String(e)}`)
    }
  }

  return (
    <div className="modal" role="dialog" aria-label="导出诊断信息" onClick={(e) => e.target === e.currentTarget && p.onClose()}>
      <div className="panel diagnostics">
        <header>
          <h2>导出诊断信息</h2>
          <button type="button" className="icon" onClick={p.onClose} aria-label="关闭">
            <Close size={16} />
          </button>
        </header>
        <p className="hint">会打包下面这些文件，可以点开看内容。不包含会议内容、录音和 API key；用户名和像 key 的字符串已经抹掉了。</p>
        {!files && <p className="hint">正在收集…</p>}
        {files?.map((f) => (
          <details key={f.name}>
            <summary>
              {f.name} <small>{f.text.length} 字符</small>
            </summary>
            <pre>{f.text}</pre>
          </details>
        ))}
        <div className="row end">
          <button type="button" className="ghost" onClick={p.onClose}>
            取消
          </button>
          <button type="button" disabled={!files} onClick={() => void save()}>
            导出 zip
          </button>
        </div>
      </div>
    </div>
  )
}
