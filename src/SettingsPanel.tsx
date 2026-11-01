// 设置：不常用的东西都放这里，主页面只留说话、收音和板书本身。

import { useEffect, useState } from 'react'
import { type LlmStatus, type UpdateInfo, type VoiceSource, isDesktop, shell, updates, voice } from './desktop.ts'
import { accelFrom } from './engine/shortcuts.ts'
import { LANGS, LANG_NAMES, setLangChoice, useLangChoice, useT } from './i18n/index.ts'
import { hostOf, needsConsent, rememberConsent } from './privacy.ts'
import { type RemoteTest, type RemoteView, remote } from './remote.ts'

export type RemoteState = { state: 'off' | 'checking' | 'ok' | 'fail'; ms?: number; message?: string }

interface Props {
  hotkey: string
  setHotkey: (v: string) => void
  hotkeyErr: string | null
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
  const [wayland, setWayland] = useState(false)
  useEffect(() => {
    if (isDesktop) void shell.wayland().then(setWayland)
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
      await remote.save({ ...draft(), enabled: false }).catch(() => undefined)
      setApiKey('')
      setResult({ ok: false, text: errText(e) })
    } finally {
      setBusy(null)
      p.onRemoteChanged()
    }
  }

  async function disable() {
    await remote.save({ ...draft(), enabled: false })
    setResult(null)
    p.onRemoteChanged()
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
  const [exporting, setExporting] = useState(false)
  const [exported, setExported] = useState<{ ok: boolean; text: string } | null>(null)
  // 打开设置时数一次（要把所有会议读一遍）
  useEffect(() => {
    void p.countSamples().then(setSamples, () => setSamples(0))
  }, [])
  async function exportSamples(kind: 'cases' | 'pairs') {
    setExporting(true)
    setExported(null)
    try {
      const where = await p.exportSamples(kind)
      if (where) setExported({ ok: true, text: s.samplesSaved(where) })
    } catch (e) {
      setExported({ ok: false, text: errText(e) })
    } finally {
      setExporting(false)
    }
  }

  const b = p.builtin
  const rs = p.remoteState
  const enabled = p.remoteView?.enabled

  return (
    <div className="modal" role="dialog" aria-label={s.title} onClick={(e) => e.target === e.currentTarget && p.onClose()}>
      <div className="panel settings">
        <header>
          <h2>{s.title}</h2>
          <button className="ghost" onClick={p.onClose}>
            {s.close}
          </button>
        </header>

        <LanguagePicker />

        <section>
          <h3>{s.local}</h3>
          {isDesktop ? (
            <>
              <p className="hint">
                {b?.state === 'ready' && s.builtinReady(b.model ?? '', b.device ?? '', p.warming)}
                {b?.state === 'idle' && s.builtinIdle(b.model ?? '')}
                {b?.state === 'starting' && s.builtinStarting(b.model ?? '', b.device ?? '')}
                {(b?.state === 'no-model' || b?.state === 'error') && b.message}
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
              />
            </label>
          )}
        </section>

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
              onChange={(e) => setModel(e.target.value)}
              placeholder={s.modelPlaceholder}
              spellCheck={false}
              autoComplete="off"
            />
          </label>
          {consent && (
            <div className="consent" role="alertdialog" aria-label={s.consentAria}>
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
          )}
          {result && <p className={`meta ${result.ok ? 'okmsg' : 'err'}`}>{result.text}</p>}
          {!result && enabled && rs.state === 'fail' && <p className="meta err">{rs.message}</p>}
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
                  value={p.hotkey}
                  readOnly
                  placeholder={s.hotkeyOff}
                  aria-label={s.hotkeyAria}
                  onKeyDown={(e) => {
                    if (e.key === 'Tab') return
                    e.preventDefault()
                    if (e.key === 'Backspace' || e.key === 'Delete') p.setHotkey('')
                    const a = accelFrom(e)
                    if (a) p.setHotkey(a)
                  }}
                />
              </label>
              <p className="hint">{s.hotkeyHint}</p>
              {p.hotkeyErr && <p className="meta err">{p.hotkeyErr}</p>}
              {wayland && (
                <p className="hint">
                  {s.wayland1} <code>boardcast --toggle-mic</code>{s.wayland2}
                </p>
              )}
            </>
          )}
        </section>

        <section>
          <h3>{s.feedback}</h3>
          <p className="hint">{s.feedbackHint}</p>
          <button type="button" onClick={p.onDiagnostics}>
            {s.diagnostics}
          </button>
        </section>
        <section>
          <h3>{s.samples}</h3>
          <p className="hint">{s.samplesHint}</p>
          <p className="meta">{samples === null ? s.samplesCounting : s.samplesCount(samples)}</p>
          <div className="actions">
            <button type="button" onClick={() => void exportSamples('cases')} disabled={!samples || exporting}>
              {s.exportCases}
            </button>
            <button type="button" className="ghost" onClick={() => void exportSamples('pairs')} disabled={!samples || exporting}>
              {s.exportPairs}
            </button>
          </div>
          <p className="hint">
            {s.samplesCases}
            <code>node eval/run.ts --file {s.samplesFile}</code>
          </p>
          <p className="hint">{s.samplesPairs}</p>
          {exported && <p className={`meta ${exported.ok ? 'okmsg' : 'err'}`}>{exported.text}</p>}
        </section>
        <section>
          <h3>{s.glossary}</h3>
          <p className="hint">{s.glossaryHint}</p>
          <label className="field">
            <span>{s.allMeetings}</span>
            <textarea
              rows={4}
              value={p.glossary}
              onChange={(e) => p.setGlossary(e.target.value)}
              placeholder={s.glossaryPlaceholder}
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span>{s.thisMeeting}</span>
            <textarea
              rows={3}
              value={p.meetingGlossary}
              onChange={(e) => p.setMeetingGlossary(e.target.value)}
              placeholder={s.thisMeetingPlaceholder}
              spellCheck={false}
            />
          </label>
        </section>

        {isDesktop && (
          <section>
            <h3>{s.audio}</h3>
            <div className="seg sources" role="radiogroup" aria-label={s.sourceAria}>
              {(['mic', 'both', 'system'] as const).map((v) => (
                <button key={v} type="button" role="radio" aria-checked={p.source === v} className={p.source === v ? 'on' : ''} onClick={() => p.setSource(v)}>
                  {s.sources[v]}
                </button>
              ))}
            </div>
            <p className="hint">{s.sourceHint}</p>
            <select value={p.mic} onChange={(e) => p.setMic(e.target.value)} aria-label={s.micAria}>
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

        {p.inkMode && (
          <section>
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
          </section>
        )}

      </div>
    </div>
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
