// 设置：不常用的东西都放这里，主页面只留说话、收音和板书本身。

import { useEffect, useState } from 'react'
import { type LlmStatus, type UpdateInfo, type VoiceSource, isDesktop, updates, voice } from './desktop.ts'
import { KEPT, SENT, hostOf, needsConsent, rememberConsent } from './privacy.ts'
import { type RemoteTest, type RemoteView, remote } from './remote.ts'

export type RemoteState = { state: 'off' | 'checking' | 'ok' | 'fail'; ms?: number; message?: string }

interface Props {
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
}

/** 错误对象只要它的消息，不要前面的「Error: 」 */
export function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

function messyName(n: number) {
  return n < 25 ? '工整' : n < 45 ? '随手' : n < 70 ? '连写' : '龙飞凤舞'
}

export default function SettingsPanel(p: Props) {
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
      const t = await remote.test(draft())
      setResult({ ok: true, text: `连上了：${t.ms} ms，回复「${t.reply}」` })
      return t
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
      const t = await remote.test(draft()).catch((e) => {
        throw new Error(`没启用：${errText(e)}`)
      })
      await remote.save({ ...draft(), enabled: true })
      setApiKey('')
      setResult({ ok: true, text: `已启用：${t.ms} ms，之后优先用这个模型出板书，出错时这一句改用本地模型` })
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
      else setChecked({ ok: true, text: '已经是最新版' })
    } catch (e) {
      setChecked({ ok: false, text: errText(e) })
    } finally {
      setChecking(false)
    }
  }

  const b = p.builtin
  const rs = p.remoteState
  const enabled = p.remoteView?.enabled

  return (
    <div className="modal" role="dialog" aria-label="设置" onClick={(e) => e.target === e.currentTarget && p.onClose()}>
      <div className="panel settings">
        <header>
          <h2>设置</h2>
          <button className="ghost" onClick={p.onClose}>
            关闭
          </button>
        </header>

        <section>
          <h3>本地模型</h3>
          {isDesktop ? (
            <>
              <p className="hint">
                {b?.state === 'ready' && `内置：${b.model} · ${b.device}${p.warming ? ' · 预热中…' : ''}`}
                {b?.state === 'idle' && `内置：${b.model} · 待命（正在用远程模型，远程出错时自动启动兜底）`}
                {b?.state === 'starting' && `正在启动 ${b.model ?? ''}（${b.device ?? ''}）…`}
                {(b?.state === 'no-model' || b?.state === 'error') && b.message}
                {!b && '读取中…'}
              </p>
              {p.localUrl && <code className="addr">{p.localUrl}</code>}
              <div className="actions">
                <button onClick={p.onManageModels}>管理模型</button>
              </div>
            </>
          ) : (
            <label className="field">
              <span>
                llama.cpp 服务地址 <i className={`dot ${p.localOnline === null ? '' : p.localOnline ? 'ok' : 'bad'}`} />
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
            远程模型（OpenAI 兼容）
            {enabled && rs.state === 'ok' && <span className="tag on">正在使用</span>}
            {enabled && rs.state === 'fail' && <span className="tag err">连不上，暂时用本地</span>}
            {enabled && rs.state === 'checking' && <span className="tag">检查中…</span>}
          </h3>
          <p className="hint">
            填了并确认能连上以后，优先用它出板书；某一句请求失败时，这一句改用本地模型。远程接口没有本地那样的输出语法约束，效果取决于模型本身。
            {!isDesktop && ' 网页版的 key 存在这台电脑的浏览器里，而且很多服务商不允许浏览器直接调用，建议用桌面版。'}
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
              placeholder={p.remoteView?.keyHint ? `已保存 ${p.remoteView.keyHint}，不改就留空` : 'sk-…（本地服务不需要可以不填）'}
              spellCheck={false}
              autoComplete="off"
            />
          </label>
          <label className="field">
            <span>模型名</span>
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="比如 gpt-4o-mini、deepseek-chat、qwen-plus"
              spellCheck={false}
              autoComplete="off"
            />
          </label>
          {consent && (
            <div className="consent" role="alertdialog" aria-label="启用远程模型前确认">
              <p>
                启用以后，开会时这些内容会发到 <b>{hostOf(baseUrl)}</b>：
              </p>
              <ul>
                {SENT.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ul>
              <p>不会发出去的：</p>
              <ul>
                {KEPT.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ul>
              <p className="hint">涉及敏感内容的会，可以在下面勾「这场会只用本地模型」。</p>
              <div className="actions">
                <button className="ghost" onClick={() => setConsent(false)}>
                  先不用
                </button>
                <button
                  onClick={() => {
                    rememberConsent(baseUrl)
                    void saveAndEnable()
                  }}
                >
                  知道了，启用
                </button>
              </div>
            </div>
          )}
          {result && <p className={`meta ${result.ok ? 'okmsg' : 'err'}`}>{result.text}</p>}
          {!result && enabled && rs.state === 'fail' && <p className="meta err">{rs.message}</p>}
          <div className="actions">
            <button className="ghost" onClick={() => void test()} disabled={!!busy || !baseUrl.trim() || !model.trim()}>
              {busy === 'test' ? '测试中…' : '测试连接'}
            </button>
            {enabled && (
              <button className="ghost" onClick={() => void disable()} disabled={!!busy}>
                停用
              </button>
            )}
            <button onClick={requestEnable} disabled={!!busy || consent || !baseUrl.trim() || !model.trim()}>
              {busy === 'save' ? '确认中…' : enabled ? '保存' : '保存并启用'}
            </button>
          </div>
          {enabled && (
            <>
              <label className="check">
                <input type="checkbox" checked={p.localOnly} onChange={(e) => p.setLocalOnly(e.target.checked)} />
                <span>这场会只用本地模型</span>
              </label>
              <p className="hint">按会议记住：勾上以后这场会的板书和摘要都不会发到远程接口，别的会照常用远程。</p>
            </>
          )}
        </section>

        <section>
          <h3>术语表</h3>
          <p className="hint">
            公司内部的叫法（项目代号、服务名、英文缩写）。一行一条，写成「正确写法 = 常听错的写法, 另一种错写」，只写正确写法也行。说的和打的话都会先按它纠正（英文不分大小写、空格可有可无），模型起名也照这个写法。
          </p>
          <label className="field">
            <span>所有会议</span>
            <textarea
              rows={4}
              value={p.glossary}
              onChange={(e) => p.setGlossary(e.target.value)}
              placeholder={'MediaCrawler = 媒体爬虫, 美的爬虫\ngraphRAG\nClickHouse = 克里克豪斯'}
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span>只用于这场会</span>
            <textarea
              rows={3}
              value={p.meetingGlossary}
              onChange={(e) => p.setMeetingGlossary(e.target.value)}
              placeholder="在白板上选中一个名字，点「加进术语」也会加到这里"
              spellCheck={false}
            />
          </label>
        </section>

        {isDesktop && (
          <section>
            <h3>收音</h3>
            <div className="seg sources" role="radiogroup" aria-label="收哪些声音">
              {(
                [
                  ['mic', '麦克风'],
                  ['both', '麦克风 + 系统声音'],
                  ['system', '只收系统声音'],
                ] as const
              ).map(([v, label]) => (
                <button key={v} type="button" role="radio" aria-checked={p.source === v} className={p.source === v ? 'on' : ''} onClick={() => p.setSource(v)}>
                  {label}
                </button>
              ))}
            </div>
            <p className="hint">
              线上会议选「麦克风 + 系统声音」：自己说的从麦克风收，别人说的从扬声器里收（Linux 收默认输出设备，Windows 收默认扬声器；macOS 暂时不支持）。外放时麦克风会把别人的声音再收一遍，戴耳机效果最好。
            </p>
            <select value={p.mic} onChange={(e) => p.setMic(e.target.value)} aria-label="麦克风">
              <option value="">系统默认</option>
              {devices.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
              {p.mic && !devices.includes(p.mic) && <option value={p.mic}>{p.mic}（没找到，会改用系统默认）</option>}
            </select>
            <p className="hint">换了麦克风或者收音来源，下次点「开始收音」生效。</p>
            <label className="check">
              <input type="checkbox" checked={p.diarize} onChange={(e) => p.setDiarize(e.target.checked)} />
              <span>按声音自动区分说话人</span>
            </label>
            <p className="hint">新出现的声音依次记成 A、B、C；在会议记录里点头像可以改成真名，同一个声音以后都用这个名字。很短的句子（「嗯」「对」）算给上一个说话的人。</p>
          </section>
        )}

        {isDesktop && (
          <section>
            <h3>更新</h3>
            <p className="hint">
              当前版本 {version || '…'}。新版本从 GitHub Release 下载，装之前会校验签名。
            </p>
            <label className="check">
              <input type="checkbox" checked={p.autoUpdate} onChange={(e) => p.setAutoUpdate(e.target.checked)} />
              <span>打开应用时自动检查更新</span>
            </label>
            {checked && <p className={`meta ${checked.ok ? 'okmsg' : 'err'}`}>{checked.text}</p>}
            <div className="actions">
              <button className="ghost" onClick={() => void checkNow()} disabled={checking}>
                {checking ? '检查中…' : '检查更新'}
              </button>
            </div>
          </section>
        )}

        {p.inkMode && (
          <section>
            <label className="messy">
              <span>
                手写潦草度 <b>{p.messy}</b>
                <i>{messyName(p.messy)}</i>
              </span>
              <input
                type="range"
                min={0}
                max={100}
                value={p.messy}
                onChange={(e) => p.setMessy(Number(e.target.value))}
                aria-label="潦草度"
              />
            </label>
            <label className="messy">
              <span>
                笔画粗细 <b>{p.weight}%</b>
                <i>{p.weight < 80 ? '细' : p.weight <= 120 ? '适中' : p.weight <= 160 ? '粗' : '很粗'}</i>
              </span>
              <input
                type="range"
                min={50}
                max={200}
                step={5}
                value={p.weight}
                onChange={(e) => p.setWeight(Number(e.target.value))}
                aria-label="笔画粗细"
              />
            </label>
          </section>
        )}

      </div>
    </div>
  )
}
