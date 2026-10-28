import { Excalidraw } from '@excalidraw/excalidraw'
import '@excalidraw/excalidraw/index.css'
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { useEffect, useMemo, useRef, useState } from 'react'
import cases from '../eval/cases.json'
import { layout } from './board/layout.ts'
import { type Look, scene } from './board/scene.ts'
import { type Graph, emptyGraph } from './engine/graph.ts'
import { formatOps } from './engine/ops.ts'
import type { Utterance } from './engine/prompt.ts'
import { Session } from './engine/session.ts'

interface Line extends Utterance {
  /** 模型处理这句话用了多久；undefined 表示还在排队 */
  ms?: number
  ops?: string
}

const LIGHT: Look = { ink: '#1e1e1e', pencil: '#868e96', highlight: '#ffec99', group: '#1971c2' }
const DARK: Look = { ink: '#e9ecef', pencil: '#868e96', highlight: '#5c4a00', group: '#74c0fc' }

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
  const [lines, setLines] = useState<Line[]>([])
  const [graph, setGraph] = useState<Graph>(emptyGraph)
  const [hot, setHot] = useState<Set<string>>(new Set())
  const [speaker, setSpeaker] = useState('A')
  const [text, setText] = useState('')
  const [demo, setDemo] = useState(cases[0].id)
  const [playing, setPlaying] = useState(false)
  const api = useRef<ExcalidrawImperativeAPI | null>(null)
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
      if (stale || !api.current) return
      const look = dark ? DARK : LIGHT
      api.current.updateScene({ elements: scene(graph, l, hot, look) })
      if (!graph.nodes.length) return
      // 自己算缩放和居中：内容少时不放大到 100% 以上，内容多时整张图塞进画面
      const boxes = [...l.nodes, ...l.groups]
      const x0 = Math.min(...boxes.map((b) => b.x))
      const y0 = Math.min(...boxes.map((b) => b.y))
      const x1 = Math.max(...boxes.map((b) => b.x + b.w))
      const y1 = Math.max(...boxes.map((b) => b.y + b.h))
      const { width, height, zoom } = api.current.getAppState()
      const z = Math.min(1, (0.85 * width) / (x1 - x0), (0.8 * height) / (y1 - y0))
      api.current.updateScene({
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
  }, [graph, hot, dark])

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
          <input className="who" value={speaker} onChange={(e) => setSpeaker(e.target.value)} aria-label="说话人" />
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="说一句…（回车）"
            aria-label="说的话"
          />
        </form>
        <button className="ghost" onClick={reset}>
          清空白板
        </button>
      </aside>

      <main className="board">
        <Excalidraw
          excalidrawAPI={(a) => (api.current = a)}
          theme={dark ? 'dark' : 'light'}
          initialData={{ appState: { viewBackgroundColor: dark ? '#121212' : '#fdfcf8', currentItemFontFamily: 1 } }}
          UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false } }}
        />
      </main>
    </div>
  )
}
