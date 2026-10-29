// 设置：不常用的东西都放这里，主页面只留说话、收音和板书本身。

import { useEffect, useState } from 'react'
import { type LlmStatus, isDesktop, voice } from './desktop.ts'
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
  // 语音
  mic: string
  setMic: (m: string) => void
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

  // 先确认能连上再启用；连不上也把填的内容存下来，免得白填
  async function saveAndEnable() {
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
            <button onClick={() => void saveAndEnable()} disabled={!!busy || !baseUrl.trim() || !model.trim()}>
              {busy === 'save' ? '确认中…' : enabled ? '保存' : '保存并启用'}
            </button>
          </div>
        </section>

        {isDesktop && (
          <section>
            <h3>麦克风</h3>
            <select value={p.mic} onChange={(e) => p.setMic(e.target.value)} aria-label="麦克风">
              <option value="">系统默认</option>
              {devices.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
              {p.mic && !devices.includes(p.mic) && <option value={p.mic}>{p.mic}（没找到，会改用系统默认）</option>}
            </select>
            <p className="hint">换了麦克风以后，下次点「开始收音」生效。</p>
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
