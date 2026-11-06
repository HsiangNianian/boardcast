import { useCallback, useEffect, useRef, useState } from 'react'
import { isDesktop, on, voice, type InputCheckStatus, type SpeechModels, type VoiceSource, type VoiceStatus } from '../desktop.ts'
import { appleLanguageState, speechMessage } from '../engine/speech.ts'
import { useT } from '../i18n/index.ts'
import Dialog from './Dialog.tsx'
import './preparation.css'

export interface PreparationProps {
  source: VoiceSource
  setSource: (source: VoiceSource) => void
  mic: string
  setMic: (mic: string) => void
  boardReady: boolean
  boardLabel: string
  localOnly: boolean
  audioBusy?: boolean
  voiceError?: string | null
  onStart: () => Promise<boolean>
  onCancelStart?: () => Promise<unknown>
  onManageSpeech: () => void
  onManageBoard: () => void
  onType: () => void
  onClose: () => void
}

const message = (error: unknown) => error instanceof Error ? error.message : String(error)

export default function Preparation(p: PreparationProps) {
  const t = useT(), s = t.preparation
  const [status, setStatus] = useState<VoiceStatus | null>(null)
  const [models, setModels] = useState<SpeechModels | null>(null)
  const [devices, setDevices] = useState<string[]>([])
  const [checking, setChecking] = useState(isDesktop)
  const [readError, setReadError] = useState<string | null>(null)
  const [startError, setStartError] = useState<string | null>(null)
  const [starting, setStarting] = useState(false)
  const [canceling, setCanceling] = useState(false)
  const canceled = useRef(false)
  const generation = useRef(0)
  const [inputCheck, setInputCheck] = useState<InputCheckStatus | null>(null)
  const [inputStopping, setInputStopping] = useState(false)
  const inputId = useRef<string | null>(null)
  const inputListener = useRef<Promise<unknown>>(Promise.resolve())
  const inputStop = useRef<Promise<void> | null>(null)

  const refresh = useCallback(async () => {
    if (!isDesktop) return
    const current = ++generation.current
    setChecking(true)
    const results = await Promise.allSettled([voice.status(), voice.models(), voice.devices()])
    if (current !== generation.current) return
    const [capture, speech, inputs] = results
    setStatus(capture.status === 'fulfilled' ? capture.value : null)
    setModels(speech.status === 'fulfilled' ? speech.value : null)
    setDevices(inputs.status === 'fulfilled' ? inputs.value : [])
    const failed = results.find((result) => result.status === 'rejected')
    setReadError(failed?.status === 'rejected' ? message(failed.reason) : null)
    setChecking(false)
  }, [])

  useEffect(() => {
    if (!isDesktop) return
    void refresh()
    let disposed = false
    const cleanups: (() => void)[] = []
    const inputEvents = on<InputCheckStatus>('voice-input-check', (event) => {
      if (!disposed && event.id === inputId.current) {
        setInputCheck(event)
        if (event.state === 'stopped' || event.state === 'error') inputId.current = null
      }
    })
    inputListener.current = inputEvents
    for (const task of [
      on<VoiceStatus>('voice-status', setStatus),
      on('voice-models-changed', () => { void refresh() }),
      inputEvents,
    ]) void task.then((off) => disposed ? off() : cleanups.push(off)).catch(() => undefined)
    return () => {
      disposed = true; canceled.current = true; generation.current++; cleanups.forEach((off) => off())
      if (inputId.current) void voice.inputCheck.stop(inputId.current).catch(() => undefined)
      inputId.current = null
    }
  }, [refresh])

  const appleState = models?.selection.provider === 'apple' ? appleLanguageState(models, models.selection.locale) : null
  const preparing = models?.selection.provider === 'apple' && (!!models.preparing || appleState === 'downloading')
  useEffect(() => {
    if (!preparing) return
    const timer = window.setInterval(() => void refresh(), 2000)
    return () => clearInterval(timer)
  }, [preparing, refresh])

  const speechReady = isDesktop && !!status?.installed && !!models && !preparing
    && (models.selection.provider === 'sensevoice' ? models.senseVoiceInstalled : !models.appleError && appleState === 'installed')
  const listening = status?.listening ?? models?.listening ?? false
  const missingDevice = p.source !== 'system' && !!p.mic && !devices.includes(p.mic)
  const busy = starting || !!p.audioBusy
  const inputActive = inputCheck?.state === 'starting' || inputCheck?.state === 'active'
  const canStart = speechReady && !checking && !readError && !missingDevice && !listening && !busy && !inputStopping
  const provider = models?.selection.provider === 'apple' ? 'Apple SpeechAnalyzer' : models ? 'SenseVoice' : ''
  const locale = models?.selection.provider === 'apple'
    ? models.selection.locale === 'auto' ? t.models.appleAuto : models.selection.locale : ''
  const speechDetail = !isDesktop ? s.webSpeech : checking ? s.checking : readError ? s.checkFailed
    : models?.selection.provider === 'apple' ? (models.appleError ? speechMessage(models.appleError) : '')
      || (!models.apple?.available ? models.apple?.reason === 'os' ? t.models.appleNeedsOS
        : models.apple?.reason === 'hardware' ? t.models.appleUnsupported : t.models.appleNeedsMac
        : preparing ? t.models.applePreparing
          : models.selection.locale === 'auto' && !models.senseVoiceInstalled ? s.appleAutoNeedsSenseVoice
            : appleState === 'installed' ? s.speechReady
          : appleState === 'unknown' ? t.models.appleStatusUnknown
            : appleState === 'unsupported' ? t.models.appleLanguageUnsupported : t.models.appleNeedsDownload)
      : speechReady ? s.speechReady : s.speechMissing

  async function start() {
    if (!canStart) return
    canceled.current = false
    setStarting(true)
    setStartError(null)
    try {
      await stopInputCheck()
      if (canceled.current) return
      const success = await p.onStart()
      if (success && !canceled.current) p.onClose()
      else if (!canceled.current) setStartError(s.startFailed)
    } catch (error) {
      setStartError(message(error))
    } finally {
      setStarting(false)
      void refresh()
    }
  }

  async function cancelStart() {
    if (!p.onCancelStart || canceling) return
    canceled.current = true
    setCanceling(true)
    try { await p.onCancelStart() }
    catch (error) { setStartError(message(error)) }
    finally { setCanceling(false) }
  }

  async function checkInput() {
    if (!isDesktop || busy || listening || inputActive || inputStopping || missingDevice) return
    const id = crypto.randomUUID()
    inputId.current = id
    setInputCheck({ id, state: 'starting' })
    try {
      await inputListener.current
      if (inputId.current !== id) return
      await voice.inputCheck.start(id, p.mic, p.source)
    } catch (error) {
      if (inputId.current === id) { inputId.current = null; setInputCheck({ id, state: 'error', message: message(error) }) }
    }
  }

  function stopInputCheck(): Promise<void> {
    if (inputStop.current) return inputStop.current
    const id = inputId.current
    if (!id) return Promise.resolve()
    setInputStopping(true)
    const task = voice.inputCheck.stop(id).then(() => {
      if (inputId.current === id) {
        inputId.current = null
        setInputCheck({ id, state: 'stopped', reason: 'cancelled' })
      }
    }).finally(() => { inputStop.current = null; setInputStopping(false) })
    inputStop.current = task
    return task
  }

  async function leave(after: () => void) {
    try { await stopInputCheck(); after() }
    catch (error) { setStartError(message(error)) }
  }

  return <Dialog label={s.title} onClose={() => void leave(p.onClose)} dismissible={!busy && !inputStopping}>
    <div className="panel preparation">
      <header><h2>{s.title}</h2><button type="button" className="ghost" onClick={() => void leave(p.onClose)} disabled={busy || inputStopping}>{t.settings.close}</button></header>
      <p className="hint">{isDesktop ? s.intro : s.webIntro}</p>
      <ol className="preparation-checks">
        <li>
          <div className="preparation-heading"><h3>{s.source}</h3><span className="tag">{listening ? s.listening : inputActive ? s.inputChecking : s.notListening}</span></div>
          {isDesktop ? <>
            <label className="preparation-field"><span>{t.settings.sourceAria}</span>
              <select value={p.source} disabled={busy || listening || inputActive || inputStopping} onChange={(event) => { setInputCheck(null); p.setSource(event.target.value as VoiceSource) }}>
                {(['mic', 'both', 'system'] as const).map((source) => <option key={source} value={source}>{t.settings.sources[source]}</option>)}
              </select>
            </label>
            {p.source !== 'system' && <label className="preparation-field"><span>{t.settings.micAria}</span>
              <select value={p.mic} disabled={busy || listening || checking || inputActive || inputStopping} onChange={(event) => { setInputCheck(null); p.setMic(event.target.value) }}>
                <option value="">{t.settings.systemDefault}</option>
                {devices.map((device) => <option key={device} value={device}>{device}</option>)}
                {missingDevice && <option value={p.mic}>{p.mic}</option>}
              </select>
            </label>}
            {missingDevice && <p className="meta err">{s.deviceMissing}</p>}
            <div className="input-check">
              <button type="button" className="ghost" disabled={busy || listening || checking || missingDevice || inputStopping} onClick={() => void (inputActive ? leave(() => {}) : checkInput())}>{inputStopping ? t.composer.stopping : inputActive ? s.stopInputCheck : s.checkInput}</button>
              <p className="hint" role="status">{inputCheck?.state === 'starting' ? s.inputStarting : inputCheck?.state === 'active' ? s.inputActive : inputCheck?.reason === 'timeout' ? s.inputTimedOut : inputCheck?.state === 'stopped' ? s.inputStopped : s.inputCheckNote}</p>
              {inputCheck?.state === 'active' && <>
                {inputCheck.device && <p className="meta">{inputCheck.device}</p>}
                {inputCheck.level === undefined ? <p className="meta">{s.inputWaiting}</p> : <meter min={0} max={1} value={inputCheck.level} aria-label={s.inputLevel} />}
              </>}
              {inputCheck?.state === 'error' && <p className="meta err" role="alert">{speechMessage(inputCheck.message ?? '')}</p>}
            </div>
            <details><summary>{s.sourceDetails}</summary>
              <p className="hint">{s.inputUnmeasured}</p>
              {p.source !== 'mic' && <p className="hint">{s.systemPermission}</p>}
            </details>
          </> : <p className="hint">{s.webSource}</p>}
        </li>
        <li>
          <div className="preparation-heading"><h3>{s.speech}</h3><span className={`tag ${speechReady && !readError && !checking ? 'on' : ''}`}>{checking ? s.checking : speechReady && !readError ? s.ready : s.notReady}</span></div>
          {provider && <p className="preparation-model">{provider}{locale && ` · ${locale}`}</p>}
          <p className="hint">{speechDetail}</p>
          {isDesktop && <div className="preparation-actions">
            <button type="button" className="ghost" onClick={() => void leave(p.onManageSpeech)} disabled={busy || listening || inputStopping}>{speechReady ? s.manageSpeech : s.prepareSpeech}</button>
            <button type="button" className="ghost" onClick={() => void refresh()} disabled={busy || checking}>{s.recheck}</button>
          </div>}
          {models?.selection.provider === 'apple' && models.selection.locale === 'auto' && <details><summary>{t.models.appleAuto}</summary><p className="hint">{t.models.appleAutoNote}</p></details>}
        </li>
        <li>
          <div className="preparation-heading"><h3>{s.board}</h3><span className={`tag ${p.boardReady ? 'on' : ''}`}>{p.boardReady ? s.ready : s.optional}</span></div>
          <p className="preparation-model">{p.boardLabel}</p>
          {!p.boardReady && <p className="hint">{s.boardMissing}</p>}
          <details><summary>{s.boardDetails}</summary><p className="hint">{p.localOnly ? s.localOnly : s.boardPolicy}</p></details>
          <button type="button" className="ghost" onClick={() => void leave(p.onManageBoard)} disabled={busy || inputStopping}>{s.manageBoard}</button>
        </li>
      </ol>
      {(readError || startError || p.voiceError) && <p className="meta err" role="alert">{speechMessage(readError || p.voiceError || startError || '')}</p>}
      <div className="preparation-footer">
        <button type="button" className={isDesktop ? 'ghost' : ''} disabled={busy || inputStopping} onClick={() => void leave(() => { p.onClose(); requestAnimationFrame(p.onType) })}>{s.typeFirst}</button>
        {isDesktop && (starting && p.onCancelStart
          ? <button type="button" disabled={canceling} onClick={() => void cancelStart()}>{canceling ? t.composer.stopping : t.composer.cancelStarting}</button>
          : <button type="button" onClick={() => void start()} disabled={!canStart} aria-busy={busy}>{busy ? t.composer.starting : p.boardReady ? s.start : s.recordFirst}</button>)}
      </div>
    </div>
  </Dialog>
}
