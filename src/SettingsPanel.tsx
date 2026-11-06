// 设置：不常用的东西都放这里，主页面只留说话、收音和板书本身。

import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { type LlmStatus, type UpdateInfo, type VoiceSource, isDesktop, llmProblem, shell, updates, voice } from './desktop.ts'
import { accelFrom } from './engine/shortcuts.ts'
import { readGlossary } from './engine/glossary.ts'
import { LANGS, LANG_NAMES, setLangChoice, useLangChoice, useT } from './i18n/index.ts'
import { hostOf, isLoopback, needsConsent, rememberConsent } from './privacy.ts'
import { type RemoteTest, type RemoteView, remote } from './remote.ts'
import Dialog from './ui/Dialog.tsx'
import SpeechModels from './ui/SpeechModels.tsx'
import './ui/settings.css'

export type RemoteState = { state: 'off' | 'checking' | 'ok' | 'fail'; ms?: number; message?: string }

const SECTIONS = ['general', 'audio', 'models', 'privacy', 'glossary', 'maintenance'] as const
type Section = typeof SECTIONS[number]

interface Props {
  initialSection?: Section
  theme: 'system' | 'light' | 'dark'
  setTheme: (theme: 'system' | 'light' | 'dark') => void
  hotkey: string
  setHotkey: (v: string) => void
  hotkeyErr: { accel: string; message: string } | null
  hotkeyBusy: boolean
  onKeys: () => void
  onDiagnostics: () => void
  onClose: () => void
  // 本地模型
  builtin: LlmStatus | null
  warming: boolean
  localUrl: string
  setLocalUrl: (u: string) => void
  localOnline: boolean | null
  onManageModels: () => void
  // 远程模型
  remoteView: RemoteView | null
  remoteState: RemoteState
  onRemoteChanged: () => void
  /** 这场会只用本地模型（按会议记住） */
  localOnly: boolean
  setLocalOnly: (on: boolean) => void
  // 术语表：全局的，和这场会的
  glossary: string
  setGlossary: (s: string) => void
  meetingGlossary: string
  setMeetingGlossary: (s: string) => void
  // 语音
  mic: string
  setMic: (m: string) => void
  source: VoiceSource
  setSource: (s: VoiceSource) => void
  audioBusy: boolean
  diarize: boolean
  setDiarize: (on: boolean) => void
  // 更新
  autoUpdate: boolean
  setAutoUpdate: (on: boolean) => void
  /** 手动检查到了新版本 */
  onUpdate: (u: UpdateInfo) => void
  // 手写
  inkMode: boolean
  messy: number
  setMessy: (n: number) => void
  /** 笔画粗细，百分比 */
  weight: number
  setWeight: (n: number) => void
  englishWeight: number
  setEnglishWeight: (n: number) => void
  /** 纠错样本：一共攒了几条能导出的；导出成评测用例或偏好数据，返回存到哪儿（取消时 null） */
  countSamples: () => Promise<number>
  exportSamples: (kind: 'cases' | 'pairs') => Promise<string | null>
}

/** 错误对象只要它的消息，不要前面的「Error: 」 */
export function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

export default function SettingsPanel(p: Props) {
  const t = useT()
  const s = t.settings
  const glossaryErrors = readGlossary(p.glossary).invalidLines
  const meetingGlossaryErrors = readGlossary(p.meetingGlossary).invalidLines
  const [section, setSection] = useState<Section>(p.initialSection ?? 'general')
  const sectionId = useId()
  const hotkeyError = useRef<HTMLParagraphElement>(null)
  useEffect(() => {
    hotkeyError.current?.scrollIntoView({ block: 'nearest' })
  }, [p.hotkeyErr, section])
  const scroll = useRef<Partial<Record<Section, number>>>({})
  const restoreScroll = useCallback((node: HTMLDivElement | null) => {
    if (node) node.scrollTop = scroll.current[section] ?? 0
  }, [section])
  const [discard, setDiscard] = useState(false)
  const [wayland, setWayland] = useState(false)
  useEffect(() => {
    if (isDesktop) void shell.wayland().then(setWayland).catch(() => undefined)
  }, [])
  // —— 远程模型的表单 ——
  const [baseUrl, setBaseUrl] = useState(p.remoteView?.baseUrl ?? '')
  const [apiKey, setApiKey] = useState('')
  const [model, setModel] = useState(p.remoteView?.model ?? '')
  const [busy, setBusy] = useState<'test' | 'save' | null>(null)
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null)
  useEffect(() => {
    setBaseUrl(p.remoteView?.baseUrl ?? '')
    setModel(p.remoteView?.model ?? '')
  }, [p.remoteView?.baseUrl, p.remoteView?.model])

  // 不改 key 就留空，沿用已保存的
  const draft = () => ({ baseUrl, model, apiKey: apiKey ? apiKey : undefined })

  async function test(): Promise<RemoteTest | null> {
    setResult(null)
    setBusy('test')
    try {
      const r = await remote.test(draft())
      setResult({ ok: true, text: s.connected(r.ms, r.reply) })
      return r
    } catch (e) {
      setResult({ ok: false, text: errText(e) })
      return null
    } finally {
      setBusy(null)
    }
  }

  // 第一次把会议内容发到某个云端地址之前，先说清楚会发什么、发到哪，用户点了确认才启用
  const [consent, setConsent] = useState(false)
  function requestEnable() {
    if (needsConsent(baseUrl)) setConsent(true)
    else void saveAndEnable()
  }

  // 先确认能连上再启用；连不上也把填的内容存下来，免得白填
  async function saveAndEnable() {
    setConsent(false)
    setBusy('save')
    setResult(null)
    try {
      const r = await remote.test(draft()).catch((e) => {
        throw new Error(s.notEnabled(errText(e)))
      })
      await remote.save({ ...draft(), enabled: true })
      setApiKey('')
      setResult({ ok: true, text: s.enabled(r.ms) })
    } catch (e) {
      await remote.save({ ...draft(), enabled: false }).then(() => setApiKey(''), () => undefined)
      setResult({ ok: false, text: errText(e) })
    } finally {
      setBusy(null)
      p.onRemoteChanged()
    }
  }

  async function disable() {
    setBusy('save')
    setResult(null)
    try {
      await remote.save({ ...draft(), enabled: false })
      p.onRemoteChanged()
    } catch (error) {
      setResult({ ok: false, text: errText(error) })
    } finally {
      setBusy(null)
    }
  }

  // —— 麦克风列表 ——
  const [devices, setDevices] = useState<string[]>([])
  useEffect(() => {
    if (isDesktop) void voice.devices().then(setDevices).catch(() => setDevices([]))
  }, [])

  // —— 更新 ——
  const [version, setVersion] = useState('')
  const [checking, setChecking] = useState(false)
  const [checked, setChecked] = useState<{ ok: boolean; text: string } | null>(null)
  useEffect(() => {
    if (isDesktop) void updates.version().then(setVersion).catch(() => undefined)
  }, [])
  async function checkNow() {
    setChecking(true)
    setChecked(null)
    try {
      const u = await updates.check()
      if (u) p.onUpdate(u)
      else setChecked({ ok: true, text: s.upToDate })
    } catch (e) {
      setChecked({ ok: false, text: errText(e) })
    } finally {
      setChecking(false)
    }
  }

  // —— 纠错样本 ——
  const [samples, setSamples] = useState<number | null>(null)
  const [samplesError, setSamplesError] = useState<string | null>(null)
  const [samplesAttempt, setSamplesAttempt] = useState(0)
  const [exporting, setExporting] = useState(false)
  const exportingRef = useRef(false)
  const [exported, setExported] = useState<{ ok: boolean; text: string } | null>(null)
  useEffect(() => {
    if (section !== 'maintenance') return
    let active = true
    setSamples(null)
    setSamplesError(null)
    void p.countSamples().then(
      (count) => { if (active) setSamples(count) },
      (error) => { if (active) setSamplesError(errText(error)) },
    )
    return () => { active = false }
  }, [section, samplesAttempt])
  async function exportSamples(kind: 'cases' | 'pairs') {
    if (exportingRef.current) return
    exportingRef.current = true
    setExporting(true)
    setExported(null)
    try {
      const where = await p.exportSamples(kind)
      if (where) setExported({ ok: true, text: isDesktop ? s.samplesSaved(where) : t.app.downloaded(where) })
    } catch (e) {
      setExported({ ok: false, text: errText(e) })
    } finally {
      exportingRef.current = false
      setExporting(false)
    }
  }

  const b = p.builtin
  const rs = p.remoteState
  const enabled = p.remoteView?.enabled

  function close() {
    if (exportingRef.current) return
    const changed = baseUrl !== (p.remoteView?.baseUrl ?? '') || model !== (p.remoteView?.model ?? '') || !!apiKey
    if (changed) setDiscard(true)
    else p.onClose()
  }

  return (
    <Dialog label={s.title} onClose={close} dismissible={!busy && !exporting}>
      <div className="panel settings">
        <header>
          <h2>{s.title}</h2>
          <button className="ghost" onClick={close} disabled={!!busy || exporting}>{s.close}</button>
        </header>
        <label className="settings-mobile-nav">
          <span>{s.sectionsLabel}</span>
          <select aria-label={s.sectionsLabel} value={section} onChange={(event) => setSection(event.target.value as Section)}>
            {SECTIONS.map((key) => <option key={key} value={key}>{s.sections[key]}</option>)}
          </select>
        </label>
        <div className="settings-shell">
          <nav className="settings-nav" role="tablist" aria-label={s.sectionsLabel} aria-orientation="vertical"
            onKeyDown={(event) => {
              const index = SECTIONS.indexOf(section)
              const next = event.key === 'Home' ? 0 : event.key === 'End' ? SECTIONS.length - 1
                : ['ArrowRight', 'ArrowDown'].includes(event.key) ? (index + 1) % SECTIONS.length
                  : ['ArrowLeft', 'ArrowUp'].includes(event.key) ? (index + SECTIONS.length - 1) % SECTIONS.length : null
              if (next === null) return
              event.preventDefault()
              setSection(SECTIONS[next])
              event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus()
            }}>
            {SECTIONS.map((key) => <button key={key} type="button" role="tab"
              id={`${sectionId}-${key}`} aria-controls={`${sectionId}-content`} aria-selected={section === key}
              tabIndex={section === key ? 0 : -1} onClick={() => setSection(key)}>{s.sections[key]}</button>)}
          </nav>
          <div className="settings-content" role="tabpanel" id={`${sectionId}-content`} aria-label={s.sections[section]} key={section}
            ref={restoreScroll}
            onScroll={(event) => { scroll.current[section] = event.currentTarget.scrollTop }}>
            <h3 className="settings-section-title">{s.sections[section]}</h3>
            {section === 'general' && <>
              <LanguagePicker />
              <section>
                <label className="field"><span>{s.appearance}</span>
                  <select value={p.theme} onChange={(event) => p.setTheme(event.target.value as Props['theme'])}>
                    {(['system', 'light', 'dark'] as const).map((value) => <option key={value} value={value}>{t.theme[value]}</option>)}
                  </select>
                </label>
              </section>
              <section>
                <h3>{s.handwriting}</h3>
                {!p.inkMode && <p className="hint">{s.handwritingHint}</p>}
                <label className="messy">
                  <span>
                    {s.messyLabel} <b>{p.messy}</b>
                    <i>{s.messy(p.messy)}</i>
                  </span>
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={p.messy}
                    onChange={(e) => p.setMessy(Number(e.target.value))}
                    aria-label={s.messyAria}
                  />
                </label>
                <label className="messy">
                  <span>
                    {s.weightLabel} <b>{p.weight}%</b>
                    <i>{s.weightName(p.weight)}</i>
                  </span>
                  <input
                    type="range"
                    min={50}
                    max={200}
                    step={5}
                    value={p.weight}
                    onChange={(e) => p.setWeight(Number(e.target.value))}
                    aria-label={s.weightLabel}
                  />
                </label>
                <label className="messy">
                  <span>
                    {s.englishWeightLabel} <b>{p.englishWeight}%</b>
                    <i>{s.weightName(p.englishWeight)}</i>
                  </span>
                  <input
                    type="range"
                    min={50}
                    max={200}
                    step={5}
                    value={p.englishWeight}
                    onChange={(e) => p.setEnglishWeight(Number(e.target.value))}
                    aria-label={s.englishWeightLabel}
                  />
                </label>
              </section>
              <section>
                <h3>{s.keys}</h3>
                <div className="actions">
                  <button type="button" className="ghost" onClick={p.onKeys}>
                    {s.allKeys}
                  </button>
                </div>
                {isDesktop && (
                  <>
                    <label className="field">
                      <span>{s.hotkey}</span>
                      <input
                        data-shortcut-recorder
                        value={p.hotkey}
                        readOnly
                        placeholder={s.hotkeyOff}
                        aria-label={s.hotkeyAria}
                        aria-busy={p.hotkeyBusy}
                        aria-describedby={`${sectionId}-hotkey-hint${p.hotkeyErr ? ` ${sectionId}-hotkey-error` : ''}`}
                        onKeyDown={(e) => {
                          if (e.key === 'Tab' || e.key === 'Escape' || e.repeat) return
                          e.preventDefault()
                          if (e.key === 'Backspace' || e.key === 'Delete') p.setHotkey('')
                          const a = accelFrom(e)
                          if (a) p.setHotkey(a)
                        }}
                      />
                    </label>
                    <p id={`${sectionId}-hotkey-hint`} className="hint" role="status">{p.hotkeyBusy ? s.hotkeySaving : s.hotkeyHint}</p>
                    {p.hotkeyErr && <p ref={hotkeyError} id={`${sectionId}-hotkey-error`} className="meta err" role="alert">
                      {s.hotkeyFailed(p.hotkeyErr.accel || s.hotkeyOff)}{' '}{p.hotkey ? s.hotkeyRetained(p.hotkey) : s.hotkeyInactive}
                      <br />{p.hotkeyErr.message}
                    </p>}
                    {wayland && (
                      <p className="hint">
                        {s.wayland1} <code>boardcast --toggle-mic</code>{s.wayland2}
                      </p>
                    )}
                  </>
                )}
              </section>
            </>}
            {section === 'audio' && <>
              {isDesktop && (
                <section>
                  <h3>{s.audio}</h3>
                  <div className="seg sources" role="radiogroup" aria-label={s.sourceAria} aria-busy={p.audioBusy}>
                    {(['mic', 'both', 'system'] as const).map((v) => (
                      <button key={v} type="button" role="radio" aria-checked={p.source === v} disabled={p.audioBusy} className={p.source === v ? 'on' : ''} onClick={() => p.setSource(v)}>
                        {s.sources[v]}
                      </button>
                    ))}
                  </div>
                  <p className="hint">{s.sourceHint}</p>
                  <select value={p.mic} disabled={p.audioBusy} onChange={(e) => p.setMic(e.target.value)} aria-label={s.micAria}>
                    <option value="">{s.systemDefault}</option>
                    {devices.map((d) => (
                      <option key={d} value={d}>
                        {d}
                      </option>
                    ))}
                    {p.mic && !devices.includes(p.mic) && <option value={p.mic}>{s.micMissing(p.mic)}</option>}
                  </select>
                  <p className="hint">{s.micHint}</p>
                  <label className="check">
                    <input type="checkbox" checked={p.diarize} onChange={(e) => p.setDiarize(e.target.checked)} />
                    <span>{s.diarize}</span>
                  </label>
                  <p className="hint">{s.diarizeHint}</p>
                </section>
              )}
              {isDesktop ? <><SpeechModels /><button type="button" className="ghost" onClick={p.onManageModels}>{s.manageModels}</button></> : <p className="hint">{s.audioDesktopOnly}</p>}
            </>}
            {section === 'models' && <>
              <section>
                <h3>{s.local}</h3>
                {isDesktop ? (
                  <>
                    <p className="hint">
                      {b?.state === 'ready' && s.builtinReady(b.model ?? '', b.device ?? '', p.warming)}
                      {b?.state === 'idle' && s.builtinIdle(b.model ?? '')}
                      {b?.state === 'starting' && s.builtinStarting(b.model ?? '', b.device ?? '')}
                      {(b?.state === 'no-model' || b?.state === 'error') && llmProblem(b)}
                      {!b && s.loading}
                    </p>
                    {p.localUrl && <code className="addr">{p.localUrl}</code>}
                    <div className="actions">
                      <button onClick={p.onManageModels}>{s.manageModels}</button>
                    </div>
                  </>
                ) : (
                  <label className="field">
                    <span>
                      {s.llamaUrl} <i className={`dot ${p.localOnline === null ? '' : p.localOnline ? 'ok' : 'bad'}`} />
                    </span>
                    <input
                      value={p.localUrl}
                      onChange={(e) => p.setLocalUrl(e.target.value.trim())}
                      placeholder="http://127.0.0.1:8080"
                      spellCheck={false}
                      aria-invalid={!!p.localUrl && !isLoopback(p.localUrl)}
                      aria-describedby={`${sectionId}-local-address`}
                    />
                    <span className={`hint${p.localUrl && !isLoopback(p.localUrl) ? ' err' : ''}`} id={`${sectionId}-local-address`}>{t.errors.localAddress}</span>
                  </label>
                )}
              </section>
            </>}
            {section === 'privacy' && <>
              <section>
                <h3>
                  {s.remote}
                  {enabled && rs.state === 'ok' && <span className="tag on">{s.inUse}</span>}
                  {enabled && rs.state === 'fail' && <span className="tag err">{s.remoteDown}</span>}
                  {enabled && rs.state === 'checking' && <span className="tag">{s.checking}</span>}
                </h3>
                <p className="hint">
                  {s.remoteHint}
                  {!isDesktop && s.remoteWebHint}
                </p>
                <label className="field">
                  <span>Base URL</span>
                  <input
                    value={baseUrl}
                    readOnly={!!busy}
                    onChange={(e) => setBaseUrl(e.target.value)}
                    placeholder="https://api.openai.com/v1"
                    spellCheck={false}
                    autoComplete="off"
                  />
                </label>
                <label className="field">
                  <span>API Key</span>
                  <input
                    type="password"
                    value={apiKey}
                    readOnly={!!busy}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={p.remoteView?.keyHint ? s.keySaved(p.remoteView.keyHint) : s.keyPlaceholder}
                    spellCheck={false}
                    autoComplete="off"
                  />
                </label>
                <label className="field">
                  <span>{s.modelName}</span>
                  <input
                    value={model}
                    readOnly={!!busy}
                    onChange={(e) => setModel(e.target.value)}
                    placeholder={s.modelPlaceholder}
                    spellCheck={false}
                    autoComplete="off"
                  />
                </label>
                {result && <p role={result.ok ? 'status' : 'alert'} className={`meta ${result.ok ? 'okmsg' : 'err'}`}>{result.text}</p>}
                {!result && enabled && rs.state === 'fail' && <p role="alert" className="meta err">{rs.message}</p>}
                <div className="actions">
                  <button className="ghost" onClick={() => void test()} disabled={!!busy || !baseUrl.trim() || !model.trim()}>
                    {busy === 'test' ? s.testing : s.test}
                  </button>
                  {enabled && (
                    <button className="ghost" onClick={() => void disable()} disabled={!!busy}>
                      {s.disable}
                    </button>
                  )}
                  <button onClick={requestEnable} disabled={!!busy || consent || !baseUrl.trim() || !model.trim()}>
                    {busy === 'save' ? s.confirming : enabled ? s.save : s.saveEnable}
                  </button>
                </div>
                {enabled && (
                  <>
                    <label className="check">
                      <input type="checkbox" checked={p.localOnly} onChange={(e) => p.setLocalOnly(e.target.checked)} />
                      <span>{s.localOnly}</span>
                    </label>
                    <p className="hint">{s.localOnlyHint}</p>
                  </>
                )}
              </section>
            </>}
            {section === 'glossary' && <>
              <section>
                <h3>{s.glossary}</h3>
                <p className="hint">{s.glossaryHint}</p>
                <p className="hint">{s.glossaryRules}</p>
                <label className="field">
                  <span>{s.allMeetings}</span>
                  <textarea
                    rows={4}
                    value={p.glossary}
                    onChange={(e) => p.setGlossary(e.target.value)}
                    placeholder={s.glossaryPlaceholder}
                    spellCheck={false}
                    aria-invalid={glossaryErrors.length > 0 || undefined}
                    aria-describedby={glossaryErrors.length ? `${sectionId}-glossary-error` : undefined}
                  />
                </label>
                {glossaryErrors.length > 0 && <p id={`${sectionId}-glossary-error`} role="status" className="meta err">{s.glossaryInvalid(glossaryErrors.join(', '))}</p>}
                <label className="field">
                  <span>{s.thisMeeting}</span>
                  <textarea
                    rows={3}
                    value={p.meetingGlossary}
                    onChange={(e) => p.setMeetingGlossary(e.target.value)}
                    placeholder={s.thisMeetingPlaceholder}
                    spellCheck={false}
                    aria-invalid={meetingGlossaryErrors.length > 0 || undefined}
                    aria-describedby={meetingGlossaryErrors.length ? `${sectionId}-meeting-glossary-error` : undefined}
                  />
                </label>
                {meetingGlossaryErrors.length > 0 && <p id={`${sectionId}-meeting-glossary-error`} role="status" className="meta err">{s.glossaryInvalid(meetingGlossaryErrors.join(', '))}</p>}
              </section>
            </>}
            {section === 'maintenance' && <>
              <section>
                <h3>{s.feedback}</h3>
                <p className="hint">{s.feedbackHint}</p>
                <button type="button" onClick={p.onDiagnostics}>
                  {s.diagnostics}
                </button>
              </section>
              <section tabIndex={-1} aria-label={s.samples}>
                <h3>{s.samples}</h3>
                <p className="hint">{s.samplesHint}</p>
                {samplesError ? <>
                  <p role="alert" className="meta err">{s.samplesReadFailed(samplesError)}</p>
                  <button type="button" onClick={(event) => {
                    event.currentTarget.closest('section')?.focus()
                    setSamplesAttempt((n) => n + 1)
                  }}>{s.samplesRetry}</button>
                </> : <p role="status" className="meta">{samples === null ? s.samplesCounting : s.samplesCount(samples)}</p>}
                <div className="actions">
                  <button type="button" onClick={() => void exportSamples('cases')} disabled={!samples || exporting}>
                    {s.exportCases}
                  </button>
                  <button type="button" className="ghost" onClick={() => void exportSamples('pairs')} disabled={!samples || exporting}>
                    {s.exportPairs}
                  </button>
                </div>
                {exporting && <p role="status" className="meta">{s.samplesExporting}</p>}
                <p className="hint">
                  {s.samplesCases}
                  <code>node eval/run.ts --file {s.samplesFile}</code>
                </p>
                <p className="hint">{s.samplesPairs}</p>
                {exported && <p role={exported.ok ? 'status' : 'alert'} className={`meta ${exported.ok ? 'okmsg' : 'err'}`}>{exported.ok ? exported.text : s.samplesExportFailed(exported.text)}</p>}
              </section>
              {isDesktop && (
                <section>
                  <h3>{s.updates}</h3>
                  <p className="hint">{s.version(version)}</p>
                  <label className="check">
                    <input type="checkbox" checked={p.autoUpdate} onChange={(e) => p.setAutoUpdate(e.target.checked)} />
                    <span>{s.autoUpdate}</span>
                  </label>
                  {checked && <p className={`meta ${checked.ok ? 'okmsg' : 'err'}`}>{checked.text}</p>}
                  <div className="actions">
                    <button className="ghost" onClick={() => void checkNow()} disabled={checking}>
                      {checking ? s.checking : s.checkNow}
                    </button>
                  </div>
                </section>
              )}
            </>}
          </div>
        </div>
      </div>
      {consent && (
        <Dialog label={s.consentAria} role="alertdialog" onClose={() => setConsent(false)}>
          <div className="panel consent">
            <p>
              {s.consentSent} <b>{hostOf(baseUrl)}</b>
              {s.consentSentEnd}
            </p>
            <ul>
              {s.sent.map((x) => (
                <li key={x}>{x}</li>
              ))}
            </ul>
            <p>{s.consentKeptTitle}</p>
            <ul>
              {s.kept.map((x) => (
                <li key={x}>{x}</li>
              ))}
            </ul>
            <p className="hint">{s.consentHint}</p>
            <div className="actions">
              <button className="ghost" onClick={() => setConsent(false)}>
                {s.notNow}
              </button>
              <button
                onClick={() => {
                  rememberConsent(baseUrl)
                  void saveAndEnable()
                }}
              >
                {s.consentOk}
              </button>
            </div>
          </div>
        </Dialog>
      )}
      {discard && <Dialog label={s.discardTitle} role="alertdialog" onClose={() => setDiscard(false)}>
        <div className="panel">
          <h2>{s.discardTitle}</h2>
          <p className="hint">{s.discardHint}</p>
          <div className="actions">
            <button type="button" onClick={() => setDiscard(false)}>{s.keepEditing}</button>
            <button type="button" className="ghost" onClick={p.onClose}>{s.discardChanges}</button>
          </div>
        </div>
      </Dialog>}
    </Dialog>
  )
}

/** Interface language: system default or a pinned one. Names are shown in their own language. */
function LanguagePicker() {
  const t = useT()
  const choice = useLangChoice()
  return (
    <section>
      <h3>{t.lang.title}</h3>
      <div className="seg sources" role="radiogroup" aria-label={t.lang.title}>
        {(['system', ...LANGS] as const).map((v) => (
          <button key={v} type="button" role="radio" aria-checked={choice === v} className={choice === v ? 'on' : ''} onClick={() => setLangChoice(v)}>
            {v === 'system' ? t.lang.system : LANG_NAMES[v]}
          </button>
        ))}
      </div>
      <p className="hint">{t.lang.hint}</p>
    </section>
  )
}
