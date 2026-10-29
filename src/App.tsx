import { Excalidraw } from '@excalidraw/excalidraw'
import '@excalidraw/excalidraw/index.css'
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import cases from '../eval/cases.json'
import { layout } from './board/layout.ts'
import { type Look, scene } from './board/scene.ts'
import { type Heard, type LlmStatus, type VoiceStatus, isDesktop, llmStatus, on, voice } from './desktop.ts'
import { type Graph, emptyGraph } from './engine/graph.ts'
import { type LlmConfig, proposeOps } from './engine/llm.ts'
import { formatOps } from './engine/ops.ts'
import type { Utterance } from './engine/prompt.ts'
import { Session } from './engine/session.ts'
import InkBoard from './ink/InkBoard.tsx'
import ModelPanel from './ModelPanel.tsx'
import { type RemoteView, remote } from './remote.ts'
import SettingsPanel, { type RemoteState, errText } from './SettingsPanel.tsx'

type Mode = 'pencil' | 'ink'
type Theme = 'system' | 'light' | 'dark'

interface Line extends Utterance {
  /** 模型处理这句话用了多久；undefined 表示还在排队 */
  ms?: number
  ops?: string
  /** 哪个模型画的；远程失败改用本地时标出来 */
  via?: string
  fellBack?: boolean
}

const LIGHT: Look = { ink: '#1e1e1e', pencil: '#868e96', highlight: '#ffec99', group: '#1971c2', question: '#e03131', note: '#1971c2' }
const DARK: Look = { ink: '#e9ecef', pencil: '#868e96', highlight: '#5c4a00', group: '#74c0fc', question: '#ff8787', note: '#74c0fc' }

const THEMES: Theme[] = ['system', 'light', 'dark']
const THEME_LABEL: Record<Theme, string> = { system: '◐ 跟随系统', light: '☀ 浅色', dark: '☾ 深色' }

function load(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback
  } catch {
    return fallback
  }
}

function save(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* 存不了就算了 */
  }
}

function useSystemDark(): boolean {
  const q = '(prefers-color-scheme: dark)'
  const [dark, setDark] = useState(() => matchMedia(q).matches)
  useEffect(() => {
    const m = matchMedia(q)
    const on = () => setDark(m.matches)
    m.addEventListener('change', on)
    return () => m.removeEventListener('change', on)
  }, [])
  return dark
}

export default function App() {
  // —— 主题：跟随系统 / 浅色 / 深色，写在 <html data-theme> 上，CSS 变量跟着变 ——
  const [theme, setTheme] = useState<Theme>(() => {
    const t = load('boardcast.theme', 'system')
    return (THEMES as string[]).includes(t) ? (t as Theme) : 'system'
  })
  const systemDark = useSystemDark()
  const dark = theme === 'dark' || (theme === 'system' && systemDark)
  useEffect(() => {
    const el = document.documentElement
    if (theme === 'system') delete el.dataset.theme
    else el.dataset.theme = theme
    save('boardcast.theme', theme)
  }, [theme])

  // 桌面版一开始留空，等内置模型就绪后由外壳告诉我们地址；网页版用上次填的
  const [baseUrl, setBaseUrl] = useState(() => (isDesktop ? '' : load('boardcast.llm', 'http://127.0.0.1:8080')))
  const [online, setOnline] = useState<boolean | null>(null)
  // 桌面版：内置的 llama-server 由外壳拉起，就绪后自动用它的地址
  const [builtin, setBuiltin] = useState<LlmStatus | null>(null)
  // 第一次请求很慢（Vulkan 现场编译着色器，提示词前缀也还没进缓存），就绪后先空跑一次
  const [warming, setWarming] = useState(false)
  const [showModels, setShowModels] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  // 模型管理面板：第一次发现没有模型时自动打开（已经启用了能用的远程模型就不打扰）
  const askedForModel = useRef(false)
  const warmed = useRef<string | null>(null)
  useEffect(() => {
    if (!isDesktop) return
    let alive = true
    let timer: ReturnType<typeof setTimeout>
    const poll = async () => {
      const s = await llmStatus().catch(() => null)
      if (!alive) return
      setBuiltin(s)
      if (s?.state === 'ready' && s.url) {
        setBaseUrl(s.url)
        if (warmed.current !== s.url) {
          warmed.current = s.url
          setWarming(true)
          void proposeOps({ baseUrl: s.url, noThink: true }, emptyGraph(), [], { text: '大家好。' })
            .catch(() => undefined)
            .finally(() => alive && setWarming(false))
        }
      }
      // 还在启动就勤快点问；定下来以后慢慢问
      timer = setTimeout(poll, s?.state === 'starting' ? 700 : 5000)
    }
    void poll()
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [])

  // —— 远程模型：启用了就先确认能连上，能连上才优先用 ——
  const [remoteView, setRemoteView] = useState<RemoteView | null>(null)
  const [remoteState, setRemoteState] = useState<RemoteState>({ state: 'off' })
  const checkRemote = useCallback(async () => {
    const v = await remote.get().catch(() => null)
    setRemoteView(v)
    if (!v?.enabled) {
      setRemoteState({ state: 'off' })
      return
    }
    setRemoteState({ state: 'checking' })
    try {
      const t = await remote.test()
      setRemoteState({ state: 'ok', ms: t.ms })
    } catch (e) {
      setRemoteState({ state: 'fail', message: errText(e) })
    }
  }, [])
  useEffect(() => {
    void checkRemote()
  }, [checkRemote])
  const useRemote = remoteState.state === 'ok' && !!remoteView

  useEffect(() => {
    if (builtin?.state === 'no-model' && !askedForModel.current && remoteState.state !== 'checking' && !useRemote) {
      askedForModel.current = true
      setShowModels(true)
    }
  }, [builtin?.state, remoteState.state, useRemote])

  // 实际用来出板书的接口：远程优先，本地兜底
  const llm = useMemo<LlmConfig>(() => {
    const local: LlmConfig = { baseUrl, noThink: true, label: isDesktop ? '内置' : '本地' }
    if (!useRemote || !remoteView) return local
    return {
      baseUrl: remoteView.baseUrl,
      model: remoteView.model,
      flavor: 'openai',
      send: remote.send,
      label: `远程 ${remoteView.model}`,
      fallback: baseUrl ? local : undefined,
    }
  }, [baseUrl, useRemote, remoteView])

  const [lines, setLines] = useState<Line[]>([])
  const [graph, setGraph] = useState<Graph>(emptyGraph)
  const [hot, setHot] = useState<Set<string>>(new Set())
  const [speaker, setSpeaker] = useState('A')
  const [text, setText] = useState('')
  const [demo, setDemo] = useState(cases[0].id)
  const [playing, setPlaying] = useState(false)
  // 用 state 而不是 ref：切回铅笔模式时 Excalidraw 重新挂载，要让重画的 effect 再跑一次
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null)
  const [mode, setMode] = useState<Mode>(() => (load('boardcast.mode', 'ink') === 'pencil' ? 'pencil' : 'ink'))
  const [messy, setMessy] = useState(() => {
    const v = Number(load('boardcast.messy', '30'))
    return Number.isFinite(v) ? Math.min(100, Math.max(0, v)) : 30
  })
  // 拖滑杆时每一格都要重算全部笔画，让它慢半拍，拖动本身不卡
  const messyLater = useDeferredValue(messy)
  const [micChoice, setMicChoice] = useState(() => load('boardcast.mic', ''))
  const listEnd = useRef<HTMLDivElement>(null)

  const session = useMemo(
    () =>
      new Session(llm, {
        graph: (g) => setGraph(g),
        mention: (ids) => {
          setHot(new Set(ids))
          if (ids.length) setTimeout(() => setHot(new Set()), 1500)
        },
        llm: (r) =>
          setLines((ls) => {
            // 一批里的每句都标上这一轮的耗时
            const out = [...ls]
            let left = r.batch.length
            for (let i = out.length - 1; i >= 0 && left > 0; i--) {
              if (out[i].ms === undefined) {
                out[i] = {
                  ...out[i],
                  ms: r.ms,
                  ops: left === r.batch.length ? formatOps(r.ops) || '-' : '',
                  via: r.via,
                  fellBack: !!r.fellBack,
                }
                left--
              }
            }
            return out
          }),
        error: (e) => {
          console.error(e)
          setOnline(false)
        },
      }),
    [llm],
  )

  useEffect(() => {
    // 桌面版的地址是内置服务每次启动时随机分配的端口，记下来下次也用不上
    if (!isDesktop) save('boardcast.llm', baseUrl)
    if (!baseUrl) {
      setOnline(false)
      return
    }
    let alive = true
    const ping = () =>
      fetch(`${baseUrl}/health`)
        .then((r) => alive && setOnline(r.ok))
        .catch(() => alive && setOnline(false))
    void ping()
    const t = setInterval(ping, 5000)
    return () => {
      alive = false
      clearInterval(t)
    }
  }, [baseUrl])

  // 图变了就重新排版、重画
  useEffect(() => {
    let stale = false
    void layout(graph).then((l) => {
      if (stale || !api) return
      const look = dark ? DARK : LIGHT
      api.updateScene({
        elements: scene(graph, l, hot, look),
        appState: { viewBackgroundColor: dark ? '#121212' : '#fdfcf8' },
      })
      if (!graph.nodes.length) return
      // 自己算缩放和居中：内容少时不放大到 100% 以上，内容多时整张图塞进画面
      const { x0, y0, x1, y1 } = l.bounds
      const { width, height, zoom } = api.getAppState()
      const z = Math.min(1, (0.85 * width) / (x1 - x0), (0.8 * height) / (y1 - y0))
      api.updateScene({
        appState: {
          zoom: { value: z as typeof zoom.value },
          scrollX: width / (2 * z) - (x0 + x1) / 2,
          scrollY: height / (2 * z) - (y0 + y1) / 2,
        },
      })
    })
    return () => {
      stale = true
    }
  }, [graph, hot, dark, api])

  useEffect(() => {
    save('boardcast.mode', mode)
    save('boardcast.messy', String(messy))
    save('boardcast.mic', micChoice)
  }, [mode, messy, micChoice])

  // 新版浏览器的 scrollIntoView 会返回 Promise，不能直接当 effect 的返回值
  useEffect(() => {
    listEnd.current?.scrollIntoView({ block: 'end' })
  }, [lines])

  function say(u: Utterance) {
    setLines((ls) => [...ls, u])
    session.say(u)
  }

  // —— 语音输入（桌面版）：外壳负责收音、断句、识别，这里只接结果 ——
  const [listening, setListening] = useState(false)
  const [level, setLevel] = useState(0)
  const [partial, setPartial] = useState('')
  const [mic, setMic] = useState<string | undefined>()
  const [voiceErr, setVoiceErr] = useState<string | null>(null)
  // 监听器只注册一次，用 ref 拿最新的 say 和说话人
  const sayRef = useRef(say)
  sayRef.current = say
  const speakerRef = useRef(speaker)
  speakerRef.current = speaker
  useEffect(() => {
    if (!isDesktop) return
    const offs: Promise<() => void>[] = [
      on<Heard>('voice-final', (h) => {
        setPartial('')
        sayRef.current({ speaker: speakerRef.current.trim() || undefined, text: h.text })
      }),
      on<string>('voice-partial', setPartial),
      on<number>('voice-level', setLevel),
      on<VoiceStatus>('voice-status', (s) => {
        setListening(s.listening)
        setMic(s.device)
        if (!s.listening) {
          setLevel(0)
          setPartial('')
        }
        if (s.message) setVoiceErr(s.message)
      }),
      on<string>('voice-error', setVoiceErr),
    ]
    return () => offs.forEach((p) => void p.then((off) => off()))
  }, [])

  async function toggleMic() {
    setVoiceErr(null)
    if (listening) {
      await voice.stop()
      return
    }
    const s = await voice.status()
    if (!s.installed) {
      setVoiceErr('先下载语音识别模型')
      setShowModels(true)
      return
    }
    await voice.start(micChoice || undefined).catch((e) => setVoiceErr(errText(e)))
  }

  function reset() {
    session.graph = emptyGraph()
    session.history = []
    setGraph(session.graph)
    setLines([])
  }

  async function play() {
    const c = cases.find((x) => x.id === demo)
    if (!c) return
    reset()
    setPlaying(true)
    for (const t of c.turns) {
      say(t)
      await new Promise((r) => setTimeout(r, 2500))
    }
    setPlaying(false)
  }

  // 顶上那一行：现在是哪个模型在画
  const ready = useRemote || !!online
  let status: string
  if (useRemote) status = `远程：${remoteView?.model}`
  else if (remoteState.state === 'checking') status = '正在检查远程模型…'
  else if (builtin?.state === 'ready') status = `内置：${builtin.model?.replace(/\.gguf$/, '')}${warming ? ' · 预热中' : ''}`
  else if (builtin?.state === 'starting') status = `正在启动 ${builtin.model?.replace(/\.gguf$/, '') ?? '内置模型'}…`
  else if (builtin) status = builtin.message ?? '内置模型不可用'
  else status = online ? `本地：${baseUrl}` : '没有连上模型'
  const remoteBroken = remoteView?.enabled && remoteState.state === 'fail'

  return (
    <div className="app">
      <aside className="side">
        <header className="top">
          <div>
            <h1>boardcast</h1>
            <p className="sub">说话，板书自己长出来</p>
          </div>
          <div className="tools">
            <button
              className="ghost"
              onClick={() => setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length])}
              title="切换主题"
              aria-label={`主题：${THEME_LABEL[theme]}，点一下切换`}
            >
              {THEME_LABEL[theme]}
            </button>
            <button className="ghost" onClick={() => setShowSettings(true)} aria-label="设置">
              ⚙ 设置
            </button>
          </div>
        </header>

        <button type="button" className={`status ${ready ? 'ok' : ''}`} onClick={() => setShowSettings(true)}>
          <i className={`dot ${ready ? 'ok' : online === false ? 'bad' : ''}`} />
          <span>{status}</span>
          {remoteBroken && <em>远程连不上，暂用本地</em>}
        </button>

        <div className="transcript">
          {lines.length === 0 && !partial && (
            <p className="empty">
              {isDesktop
                ? '点「开始收音」直接说话，也可以在下面打字。示例会议在设置里。'
                : '在下面输入一句话。示例会议在设置里；语音输入要用桌面版。'}
            </p>
          )}
          {lines.map((l, i) => (
            <div key={i} className="line">
              <div>
                {l.speaker && <b>{l.speaker}：</b>}
                {l.text}
              </div>
              <div className="meta">
                {l.ms === undefined ? '画着…' : `${Math.round(l.ms)} ms`}
                {l.ms !== undefined && useRemote && l.via && ` · ${l.via}`}
                {l.fellBack && ' · 远程失败，这句用了本地模型'}
                {l.ops ? <pre>{l.ops}</pre> : null}
              </div>
            </div>
          ))}
          {partial && (
            <div className="line partial" aria-live="polite">
              {speaker.trim() && <b>{speaker.trim()}：</b>}
              {partial}…
            </div>
          )}
          <div ref={listEnd} />
        </div>

        {isDesktop && (
          <div className="mic">
            <button type="button" className={listening ? 'on' : ''} onClick={() => void toggleMic()} aria-pressed={listening}>
              {listening ? '停止收音' : '开始收音'}
            </button>
            <span className="meter" aria-hidden>
              {/* 说话时 RMS 大概在 0.02～0.2，放大一点再开根号，小声也看得见 */}
              <i style={{ width: `${Math.min(100, Math.sqrt(level * 8) * 100).toFixed(0)}%` }} />
            </span>
            <small>{voiceErr ?? (listening ? mic : '')}</small>
          </div>
        )}

        <form
          className="say"
          onSubmit={(e) => {
            e.preventDefault()
            if (!text.trim()) return
            say({ speaker: speaker.trim() || undefined, text: text.trim() })
            setText('')
          }}
        >
          <input className="who" value={speaker} onChange={(e) => setSpeaker(e.target.value)} aria-label="说话人" autoComplete="off" />
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="说一句…（回车）"
            aria-label="说的话"
            autoComplete="off"
          />
          {/* 表单里有两个输入框时，没有提交按钮回车就不会提交 */}
          <button type="submit" disabled={!text.trim()}>
            说
          </button>
        </form>
        <div className="row">
          <div className="seg" role="radiogroup" aria-label="板书风格">
            <button role="radio" aria-checked={mode === 'ink'} className={mode === 'ink' ? 'on' : ''} onClick={() => setMode('ink')}>
              手写
            </button>
            <button
              role="radio"
              aria-checked={mode === 'pencil'}
              className={mode === 'pencil' ? 'on' : ''}
              onClick={() => setMode('pencil')}
            >
              铅笔 · 可编辑
            </button>
          </div>
          <button className="ghost" onClick={reset}>
            清空白板
          </button>
        </div>
      </aside>

      <main className="board">
        {mode === 'ink' ? (
          <InkBoard graph={graph} hot={hot} dark={dark} messy={messyLater / 100} />
        ) : (
          <Excalidraw
            excalidrawAPI={setApi}
            theme={dark ? 'dark' : 'light'}
            initialData={{ appState: { viewBackgroundColor: dark ? '#121212' : '#fdfcf8', currentItemFontFamily: 1 } }}
            UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false } }}
          />
        )}
      </main>
      {showSettings && (
        <SettingsPanel
          onClose={() => setShowSettings(false)}
          builtin={builtin}
          warming={warming}
          localUrl={baseUrl}
          setLocalUrl={setBaseUrl}
          localOnline={online}
          onManageModels={() => {
            setShowSettings(false)
            setShowModels(true)
          }}
          remoteView={remoteView}
          remoteState={remoteState}
          onRemoteChanged={() => void checkRemote()}
          mic={micChoice}
          setMic={setMicChoice}
          inkMode={mode === 'ink'}
          messy={messy}
          setMessy={setMessy}
          demo={demo}
          setDemo={setDemo}
          playing={playing}
          canPlay={ready}
          play={() => void play()}
        />
      )}
      {showModels && <ModelPanel status={builtin} onClose={() => setShowModels(false)} />}
    </div>
  )
}
