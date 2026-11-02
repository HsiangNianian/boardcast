import { useCallback, useEffect, useState } from 'react'
import { on, voice, type SpeechModels as View, type VoiceStatus } from '../desktop.ts'
import { recordError } from '../engine/diagnostics.ts'
import { appleAssetIssue } from '../engine/speech.ts'
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
    setLocale((previous) => [previous, next.selection.locale, next.apple?.defaultLocale]
      .find((id) => id && next.apple?.locales.some((l) => l.id === id)) ?? next.apple?.locales[0]?.id ?? '')
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

  const language = view?.apple?.locales.find((l) => l.id === locale)
  const waiting = !!view?.preparing || language?.status === 'downloading'
  useEffect(() => {
    if (!waiting) return
    let disposed = false
    let timer = window.setTimeout(poll, 2000)
    async function poll() {
      try { await refresh() }
      catch (e) { if (!disposed) setError(String(e)) }
      if (!disposed) timer = window.setTimeout(poll, 2000)
    }
    return () => { disposed = true; clearTimeout(timer) }
  }, [waiting, refresh])

  async function act(action?: () => Promise<void>) {
    setBusy(true)
    setError('')
    try { await action?.() }
    catch (e) {
      const text = e instanceof Error ? e.message : String(e)
      recordError('speech-model', text)
      if (!appleAssetIssue(text)) setError(text)
    } finally {
      try { await refresh() } catch (e) { setError(String(e)) }
      setBusy(false)
    }
  }

  async function prepare() {
    await voice.prepare(locale)
    await voice.select('apple', locale)
  }

  const selected = view?.selection.provider === 'apple' && view.selection.locale === locale
  const appleReady = view?.apple?.available && view.apple.locales.some((l) => l.id === view.selection.locale && l.status === 'installed')
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
          {view.selection.provider === 'sensevoice' && <span className="tag on">{view.senseVoiceInstalled ? s.inUse : s.chosen}</span>}
        </div>
        <p className="note">{s.senseVoiceNote}</p>
        {!view.senseVoiceInstalled && <p className="meta">{s.speechDownloadBelow}</p>}
        {view.selection.provider !== 'sensevoice' && <div className="actions">
          <button disabled={disabled || !view.senseVoiceInstalled} onClick={() => void act(() => voice.select('sensevoice'))}>{s.use}</button>
        </div>}
      </div>
      <div className={`model ${view.selection.provider === 'apple' ? 'current' : ''}`}>
        <div className="row1"><b>Apple SpeechAnalyzer</b><span className="tag">{s.experimental}</span>
          {view.selection.provider === 'apple' && <span className="tag on">{appleReady ? s.inUse : s.chosen} · {view.selection.locale}</span>}
        </div>
        <p className="note">{s.appleNote}</p>
        <p className="meta">{s.appleStorage}</p>
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
              : !language ? s.appleNoLanguages : language.status === 'installed' ? s.appleInstalled
              : language.status === 'downloading' ? s.appleDownloading
              : language.status === 'unsupported' ? s.appleLanguageUnsupported
              : language.status === 'unknown' ? s.appleStatusUnknown : s.appleNeedsDownload}</p>
            <div className="actions">
              {language?.status === 'installed' ? <button disabled={disabled || selected} onClick={() => void act(() => voice.select('apple', locale))}>
                {selected ? s.inUse : s.use}
              </button> : <button disabled={disabled || !language || language.status === 'unsupported' || language.status === 'unknown'} onClick={() => void act(prepare)}>
                {view.preparing || busy ? s.applePreparing : language?.status === 'downloading' ? s.appleResume : s.appleDownload}
              </button>}
            </div>
            {selected && language?.status === 'installed' && !busy && <p className="hint">{s.speechReadyHint}</p>}
          </>}
      </div>
      {view.listening && <p className="hint">{s.stopToSwitch}</p>}
    </>}
    {error && <p className="meta err" role="alert">{error}</p>}
  </section>
}
