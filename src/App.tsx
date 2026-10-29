import { Excalidraw } from '@excalidraw/excalidraw'
import '@excalidraw/excalidraw/index.css'
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import cases from '../eval/cases.json'
import { layout } from './board/layout.ts'
import { type Look, scene } from './board/scene.ts'
import { type Graph, emptyGraph } from './engine/graph.ts'
import { proposeOps } from './engine/llm.ts'
import { formatOps } from './engine/ops.ts'
import type { Utterance } from './engine/prompt.ts'
import { Session } from './engine/session.ts'
import { type LlmStatus, isDesktop, llmStatus } from './desktop.ts'
import InkBoard from './ink/InkBoard.tsx'
import ModelPanel from './ModelPanel.tsx'

type Mode = 'pencil' | 'ink'

interface Line extends Utterance {
  /** 模型处理这句话用了多久；undefined 表示还在排队 */
  ms?: number
  ops?: string
}

const LIGHT: Look = { ink: '#1e1e1e', pencil: '#868e96', highlight: '#ffec99', group: '#1971c2', question: '#e03131', note: '#1971c2' }
const DARK: Look = { ink: '#e9ecef', pencil: '#868e96', highlight: '#5c4a00', group: '#74c0fc', question: '#ff8787', note: '#74c0fc' }

function load(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback
  } catch {
    return fallback
  }
}

function useDark(): boolean {
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
  const dark = useDark()
  const [baseUrl, setBaseUrl] = useState(() => load('boardcast.llm', 'http://127.0.0.1:8080'))
  const [online, setOnline] = useState<boolean | null>(null)
  // 桌面版：内置的 llama-server 由外壳拉起，就绪后自动用它的地址
  const [builtin, setBuiltin] = useState<LlmStatus | null>(null)
  // 第一次请求很慢（Vulkan 现场编译着色器，提示词前缀也还没进缓存），就绪后先空跑一次
  const [warming, setWarming] = useState(false)
  // 模型管理面板：第一次发现没有模型时自动打开
  const [showModels, setShowModels] = useState(false)
  const askedForModel = useRef(false)
  useEffect(() => {
    if (builtin?.state === 'no-model' && !askedForModel.current) {
      askedForModel.current = true
      setShowModels(true)
    }
  }, [builtin?.state])
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
  const listEnd = useRef<HTMLDivElement>(null)

  const session = useMemo(
    () =>
      new Session(
        { baseUrl, noThink: true },
        {
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
                  out[i] = { ...out[i], ms: r.ms, ops: left === r.batch.length ? formatOps(r.ops) || '-' : '' }
                  left--
                }
              }
              return out
            }),
          error: (e) => {
            console.error(e)
            setOnline(false)
          },
        },
      ),
    [baseUrl],
  )

  useEffect(() => {
    try {
      localStorage.setItem('boardcast.llm', baseUrl)
    } catch {
      /* 存不了就算了 */
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
      api.updateScene({ elements: scene(graph, l, hot, look) })
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
    try {
      localStorage.setItem('boardcast.mode', mode)
      localStorage.setItem('boardcast.messy', String(messy))
    } catch {
      /* 存不了就算了 */
    }
  }, [mode, messy])

  // 新版浏览器的 scrollIntoView 会返回 Promise，不能直接当 effect 的返回值
  useEffect(() => {
    listEnd.current?.scrollIntoView({ block: 'end' })
  }, [lines])

  function say(u: Utterance) {
    setLines((ls) => [...ls, u])
    session.say(u)
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

  return (
    <div className="app">
      <aside className="side">
        <header>
          <h1>boardcast</h1>
          <p className="sub">说话，板书自己长出来</p>
        </header>

        <label className="field">
          <span>
            本地模型 <i className={`dot ${online === null ? '' : online ? 'ok' : 'bad'}`} />
          </span>
          <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value.trim())} spellCheck={false} />
          {builtin && (
            <small className={`builtin ${builtin.state}`}>
              {builtin.state === 'ready' && `内置：${builtin.model} · ${builtin.device}${warming ? ' · 预热中…' : ''}`}
              {builtin.state === 'starting' && `正在启动内置模型 ${builtin.model ?? ''}（${builtin.device ?? ''}）…`}
              {builtin.state === 'no-model' && builtin.message}
              {builtin.state === 'error' && builtin.message}{' '}
              <button type="button" className="link" onClick={() => setShowModels(true)}>
                管理模型
              </button>
            </small>
          )}
        </label>

        <div className="row">
          <select value={demo} onChange={(e) => setDemo(e.target.value)} disabled={playing}>
            {cases.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          <button onClick={play} disabled={playing || !online}>
            {playing ? '播放中…' : '播放示例会议'}
          </button>
        </div>

        <div className="transcript">
          {lines.length === 0 && <p className="empty">在下面输入一句话，或者播放一段示例会议。语音输入还在做。</p>}
          {lines.map((l, i) => (
            <div key={i} className="line">
              <div>
                {l.speaker && <b>{l.speaker}：</b>}
                {l.text}
              </div>
              <div className="meta">
                {l.ms === undefined ? '画着…' : `${Math.round(l.ms)} ms`}
                {l.ops ? <pre>{l.ops}</pre> : null}
              </div>
            </div>
          ))}
          <div ref={listEnd} />
        </div>

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
        {mode === 'ink' && (
          <label className="messy">
            <span>
              潦草度 <b>{messy}</b>
              <i>{messy < 25 ? '工整' : messy < 45 ? '随手' : messy < 70 ? '连写' : '龙飞凤舞'}</i>
            </span>
            <input
              type="range"
              min={0}
              max={100}
              value={messy}
              onChange={(e) => setMessy(Number(e.target.value))}
              aria-label="潦草度"
            />
          </label>
        )}
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
      {showModels && <ModelPanel status={builtin} onClose={() => setShowModels(false)} />}
    </div>
  )
}
