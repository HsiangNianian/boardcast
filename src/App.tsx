import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { Suspense, lazy, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import cases from '../eval/cases.json'
import { board } from './board/layout.ts'
import type { Look } from './board/scene.ts'
import {
  type Heard,
  type LlmStatus,
  type VoiceStatus,
  isDesktop,
  llmEnsure,
  llmStatus,
  llmStop,
  localReady,
  on,
  voice,
} from './desktop.ts'
import { type Graph, emptyGraph } from './engine/graph.ts'
import { type LlmConfig, proposeOps } from './engine/llm.ts'
import { formatOps } from './engine/ops.ts'
import type { Utterance } from './engine/prompt.ts'
import { Session } from './engine/session.ts'
import InkBoard from './ink/InkBoard.tsx'
import ModelPanel from './ModelPanel.tsx'
import { type RemoteView, remote } from './remote.ts'
import SettingsPanel, { type RemoteState, errText } from './SettingsPanel.tsx'
import Composer from './ui/Composer.tsx'
import type { ExportKind } from './exporter.ts'
import { Auto, Close, Download, Eraser, Gear, Moon, Pen, Pencil, Sun, Tidy, Transcript as TranscriptIcon } from './ui/icons.tsx'
import Splash from './ui/Splash.tsx'
import Wordmark from './ui/Wordmark.tsx'

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

const THEMES: Theme[] = ['system', 'light', 'dark']
const THEME_LABEL: Record<Theme, string> = { system: '跟随系统', light: '浅色', dark: '深色' }
// Excalidraw 很大（连同 mermaid、katex 好几 MB），只在切到铅笔模式时才加载
const PencilBoard = lazy(() => import('./board/PencilBoard.tsx'))

// 铅笔模式的纸色，和界面的浅色底一致；深色模式下 Excalidraw 会自己反色
const PAPER_LIGHT = '#fbfbf9'

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
  // 正在用远程模型时不预热内置模型（它这时只是兜底）；轮询的闭包里拿不到最新的 state，用 ref
  const remoteFirst = useRef(false)
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
        if (warmed.current !== s.url && !remoteFirst.current) {
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
      remoteFirst.current = false
      setRemoteState({ state: 'off' })
      // 没有远程模型：内置模型要起来（启动时是待命的话）
      if (isDesktop) void llmEnsure().catch(() => undefined)
      return
    }
    setRemoteState({ state: 'checking' })
    try {
      const t = await remote.test()
      remoteFirst.current = true
      setRemoteState({ state: 'ok', ms: t.ms })
      // 远程能用：内置模型停下来，把显存让出来；某一句远程出错时再临时拉起
      if (isDesktop) {
        const b = await llmStatus().catch(() => null)
        if (b && (b.state === 'ready' || b.state === 'starting')) await llmStop().catch(() => undefined)
      }
    } catch (e) {
      remoteFirst.current = false
      setRemoteState({ state: 'fail', message: errText(e) })
      if (isDesktop) void llmEnsure().catch(() => undefined)
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
    // 桌面版的内置模型这时多半在待命：某一句远程失败了，先把它拉起来、等就绪，再用它画这一句
    const fallback: LlmConfig | undefined = isDesktop
      ? {
          ...local,
          baseUrl: 'builtin',
          send: async (body) => {
            const url = await localReady()
            const res = await fetch(`${url}/v1/chat/completions`, {
              method: 'POST',
              headers: { 'content-type': 'application/json' },
              body: JSON.stringify(body),
            })
            if (!res.ok) throw new Error(`内置模型 ${res.status}: ${await res.text()}`)
            return res.json()
          },
        }
      : baseUrl
        ? local
        : undefined
    return {
      baseUrl: remoteView.baseUrl,
      model: remoteView.model,
      flavor: 'openai',
      send: remote.send,
      label: `远程 ${remoteView.model}`,
      fallback,
    }
  }, [baseUrl, useRemote, remoteView])

  const [lines, setLines] = useState<Line[]>([])
  const [graph, setGraph] = useState<Graph>(emptyGraph)
  const [hot, setHot] = useState<Set<string>>(new Set())
  const [speaker, setSpeaker] = useState('A')
  // 会议记录栏：窄屏默认收起
  const [showLog, setShowLog] = useState(() => load('boardcast.log', innerWidth > 1100 ? 'open' : 'closed') === 'open')
  // 启动动画每次打开应用放一次
  const [splash, setSplash] = useState(true)
  // 「整理排版」：忘掉记住的位置，整块重新排一次
  const [tidy, setTidy] = useState(0)
  const tidyUp = () => {
    board.reset()
    setTidy((t) => t + 1)
  }
  // 导出菜单和保存后的提示
  const [exportMenu, setExportMenu] = useState(false)
  const [toast, setToast] = useState<{ ok: boolean; text: string } | null>(null)
  useEffect(() => {
    if (!toast) return
    const t = setTimeout(() => setToast(null), 4000)
    return () => clearTimeout(t)
  }, [toast])
  // 悬浮输入框挡住了白板底部多高：自动适配画面时让开（输入框展开、收起、换屏幕尺寸时都会变）
  const [inset, setInset] = useState(170)
  useEffect(() => {
    const el = document.querySelector('.dock')
    if (!el) return
    const board = el.parentElement!
    // 用布局尺寸算，不用 getBoundingClientRect：手机上转过来横屏时后者是屏幕方向的
    const ro = new ResizeObserver(() => setInset(board.clientHeight - (el as HTMLElement).offsetTop + 12))
    ro.observe(el)
    ro.observe(board)
    return () => ro.disconnect()
  }, [])
  const [demo, setDemo] = useState(cases[0].id)
  const [playing, setPlaying] = useState(false)
  // 用 state 而不是 ref：切回铅笔模式时 Excalidraw 重新挂载，要让重画的 effect 再跑一次
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null)
  const [mode, setMode] = useState<Mode>(() => (load('boardcast.mode', 'ink') === 'pencil' ? 'pencil' : 'ink'))
  const [messy, setMessy] = useState(() => {
    const v = Number(load('boardcast.messy', '30'))
    return Number.isFinite(v) ? Math.min(100, Math.max(0, v)) : 30
  })
  // 笔画粗细：百分比，100 是默认
  const [weight, setWeight] = useState(() => {
    const v = Number(load('boardcast.weight', '100'))
    return Number.isFinite(v) ? Math.min(200, Math.max(50, v)) : 100
  })
  // 拖滑杆时每一格都要重算全部笔画，让它慢半拍，拖动本身不卡
  const messyLater = useDeferredValue(messy)
  const weightLater = useDeferredValue(weight)
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

  // 图变了就重新排版、重画。排版有记忆：旧内容原地不动，新内容找空地（见 board/layout.ts）
  const pencilView = useRef({ had: false, tidy: 0 })
  useEffect(() => {
    let stale = false
    void board.layout(graph).then(async (l) => {
      if (stale || !api) return
      // 只有铅笔模式用得到；跟着 Excalidraw 一起按需加载
      const { scene } = await import('./board/scene.ts')
      if (stale) return
      // Excalidraw 的深色模式是给整块画布加反色滤镜，所以永远按浅色画，深色由它反过来
      const look = LIGHT
      api.updateScene({
        elements: scene(graph, l, hot, look),
        appState: { viewBackgroundColor: PAPER_LIGHT },
      })
      const seen = pencilView.current
      const first = !seen.had || seen.tidy !== tidy
      pencilView.current = { had: graph.nodes.length > 0, tidy }
      if (!graph.nodes.length) return
      const { x0, y0, x1, y1 } = l.bounds
      const { width, height, zoom, scrollX, scrollY } = api.getAppState()
      // 底下留出悬浮输入框的位置
      const free = Math.max(120, height - inset)
      // 新内容还在画面里就不动视角；跑出画面、第一笔、刚整理过才重新缩放居中
      const vx = -scrollX
      const vy = -scrollY
      const inside = x0 >= vx && y0 >= vy && x1 <= vx + width / zoom.value && y1 <= vy + free / zoom.value
      if (inside && !first) return
      // 内容少时不放大到 100% 以上，内容多时整张图塞进画面
      const z = Math.min(1, (0.85 * width) / (x1 - x0), (0.8 * free) / (y1 - y0))
      api.updateScene({
        appState: {
          zoom: { value: z as typeof zoom.value },
          scrollX: width / (2 * z) - (x0 + x1) / 2,
          scrollY: free / (2 * z) - (y0 + y1) / 2,
        },
      })
    })
    return () => {
      stale = true
    }
  }, [graph, hot, api, inset, tidy])

  useEffect(() => {
    save('boardcast.mode', mode)
    save('boardcast.messy', String(messy))
    save('boardcast.weight', String(weight))
    save('boardcast.mic', micChoice)
    save('boardcast.log', showLog ? 'open' : 'closed')
  }, [mode, messy, weight, micChoice, showLog])

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

  async function exportAs(kind: ExportKind) {
    setExportMenu(false)
    try {
      const ink = document.querySelector('.ink')
      const paper = mode === 'ink' && ink ? getComputedStyle(ink).backgroundColor : PAPER_LIGHT
      const { build, saveFile } = await import('./exporter.ts')
      const { name, blob } = await build(kind, { graph, lines, mode, api: mode === 'pencil' ? api : null, look: LIGHT, paper })
      const where = await saveFile(name, blob)
      if (where) setToast({ ok: true, text: isDesktop ? `已保存：${where}` : `已下载：${where}` })
    } catch (e) {
      setToast({ ok: false, text: `导出失败：${errText(e)}` })
    }
  }

  function reset() {
    board.reset()
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

  // 现在是哪个模型在画
  const ready = useRemote || !!online
  let status: string
  const localName = builtin?.model?.replace(/\.gguf$/, '')
  if (useRemote) {
    status = `远程 · ${remoteView?.model}`
  } else if (remoteState.state === 'checking') {
    status = '正在检查远程模型…'
  } else if (builtin?.state === 'ready') {
    status = `内置 · ${localName}${warming ? ' · 预热中' : ''}`
  } else if (builtin?.state === 'idle') {
    status = `内置 · ${localName ?? ''} · 待命`
  } else if (builtin?.state === 'starting') {
    status = `正在启动 ${localName ?? '内置模型'}…`
  } else if (builtin) {
    status = builtin.message ?? '内置模型不可用'
  } else {
    status = online ? `本地 · ${baseUrl.replace(/^https?:\/\//, '')}` : '没有连上模型'
  }
  const remoteBroken = remoteView?.enabled && remoteState.state === 'fail'

  const speakers = useMemo(() => {
    const seen = ['A', 'B', 'C']
    for (const l of lines) if (l.speaker && !seen.includes(l.speaker)) seen.push(l.speaker)
    if (speaker.trim() && !seen.includes(speaker.trim())) seen.push(speaker.trim())
    return seen
  }, [lines, speaker])

  const empty = lines.length === 0 && graph.nodes.length === 0 && !partial

  return (
    <div className={`app${showLog ? '' : ' no-log'}`}>
      {splash && <Splash onDone={() => setSplash(false)} />}

      <header className="topbar">
        <div className="brand">
          <Wordmark size={24} className="logo" />
        </div>
        <button
          type="button"
          className={`chip status${ready ? ' ok' : ''}${remoteBroken ? ' warn' : ''}`}
          onClick={() => setShowSettings(true)}
          title="出板书的模型，点开设置"
        >
          <i className="dot" />
          <span>{status}</span>
          {remoteBroken && <em>远程连不上，暂用本地</em>}
        </button>
        <span className="spacer" />
        <div className="seg" role="radiogroup" aria-label="板书风格">
          <button role="radio" aria-checked={mode === 'ink'} className={mode === 'ink' ? 'on' : ''} onClick={() => setMode('ink')}>
            <Pen size={15} />
            手写
          </button>
          <button role="radio" aria-checked={mode === 'pencil'} className={mode === 'pencil' ? 'on' : ''} onClick={() => setMode('pencil')}>
            <Pencil size={15} />
            铅笔 · 可编辑
          </button>
        </div>
        <div className="icons">
          <button
            type="button"
            className={`icon${showLog ? ' on' : ''}`}
            onClick={() => setShowLog(!showLog)}
            aria-pressed={showLog}
            aria-label="会议记录"
            title="会议记录"
          >
            <TranscriptIcon />
          </button>
          <button type="button" className="icon" onClick={tidyUp} disabled={!graph.nodes.length} aria-label="整理排版" title="整理排版：整块重新排一次">
            <Tidy />
          </button>
          <div className="menu-anchor">
            <button
              type="button"
              className={`icon${exportMenu ? ' on' : ''}`}
              onClick={() => setExportMenu(!exportMenu)}
              disabled={!graph.nodes.length}
              aria-haspopup="menu"
              aria-expanded={exportMenu}
              aria-label="导出"
              title="导出"
            >
              <Download />
            </button>
            {exportMenu && (
              <div className="menu" role="menu" onMouseLeave={() => setExportMenu(false)}>
                <button type="button" role="menuitem" onClick={() => void exportAs('md')}>
                  <span>Markdown</span>
                  <small>要点、Mermaid 流程图和会议记录</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('png')}>
                  <span>PNG 图片</span>
                  <small>{mode === 'ink' ? '手写板书，和画面上一样' : '铅笔板书，含手动改动'}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('excalidraw')}>
                  <span>Excalidraw</span>
                  <small>.excalidraw，可以在 excalidraw.com 接着改</small>
                </button>
              </div>
            )}
          </div>
          <button type="button" className="icon" onClick={reset} aria-label="清空白板" title="清空白板">
            <Eraser />
          </button>
          <button
            type="button"
            className="icon"
            onClick={() => setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length])}
            aria-label={`主题：${THEME_LABEL[theme]}，点一下切换`}
            title={`主题：${THEME_LABEL[theme]}`}
          >
            {theme === 'light' ? <Sun /> : theme === 'dark' ? <Moon /> : <Auto />}
          </button>
          <button type="button" className="icon" onClick={() => setShowSettings(true)} aria-label="设置" title="设置">
            <Gear />
          </button>
        </div>
      </header>

      <main className="board">
        {mode === 'ink' ? (
          <InkBoard graph={graph} hot={hot} dark={dark} messy={messyLater / 100} bottomInset={inset} tidy={tidy} weight={weightLater / 100} />
        ) : (
          <Suspense fallback={<div className="board-loading">正在打开铅笔白板…</div>}>
            <PencilBoard onApi={setApi} dark={dark} paper={PAPER_LIGHT} />
          </Suspense>
        )}

        {empty && !splash && (
          <div className={`hero${mode === 'ink' && dark ? ' on-board' : ''}`}>
            <span className="kicker">本地优先 · 实时会议板书</span>
            <h1>
              开会时只管说，
              <br />
              板书自己长出来。
            </h1>
            <p>
              说出来的话会被整理成话题、要点、流程和问题，一笔一笔写在白板上。
              {isDesktop ? '点下面的麦克风开始，或者先放一段示例会议看看。' : '在下面输入一句话，或者先放一段示例会议看看。'}
            </p>
            <div className="cta">
              {isDesktop && (
                <button type="button" className="primary" onClick={() => void toggleMic()}>
                  开始收音
                </button>
              )}
              <button type="button" className={isDesktop ? 'secondary' : 'primary'} onClick={() => void play()} disabled={!ready || playing}>
                {ready ? '播放示例会议' : '等模型就绪…'}
              </button>
            </div>
            <label className="demo-pick">
              <span>示例</span>
              <select value={demo} onChange={(e) => setDemo(e.target.value)} aria-label="示例会议">
                {cases.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}

        <Composer
          onSend={(t, who) => say({ speaker: who.trim() || undefined, text: t })}
          speaker={speaker}
          setSpeaker={setSpeaker}
          speakers={speakers}
          canListen={isDesktop}
          listening={listening}
          level={level}
          partial={partial}
          micName={mic}
          voiceErr={voiceErr}
          toggleMic={() => void toggleMic()}
        />
      </main>

      <aside className="log" aria-label="会议记录" hidden={!showLog}>
        <header>
          <h2>会议记录</h2>
          <span className="count">{lines.length ? `${lines.length} 句` : ''}</span>
          <button type="button" className="icon" onClick={() => setShowLog(false)} aria-label="收起会议记录">
            <Close size={16} />
          </button>
        </header>
        <div className="lines">
          {lines.length === 0 && !partial && <p className="empty">还没有人说话。说的每一句和它画出来的东西都会记在这里。</p>}
          {lines.map((l, i) => (
            <article key={i} className={`line${l.ms === undefined ? ' pending' : ''}`}>
              {l.speaker && <span className={`who c${speakers.indexOf(l.speaker) % 6}`}>{l.speaker}</span>}
              <div className="body">
                <p>{l.text}</p>
                <details>
                  <summary>
                    {l.ms === undefined ? '画着…' : `${Math.round(l.ms)} ms`}
                    {l.ms !== undefined && useRemote && l.via && ` · ${l.via}`}
                    {l.fellBack && <em> · 远程失败，这句用了本地模型</em>}
                  </summary>
                  {l.ops ? <pre>{l.ops}</pre> : <pre>（没有改动）</pre>}
                </details>
              </div>
            </article>
          ))}
          {partial && (
            <article className="line partial" aria-live="polite">
              {speaker.trim() && <span className={`who c${speakers.indexOf(speaker.trim()) % 6}`}>{speaker.trim()}</span>}
              <div className="body">
                <p>{partial}…</p>
              </div>
            </article>
          )}
          <div ref={listEnd} />
        </div>
      </aside>

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
          weight={weight}
          setWeight={setWeight}
        />
      )}
      {showModels && <ModelPanel status={builtin} onClose={() => setShowModels(false)} />}
      {toast && (
        <div className={`toast${toast.ok ? '' : ' err'}`} role="status">
          {toast.text}
        </div>
      )}
    </div>
  )
}
