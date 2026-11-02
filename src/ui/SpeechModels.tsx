import { useCallback, useEffect, useState } from 'react'
import { on, voice, type SpeechModels as View, type VoiceStatus } from '../desktop.ts'
import { useT } from '../i18n/index.ts'

export default function SpeechModels() {
  const s = useT().models
  const [view, setView] = useState<View | null>(null)
  const [locale, setLocale] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const refresh = useCallback(async () => {
    const next = await voice.models()
    setView(next)
    setLocale((previous) => next.apple?.locales.some((l) => l.id === previous) ? previous
      : next.selection.locale || next.apple?.defaultLocale || next.apple?.locales[0]?.id || '')
  }, [])

  useEffect(() => {
    void refresh().catch((e) => setError(String(e)))
    let disposed = false
    const cleanups: (() => void)[] = []
    for (const task of [
      on<VoiceStatus>('voice-status', (s) => setView((v) => v && { ...v, listening: s.listening })),
      on('voice-models-changed', () => { void refresh().catch((e) => setError(String(e))) }),
    ]) void task.then((off) => disposed ? off() : cleanups.push(off))
    return () => { disposed = true; cleanups.forEach((off) => off()) }
  }, [refresh])

  async function act(action?: () => Promise<void>) {
    setBusy(true)
    setError('')
    try { await action?.(); await refresh() }
    catch (e) { setError(String(e)) }
    finally { setBusy(false) }
  }

  const language = view?.apple?.locales.find((l) => l.id === locale)
  const selected = view?.selection.provider === 'apple' && view.selection.locale === locale
  const disabled = busy || !!view?.listening || !!view?.preparing
  return <section className="speech-models" aria-labelledby="speech-models-title" aria-busy={busy}>
    <div className="row speech-heading">
      <h3 id="speech-models-title">{s.speechTitle}</h3>
      <button className="ghost" disabled={busy} onClick={() => void act()}>{s.detect}</button>
    </div>
    {!view && !error && <p className="hint" role="status">{s.detecting}</p>}
    {view && <>
      <div className={`model ${view.selection.provider === 'sensevoice' ? 'current' : ''}`}>
        <div className="row1"><b>SenseVoice</b>
          {view.selection.provider === 'sensevoice' && <span className="tag on">{s.inUse}</span>}
        </div>
        <p className="note">{s.senseVoiceNote}</p>
        {!view.senseVoiceInstalled && <p className="meta">{s.speechDownloadBelow}</p>}
        {view.selection.provider !== 'sensevoice' && <div className="actions">
          <button disabled={disabled || !view.senseVoiceInstalled} onClick={() => void act(() => voice.select('sensevoice'))}>{s.use}</button>
        </div>}
      </div>
      <div className={`model ${view.selection.provider === 'apple' ? 'current' : ''}`}>
        <div className="row1"><b>Apple SpeechAnalyzer</b><span className="tag">{s.experimental}</span>
          {view.selection.provider === 'apple' && <span className="tag on">{s.inUse} · {view.selection.locale}</span>}
        </div>
        <p className="note">{s.appleNote}</p>
        {view.appleError ? <p className="meta err" role="alert">{view.appleError}</p>
          : !view.apple?.available ? <p className="meta">{view.apple?.reason === 'os' ? s.appleNeedsOS
            : view.apple?.reason === 'hardware' ? s.appleUnsupported : s.appleNeedsMac}</p>
          : <>
            <label className="speech-language">{s.speechLanguage}
              <select value={locale} disabled={disabled} onChange={(e) => setLocale(e.target.value)}>
                {view.apple.locales.map((l) => <option value={l.id} key={l.id}>{l.name} ({l.id})</option>)}
              </select>
            </label>
            <p className="meta" role="status">{view.preparing || busy ? s.speechWorking
              : !language ? s.appleNoLanguages : language.installed ? s.ready : s.appleNeedsDownload}</p>
            <div className="actions">
              {language?.installed ? <button disabled={disabled || selected} onClick={() => void act(() => voice.select('apple', locale))}>
                {selected ? s.inUse : s.use}
              </button> : <button disabled={disabled || !language} onClick={() => void act(() => voice.prepare(locale))}>{s.appleDownload}</button>}
            </div>
          </>}
      </div>
      {view.listening && <p className="hint">{s.stopToSwitch}</p>}
    </>}
    {error && <p className="meta err" role="alert">{error}</p>}
  </section>
}
