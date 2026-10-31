import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types'
import { Fragment, Suspense, lazy, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import cases from '../eval/cases.json'
import { type LayoutMemory, board } from './board/layout.ts'
import type { Look } from './board/scene.ts'
import {
  type Heard,
  type HeardTiming,
  type LlmStatus,
  type VoiceStatus,
  isDesktop,
  shell,
  llmEnsure,
  type UpdateInfo,
  type VoiceSource,
  arxivPdf,
  updates,
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
import { History as EditHistory, type Selection, describe, exists, pinned, remove as removeSel, rename as renameSel, toggleMark } from './engine/edit.ts'
import { MARKS, type Mark } from './engine/graph.ts'
import { Session } from './engine/session.ts'
import { type Step, diffGraph, inkOnly, replayTo } from './engine/steps.ts'
import { summarize } from './engine/summary.ts'
import { correct, parseGlossary } from './engine/glossary.ts'
import { type Voice, identify, renameVoice } from './engine/voices.ts'
import { type Meeting, type MeetingLine, type Page, MEETING_VERSION, autoTitle, meetings, newMeetingId } from './meetings.ts'
import InkBoard from './ink/InkBoard.tsx'
import ModelPanel from './ModelPanel.tsx'
import { hostOf, isLoopback } from './privacy.ts'
import { type RemoteView, remote } from './remote.ts'
import SettingsPanel, { type RemoteState, errText } from './SettingsPanel.tsx'
import Composer from './ui/Composer.tsx'
import type { ExportKind } from './exporter.ts'
import { Auto, Close, Download, Gear, History, Moon, Pen, Pencil, Plus, Redo, Replay as ReplayIcon, Sun, Tidy, Upload, Transcript as TranscriptIcon, Undo } from './ui/icons.tsx'
import MeetingsPanel from './ui/MeetingsPanel.tsx'
import ReplayBar, { SPEEDS } from './ui/ReplayBar.tsx'
import PageStrip from './ui/PageStrip.tsx'
import type { PdfImport } from './board/importPdf.ts'
import { type Thumb, pageThumb, thumbOf } from './board/thumb.ts'
import { navTarget, parseNav } from './engine/nav.ts'
import type { Hit } from './engine/search.ts'
import { hasSpan, spanAt, toggleSpan } from './engine/spans.ts'
import DiagnosticsPanel from './ui/DiagnosticsPanel.tsx'
import { lineTiming, recentErrors, timingReport } from './engine/diagnostics.ts'
import { type Action, actionFor } from './engine/shortcuts.ts'
import { currentLang, msg, useT } from './i18n/index.ts'
import { type SpokenLang, spokenLang } from './engine/lang.ts'
import { type DemoInfo, demoIndex, demoMeeting, loadDemo } from './demo.ts'
import Splash from './ui/Splash.tsx'
import Wordmark from './ui/Wordmark.tsx'

type Mode = 'pencil' | 'ink'
type Theme = 'system' | 'light' | 'dark'

type Line = MeetingLine

const LIGHT: Look = { ink: '#1e1e1e', pencil: '#868e96', highlight: '#ffec99', group: '#1971c2', question: '#e03131', note: '#1971c2' }

const THEMES: Theme[] = ['system', 'light', 'dark']
// Excalidraw 很大（连同 mermaid、katex 好几 MB），只在切到铅笔模式时才加载
// 摘要要用 Markdown 渲染库，第一次显示摘要时再加载
const Summary = lazy(() => import('./ui/Summary.tsx'))
const UpdatePanel = lazy(() => import('./ui/UpdatePanel.tsx'))
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

/** Sample meetings in the given language; all of them if there are none in it */
function inLang<T extends { lang: SpokenLang }>(xs: T[], lang: SpokenLang): T[] {
  const mine = xs.filter((x) => x.lang === lang)
  return mine.length ? mine : xs
}

const LIVE_DEMOS = cases.map((c) => ({ id: c.id, name: c.name, lang: spokenLang(c.turns.map((u) => u.text)) }))
const RELEASES = 'https://github.com/HsiangNianian/boardcast/releases/latest'

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
  const t = useT()
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
  // 这场会只用本地模型（按会议记住）：开着远程也不发出去
  const [localOnly, setLocalOnly] = useState(false)
  // 术语表：全局的存在本机，这场会的跟着会议存
  const [glossary, setGlossary] = useState(() => load('boardcast.glossary', ''))
  const [meetingGlossary, setMeetingGlossary] = useState('')
  const meetingGlossaryRef = useRef('')
  meetingGlossaryRef.current = meetingGlossary
  const terms = useMemo(() => parseGlossary(`${glossary}\n${meetingGlossary}`), [glossary, meetingGlossary])
  const termsRef = useRef(terms)
  termsRef.current = terms
  const localOnlyRef = useRef(false)
  localOnlyRef.current = localOnly
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
      remoteFirst.current = !localOnlyRef.current
      setRemoteState({ state: 'ok', ms: t.ms })
      // 远程能用：内置模型停下来，把显存让出来；某一句远程出错时再临时拉起（这场会只用本地时不停）
      if (isDesktop && !localOnlyRef.current) {
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
  /** 这场会用不用远程接口：远程能用，而且这场会没有限定只用本地 */
  const remoteOn = useRemote && !localOnly
  /** 说的话会不会离开这台电脑：远程接口填的是本机地址（LM Studio 之类）不算 */
  const cloud = remoteOn && !!remoteView && !isLoopback(remoteView.baseUrl)
  // 这场会只用本地：内置模型这时可能在待命，拉起来
  useEffect(() => {
    if (!localOnly || !useRemote) return
    remoteFirst.current = false
    if (isDesktop) void llmEnsure().catch(() => undefined)
  }, [localOnly, useRemote])

  useEffect(() => {
    if (builtin?.state === 'no-model' && !askedForModel.current && remoteState.state !== 'checking' && !useRemote) {
      askedForModel.current = true
      setShowModels(true)
    }
  }, [builtin?.state, remoteState.state, useRemote])

  // 实际用来出板书的接口：远程优先，本地兜底
  const llm = useMemo<LlmConfig>(() => {
    const local: LlmConfig = { baseUrl, noThink: true, label: isDesktop ? t.app.builtinLabel : t.app.localLabel }
    if (!remoteOn || !remoteView) return local
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
            if (!res.ok) throw new Error(t.app.builtinHttp(res.status, await res.text()))
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
      label: t.app.remoteLabel(remoteView.model),
      fallback,
    }
  }, [baseUrl, remoteOn, remoteView, t])

  const [lines, setLines] = useState<Line[]>([])
  // 会议摘要：一键生成，可以手改，跟着会议一起存、一起导出
  const [summary, setSummary] = useState('')
  const summaryRef = useRef('')
  summaryRef.current = summary
  /** 正在生成摘要：给界面看的进度；null 表示没在生成 */
  const [summing, setSumming] = useState<string | null>(null)
  const summingFor = useRef<AbortController | null>(null)
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
  const [showKeys, setShowKeys] = useState(false)
  const [hotkey, setHotkey] = useState(() => load('boardcast.hotkey', 'Ctrl+Shift+Space'))
  const [hotkeyErr, setHotkeyErr] = useState<string | null>(null)
  const [toast, setToast] = useState<{ ok: boolean; text: string } | null>(null)
  useEffect(() => {
    if (!toast) return
    // 报错多留一会儿，来得及复制
    const t = setTimeout(() => setToast(null), toast.ok ? 4000 : 10000)
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
  // Sample meetings: with a model they are played live; without one (the web demo) the ones
  // recorded by eval/record.ts are replayed. The list itself is worked out once `ready` is known.
  const lang = currentLang()
  const [recordedDemos, setRecordedDemos] = useState<DemoInfo[]>([])
  useEffect(() => {
    if (!isDesktop) void demoIndex().then(setRecordedDemos)
  }, [])
  const [demoPick, setDemo] = useState('')
  /** Id of the meeting opened from a recorded demo: it is not saved */
  const demoMeetingId = useRef<string | null>(null)
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
  // 收哪些声音：线上会议要加上系统声音（别人的声音从扬声器出来）
  const [voiceSource, setVoiceSource] = useState<VoiceSource>(() => {
    const v = load('boardcast.source', 'mic')
    return v === 'both' || v === 'system' ? v : 'mic'
  })
  // 自动更新：打开应用一会儿以后查一次，有新版本就在顶栏冒一个按钮
  const [autoUpdate, setAutoUpdate] = useState(() => load('boardcast.autoupdate', 'on') === 'on')
  const [update, setUpdate] = useState<UpdateInfo | null>(null)
  const [showUpdate, setShowUpdate] = useState(false)
  useEffect(() => {
    if (!isDesktop || !autoUpdate) return
    // 等启动的这阵忙完（内置模型、语音模型加载）再查；查不到（离线之类）不打扰
    const t = setTimeout(() => void updates.check().then(setUpdate, () => undefined), 8000)
    return () => clearTimeout(t)
  }, [autoUpdate])
  // 按声音区分说话人：声纹表跟着会议存
  const [diarize, setDiarize] = useState(() => load('boardcast.diarize', 'on') === 'on')
  const diarizeRef = useRef(diarize)
  diarizeRef.current = diarize
  const voicesRef = useRef<Voice[]>([])
  /** 正在给哪个说话人改名 */
  const [renamingWho, setRenamingWho] = useState<string | null>(null)
  const listEnd = useRef<HTMLDivElement>(null)

  // —— 手动修改：撤销历史、选中 ——
  /** 撤销的一步：板书 + 排版记住的位置（拖动只改位置；撤销删除时东西也回到原处） */
  type Snap = { graph: Graph; layout: LayoutMemory }
  const editHistory = useRef(new EditHistory<Snap>())
  /** 排版记忆变了（拖动、撤销）就加一，让白板重新排版 */
  const [layoutRev, setLayoutRev] = useState(0)
  /** 最近一张墨线板（不含铅笔稿）：撤销时退回到这里 */
  const lastInk = useRef<Graph>(emptyGraph())
  const [, setUndoTick] = useState(0)
  const [selection, setSelection] = useState<Selection | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)

  // —— 回放：墨线板书每变一次记一步（和上一版的差异），回放时从空板依次打上 ——
  const stepsRef = useRef<Step[]>([])
  /** 最近记下的那一版墨线板书 */
  const recorded = useRef<Graph>(emptyGraph())
  /** 正在回放：放到第几步、是不是在播、倍速，和回放前的板书（退出时放回去） */
  type Replay = { at: number; playing: boolean; speed: number; total: number; saved: Snap }
  const [replay, setReplay] = useState<Replay | null>(null)
  const replayRef = useRef<Replay | null>(null)
  replayRef.current = replay

  // —— 几张板：当前这张的内容在 session / board / stepsRef 里，别的张存在 pagesRef 里 ——
  const pagesRef = useRef<Page[]>([{ graph: emptyGraph() }])
  const [page, setPage] = useState(0)
  const pageRef = useRef(0)
  pageRef.current = page
  const [thumbs, setThumbs] = useState<(Thumb | undefined)[]>([undefined])
  const thumbRef = useRef<Thumb | undefined>(undefined)
  /** 已经建议过新开一张的板（每张只提一次） */
  const suggested = useRef(new Set<number>())
  const [suggest, setSuggest] = useState(false)
  /** 当前这张板的完整内容 */
  const capture = (): Page => ({
    graph: session.graph,
    layout: board.snapshot(),
    steps: stepsRef.current,
    thumb: thumbRef.current,
    underlay: pagesRef.current[pageRef.current]?.underlay,
  })

  // Session 只建一次。以前跟着模型配置重建：远程出错临时拉起本地模型、地址一变，会开到一半板书就被换成空的
  const sessionRef = useRef<Session | null>(null)
  sessionRef.current ??= new Session(llm, {
        graph: (g, why) => {
          if (why === 'ink') {
            editHistory.current.record({ graph: lastInk.current, layout: board.snapshot() })
            lastInk.current = g
            setUndoTick((t) => t + 1)
          }
          if (!replayRef.current) setGraph(g)
        },
        mention: (ids) => {
          setHot(new Set(ids))
          if (ids.length) setTimeout(() => setHot(new Set()), 3000)
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
                  queueMs: r.queuedMs[left - 1],
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
      })
  const session = sessionRef.current
  useEffect(() => {
    session.llm = llm
  }, [llm, session])
  useEffect(() => {
    session.terms = terms
  }, [terms, session])

  // —— 会议：自动保存，下次打开接着开 ——
  type MeetingMeta = Pick<Meeting, 'id' | 'title' | 'titled' | 'createdAt'>
  const [meeting, setMeeting] = useState<MeetingMeta>(() => ({ id: newMeetingId(), title: msg().app.untitled, createdAt: Date.now() }))
  const [restored, setRestored] = useState(false)
  const [showMeetings, setShowMeetings] = useState(false)
  const linesRef = useRef<Line[]>([])
  linesRef.current = lines

  /** 把一场存过的会议放回界面上：板书、上下文、会议记录、排版位置 */
  const openMeeting = useCallback(
    (m: Meeting) => {
      board.restore(m.layout)
      editHistory.current.clear()
      lastInk.current = m.graph
      setSelection(null)
      session.graph = m.graph
      session.history = m.history
      setGraph(m.graph)
      setLines(m.lines)
      setReplay(null)
      // 旧版本存的会议没有逐步记录：整块板算一步
      const seed = m.steps?.length ? null : diffGraph(emptyGraph(), inkOnly(m.graph))
      stepsRef.current = m.steps?.length ? m.steps : seed ? [{ t: m.updatedAt, line: m.lines.length, d: seed }] : []
      recorded.current = replayTo(stepsRef.current, stepsRef.current.length)
      pagesRef.current = m.pages?.length ? m.pages : [{ graph: m.graph }]
      const cur = Math.min(m.page ?? 0, pagesRef.current.length - 1)
      pageRef.current = cur
      setPage(cur)
      thumbRef.current = pagesRef.current[cur].thumb
      setThumbs(pagesRef.current.map((p) => p.thumb))
      suggested.current = new Set()
      setSuggest(false)
      summingFor.current?.abort()
      setSummary(m.summary ?? '')
      voicesRef.current = m.voices ?? []
      setRenamingWho(null)
      setLocalOnly(!!m.localOnly)
      setMeetingGlossary(m.glossary ?? '')
      setMeeting({ id: m.id, title: m.title, titled: m.titled, createdAt: m.createdAt })
      save('boardcast.meeting', m.id)
    },
    [session],
  )

  // 启动时打开上次那场会
  useEffect(() => {
    const last = load('boardcast.meeting', '')
    void (async () => {
      const m = last ? await meetings.load(last).catch(() => null) : null
      if (m) openMeeting(m)
      setRestored(true)
    })()
  }, [openMeeting])

  /** 当前这场会的完整内容（板书以 Session 里的为准） */
  const snapshot = useCallback(
    (meta: MeetingMeta): Meeting => ({
      version: MEETING_VERSION,
      ...meta,
      // 按第一张板起名：翻到后面的板时会议名不跟着变
      title: meta.titled ? meta.title : autoTitle(pageRef.current === 0 ? session.graph : pagesRef.current[0].graph, linesRef.current),
      updatedAt: Date.now(),
      graph: session.graph,
      history: session.history,
      lines: linesRef.current,
      layout: board.snapshot(),
      summary: summaryRef.current || undefined,
      voices: voicesRef.current.length ? voicesRef.current : undefined,
      steps: stepsRef.current.length ? stepsRef.current : undefined,
      localOnly: localOnlyRef.current || undefined,
      glossary: meetingGlossaryRef.current.trim() || undefined,
      // 只有一张板、但它垫着导入页时也要存 pages：底图存在这里
      ...(pagesRef.current.length > 1 || pagesRef.current.some((p) => p.underlay)
        ? { pages: pagesRef.current.map((p, i) => (i === pageRef.current ? capture() : p)), page: pageRef.current }
        : {}),
    }),
    [session],
  )

  // 板书或会议记录一变，停半秒存一次；空会议不存
  useEffect(() => {
    // 回放时画面上是过去某一刻的板，不存；放的录好的示例会议也不存
    if (!restored || replayRef.current || meeting.id === demoMeetingId.current) return
    if (!lines.length && !graph.nodes.length) return
    const t = setTimeout(() => {
      const m = snapshot(meeting)
      void meetings
        .save(m)
        .then(() => {
          save('boardcast.meeting', m.id)
          if (m.title !== meeting.title) setMeeting((x) => (x.id === m.id ? { ...x, title: m.title } : x))
        })
        .catch((e) => console.error('saving the meeting failed', e))
    }, 500)
    return () => clearTimeout(t)
  }, [graph, lines, summary, localOnly, meetingGlossary, page, meeting, restored, snapshot])

  // 墨线板书变了就记一步（铅笔稿不记）；会议记录说到第几句也记下，回放时字幕跟着走
  useEffect(() => {
    if (!restored || replayRef.current) return
    const ink = inkOnly(session.graph)
    const d = diffGraph(recorded.current, ink)
    if (!d) return
    stepsRef.current = [...stepsRef.current, { t: Date.now(), line: linesRef.current.length, d }]
    recorded.current = ink
  }, [graph, restored, session])

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
      if (stale) return
      if (!replayRef.current) {
        // 导入页：画原文块的位置，一眼认得出是哪一页
        const t = graph.underlay ? pageThumb(graph) : thumbOf(l)
        thumbRef.current = t
        setThumbs((ts) => ts.map((x, i) => (i === pageRef.current ? t : x)))
      }
      if (!api) return
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
  }, [graph, hot, api, inset, tidy, layoutRev])

  useEffect(() => {
    save('boardcast.mode', mode)
    save('boardcast.messy', String(messy))
    save('boardcast.weight', String(weight))
    save('boardcast.mic', micChoice)
    save('boardcast.source', voiceSource)
    save('boardcast.diarize', diarize ? 'on' : 'off')
    save('boardcast.glossary', glossary)
    save('boardcast.autoupdate', autoUpdate ? 'on' : 'off')
    save('boardcast.log', showLog ? 'open' : 'closed')
  }, [mode, messy, weight, micChoice, voiceSource, diarize, autoUpdate, glossary, showLog])

  // 新版浏览器的 scrollIntoView 会返回 Promise，不能直接当 effect 的返回值
  // 从搜索结果跳过来的那一句：滚到它，亮一下
  const [jumpLine, setJumpLine] = useState<number | null>(null)
  useEffect(() => {
    if (jumpLine === null) listEnd.current?.scrollIntoView({ block: 'end' })
  }, [lines, replay?.at, jumpLine])
  useEffect(() => {
    if (jumpLine === null) return
    const t = setTimeout(() => document.querySelector('.line.jump')?.scrollIntoView({ block: 'center' }), 60)
    const off = setTimeout(() => setJumpLine(null), 2500)
    return () => {
      clearTimeout(t)
      clearTimeout(off)
    }
  }, [jumpLine])

  function say(said: Utterance, heard?: HeardTiming) {
    // Saying something in a recorded demo makes it your own meeting, saved from now on
    demoMeetingId.current = null
    // 先按术语表纠正（说的和打的都纠）：内部叫法常被听成、打成别的字
    const u = { ...said, text: correct(said.text, termsRef.current) }
    // 整句就是翻页：不用问模型，也不写板书
    const nav = parseNav(u.text)
    if (nav) {
      const boards = pagesRef.current.map((p, i) => (i === pageRef.current ? session.graph : p.graph).underlay ?? {})
      const to = navTarget(nav, boards, pageRef.current)
      setLines((ls) => [...ls, { ...u, page: pageRef.current, ms: 0 }])
      if (to < 0) setToast({ ok: false, text: t.app.noSuchPage })
      else if (waiting) setToast({ ok: false, text: t.app.busyPage })
      else goPage(to)
      return
    }
    setLines((ls) => [...ls, { ...u, page: pageRef.current, heard }])
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
        // 有声纹就认人；没有（句子太短、没带声纹模型、关掉了）就用当前的说话人，也就是上一个说话的人
        let who = speakerRef.current.trim() || undefined
        if (diarizeRef.current && h.voiceprint?.length) {
          const taken = linesRef.current.flatMap((l) => (l.speaker ? [l.speaker] : []))
          const r = identify(voicesRef.current, h.voiceprint, taken)
          voicesRef.current = r.voices
          who = r.name
          speakerRef.current = r.name
          setSpeaker(r.name)
        }
        sayRef.current({ speaker: who, text: h.text }, h.timing)
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
      setVoiceErr(t.app.needVoiceModel)
      setShowModels(true)
      return
    }
    await voice.start(micChoice || undefined, voiceSource).catch((e) => setVoiceErr(errText(e)))
  }

  /** 生成摘要：会议记录（不含手动改动）加板书结构，交给当前在用的模型 */
  async function makeSummary() {
    if (summing) {
      summingFor.current?.abort()
      return
    }
    const ctl = new AbortController()
    summingFor.current = ctl
    setSumming(t.app.summing)
    try {
      // 所有板都带上：当前这张以 Session 里的为准
      const boards = pagesRef.current.map((p, i) => (i === pageRef.current ? session.graph : p.graph))
      const r = await summarize(session.llm, boards, lines.filter((l) => !l.manual), ctl.signal, (done, total) =>
        setSumming(done < total - 1 ? t.app.summingPart(done + 1, total - 1) : t.app.summingMerge),
      )
      if (ctl.signal.aborted) return
      setSummary(r.text)
      if (r.fellBack) setToast({ ok: false, text: t.app.summaryFellBack })
    } catch (e) {
      if (!ctl.signal.aborted) setToast({ ok: false, text: t.app.summaryFailed(errText(e)) })
    } finally {
      if (summingFor.current === ctl) {
        summingFor.current = null
        setSumming(null)
      }
    }
  }

  async function exportAs(kind: ExportKind) {
    setExportMenu(false)
    try {
      const ink = document.querySelector('.ink')
      const paper = mode === 'ink' && ink ? getComputedStyle(ink).backgroundColor : PAPER_LIGHT
      const { build, saveFile } = await import('./exporter.ts')
      const { name, blob } = await build(kind, {
        graph,
        lines,
        title: meeting.title,
        summary,
        pages: pagesRef.current.length > 1 ? pagesRef.current.map((p, i) => (i === page ? session.graph : p.graph)) : undefined,
        replay:
          kind === 'html' || kind === 'pdf'
            ? {
                title: meeting.title,
                createdAt: meeting.createdAt,
                summary: summary || undefined,
                lines,
                pages: pagesRef.current.map((p, i) => {
                  const cur = i === page ? capture() : p
                  return { graph: cur.graph, steps: cur.steps, underlay: cur.underlay, layout: cur.layout }
                }),
              }
            : undefined,
        ...(kind === 'anim'
          ? { steps: stepsRef.current, underlay: pagesRef.current[page]?.underlay, messy: messyLater / 100, weight: weightLater / 100 }
          : {}),
        mode, api: mode === 'pencil' ? api : null, look: LIGHT, paper })
      const where = await saveFile(name, blob)
      if (where) setToast({ ok: true, text: isDesktop ? t.app.saved(where) : t.app.downloaded(where) })
    } catch (e) {
      setToast({ ok: false, text: t.app.exportFailed(errText(e)) })
    }
  }

  /** 新会议：当前这场已经自动存下了，换一个新编号从空板开始；`keepCurrent` 为 false 时当前这场刚被删掉，不再存 */
  function reset(keepCurrent = true) {
    if (keepCurrent && (lines.length || graph.nodes.length) && meeting.id !== demoMeetingId.current)
      void meetings.save(snapshot(meeting)).catch(() => undefined)
    board.reset()
    session.graph = emptyGraph()
    session.history = []
    editHistory.current.clear()
    lastInk.current = session.graph
    setSelection(null)
    setGraph(session.graph)
    setLines([])
    summingFor.current?.abort()
    setSummary('')
    voicesRef.current = []
    setRenamingWho(null)
    stepsRef.current = []
    recorded.current = emptyGraph()
    setReplay(null)
    setLocalOnly(false)
    setMeetingGlossary('')
    pagesRef.current = [{ graph: emptyGraph() }]
    pageRef.current = 0
    setPage(0)
    thumbRef.current = undefined
    setThumbs([undefined])
    suggested.current = new Set()
    setSuggest(false)
    const id = newMeetingId()
    setMeeting({ id, title: t.app.untitled, createdAt: Date.now() })
    save('boardcast.meeting', id)
  }

  /** 手动改了板书：记一步撤销，记进会议记录（模型下一句看到的就是改过的板） */
  function applyEdit(next: Graph, note: string) {
    if (next === session.graph) return
    editHistory.current.record({ graph: session.graph, layout: board.snapshot() })
    session.graph = next
    lastInk.current = next
    setGraph(next)
    setUndoTick((t) => t + 1)
    setLines((ls) => [...ls, { text: note, manual: true, ms: 0, page: pageRef.current }])
    setSelection((sel) => (sel && exists(next, sel) ? sel : null))
  }

  function stepTo(snap: Snap | null, note: string) {
    if (!snap) return
    board.restore(snap.layout)
    const g = snap.graph
    session.graph = g
    lastInk.current = g
    setGraph(g)
    setLayoutRev((n) => n + 1)
    setUndoTick((t) => t + 1)
    setSelection(null)
    // 也记进会议记录：不然记录里还写着「删掉了……」，看着像没恢复
    setLines((ls) => [...ls, { text: note, manual: true, ms: 0, page: pageRef.current }])
  }
  const now = (): Snap => ({ graph: session.graph, layout: board.snapshot() })
  const undo = () => stepTo(editHistory.current.undo(now()), t.app.undoNote)
  const redo = () => stepTo(editHistory.current.redo(now()), t.app.redoNote)

  /** 拖动：把新位置写进排版记忆，记一步撤销 */
  function moveSelection(sel: Selection, dx: number, dy: number) {
    if (sel.kind !== 'node') return
    const before = now()
    if (!board.move(sel.id, dx, dy)) return
    editHistory.current.record(before)
    setUndoTick((t) => t + 1)
    setLayoutRev((n) => n + 1)
    setLines((ls) => [...ls, { text: t.app.movedNote(describe(session.graph, sel, t.doc)), manual: true, ms: 0, page: pageRef.current }])
  }

  function deleteSelection() {
    if (!selection) return
    applyEdit(removeSel(session.graph, selection), t.app.deletedNote(describe(session.graph, selection, t.doc)))
    setSelection(null)
  }

  /** 选中的原文：手动圈画按这个范围，点到的那个词或那句话 */
  const [markUnit, setMarkUnit] = useState<'word' | 'sentence' | 'block'>('block')
  const selectedAt = useRef<{ x: number; y: number } | undefined>(undefined)
  const unitSpan = (mark: Mark) => {
    if (selection?.kind !== 'node' || markUnit === 'block' || !selectedAt.current) return null
    const n = session.graph.nodes.find((x) => x.id === selection.id)
    return n?.rows ? spanAt(n.rows, selectedAt.current.x, selectedAt.current.y, mark, markUnit) : null
  }

  function markSelection(mark: Mark) {
    if (selection?.kind !== 'node') return
    const n = session.graph.nodes.find((x) => x.id === selection.id)
    const span = unitSpan(mark)
    if (span) {
      applyEdit(toggleSpan(session.graph, selection.id, span), t.app.markSpanNote(hasSpan(n, span), span.text.slice(0, 30), t.app.marks[mark]))
      return
    }
    const on = n?.marks.includes(mark)
    applyEdit(toggleMark(session.graph, selection.id, mark), t.app.markNote(!!on, describe(session.graph, selection, t.doc), t.app.marks[mark]))
  }

  /** 说话人改名：会议记录、模型看的前文、声纹表一起改；改成已有的名字就是把两个人合成一个 */
  function renameSpeaker(from: string, to: string) {
    to = to.trim()
    setRenamingWho(null)
    if (!to || to === from) return
    const fix = <T extends { speaker?: string }>(u: T): T => (u.speaker === from ? { ...u, speaker: to } : u)
    setLines((ls) => ls.map(fix))
    session.history = session.history.map(fix)
    voicesRef.current = renameVoice(voicesRef.current, from, to)
    if (speakerRef.current.trim() === from) setSpeaker(to)
  }

  function commitRename() {
    if (!selection || renaming === null) return
    const before = describe(session.graph, selection, t.doc)
    const next = renameSel(session.graph, selection, renaming)
    setRenaming(null)
    if (next !== session.graph) applyEdit(next, t.app.renamedNote(before, renaming.trim()))
  }

  // 键盘：撤销 / 重做 / 删除 / 取消选中。焦点在输入框里时不管；铅笔模式交给 Excalidraw 自己处理
  const run = (a: Action) => {
    const replaying = !!replayRef.current
    if (a === 'help') setShowKeys((v) => !v)
    else if (a === 'replay') (replaying ? exitReplay() : startReplay())
    else if (replaying) return
    else if (a === 'import') fileInput.current?.click()
    else if (a === 'summary') void makeSummary()
    else if (waiting) setToast({ ok: false, text: t.app.busyPage })
    else if (a === 'newPage') newPage()
    else goPage(pageRef.current + (a === 'nextPage' ? 1 : -1))
  }
  const keys = useRef({ undo, redo, deleteSelection, toggleReplay, seekReplay, exitReplay, run, toggleMic, listening })
  keys.current = { undo, redo, deleteSelection, toggleReplay, seekReplay, exitReplay, run, toggleMic, listening }
  // 按住空格说话：只有这次是按住才开的，松开才停
  const held = useRef(false)
  useEffect(() => {
    const blocked = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement
      // 键盘移到按钮上时，空格还是点按钮；鼠标点过的按钮留着焦点，不算
      return !!t.closest('input:not([type=range]), textarea, select, [contenteditable]') || !!t.closest('.modal') || (t.tagName === 'BUTTON' && t.matches(':focus-visible'))
    }
    const down = (e: KeyboardEvent) => {
      if (e.code !== 'Space' || !isDesktop || mode !== 'ink' || replayRef.current || blocked(e)) return
      e.preventDefault()
      if (e.repeat || held.current || keys.current.listening) return
      held.current = true
      void keys.current.toggleMic()
    }
    const up = (e: KeyboardEvent) => {
      if (e.code !== 'Space' || !held.current) return
      e.preventDefault()
      held.current = false
      // 多收一小会儿，免得切掉最后一个字；停下时引擎会把没说完的这句收尾
      setTimeout(() => void voice.stop(), 300)
    }
    addEventListener('keydown', down)
    addEventListener('keyup', up)
    return () => {
      removeEventListener('keydown', down)
      removeEventListener('keyup', up)
    }
  }, [mode])
  // 全局快捷键、托盘、boardcast --toggle-mic
  useEffect(() => {
    if (!isDesktop) return
    const off = shell.onToggle(() => void keys.current.toggleMic())
    return () => void off.then((f) => f())
  }, [])
  useEffect(() => {
    if (!isDesktop) return
    save('boardcast.hotkey', hotkey)
    shell
      .setShortcut(hotkey || null)
      .then(() => setHotkeyErr(null))
      .catch((e) => setHotkeyErr(errText(e)))
  }, [hotkey])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // 焦点在文字输入框里时不管（回放的进度条不算）
      if ((e.target as HTMLElement).closest('input:not([type=range]), textarea, select, [contenteditable]')) return
      // 回放：空格播放 / 暂停，左右键一步一步走，Esc 退出；别的快捷键都不管
      const r = replayRef.current
      if (r) {
        if (e.key === ' ') keys.current.toggleReplay()
        else if (e.key === 'ArrowLeft') keys.current.seekReplay(r.at - 1)
        else if (e.key === 'ArrowRight') keys.current.seekReplay(r.at + 1)
        else if (e.key === 'Escape') keys.current.exitReplay()
        else return
        e.preventDefault()
        return
      }
      if (mode !== 'ink') return
      if (document.querySelector('.modal')) return
      const action = actionFor(e)
      if (action) {
        e.preventDefault()
        keys.current.run(action)
        return
      }
      const mod = e.ctrlKey || e.metaKey
      if (mod && e.key.toLowerCase() === 'z' && !e.shiftKey) {
        e.preventDefault()
        keys.current.undo()
      } else if (mod && (e.key.toLowerCase() === 'y' || (e.key.toLowerCase() === 'z' && e.shiftKey))) {
        e.preventDefault()
        keys.current.redo()
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        keys.current.deleteSelection()
      } else if (e.key === 'Escape') {
        setSelection(null)
        setRenaming(null)
      }
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  }, [mode])

  // 选中的东西被模型改没了（或者撤销掉了），取消选中
  useEffect(() => {
    if (selection && !exists(graph, selection)) setSelection(null)
  }, [graph, selection])

  /** 开始回放：记下现在的板书和排版，从空板开始 */
  function startReplay() {
    const total = stepsRef.current.length
    if (!total || replayRef.current) return
    const saved: Snap = { graph: session.graph, layout: board.snapshot() }
    setSelection(null)
    setRenaming(null)
    setExportMenu(false)
    board.reset()
    setGraph(emptyGraph())
    setReplay({ at: 0, playing: true, speed: 1, total, saved })
  }

  /** 跳到第 `at` 步。往回跳时排版从头来，往前走时新内容照常找空地 */
  function seekReplay(at: number) {
    const r = replayRef.current
    if (!r) return
    at = Math.max(0, Math.min(r.total, Math.round(at)))
    if (at === r.at) return
    if (at < r.at) board.reset()
    setGraph(replayTo(stepsRef.current, at))
    const next = { ...r, at, playing: r.playing && at < r.total }
    replayRef.current = next
    setReplay(next)
  }

  function toggleReplay() {
    const r = replayRef.current
    if (!r) return
    if (r.playing) setReplay({ ...r, playing: false })
    else if (r.at >= r.total) {
      // 放完了再点播放：从头开始
      board.reset()
      setGraph(emptyGraph())
      setReplay({ ...r, at: 0, playing: true })
    } else setReplay({ ...r, playing: true })
  }

  /** 退出回放：放回原来的板书和排版 */
  function exitReplay() {
    const r = replayRef.current
    if (!r) return
    board.restore(r.saved.layout)
    replayRef.current = null
    setReplay(null)
    setGraph(session.graph)
    setLayoutRev((n) => n + 1)
  }

  // 播放：按当时的节奏走（停顿太长的压到 3 秒，太密的至少留 0.9 秒让字写出来），再除以倍速
  useEffect(() => {
    if (!replay?.playing) return
    const s = stepsRef.current
    const gap = replay.at > 0 && replay.at < s.length ? s[replay.at].t - s[replay.at - 1].t : 0
    const wait = replay.at === 0 ? 300 : Math.min(Math.max(gap, 900), 3000) / replay.speed
    const t = setTimeout(() => seekReplay(replay.at + 1), wait)
    return () => clearTimeout(t)
    // seekReplay 每次渲染都是新的，读的是 ref，不用放进依赖
  }, [replay])

  /** 选中的名字加进这场会的术语表 */
  function addTerm() {
    if (selection?.kind !== 'node') return
    const label = session.graph.nodes.find((n) => n.id === selection.id)?.label
    if (!label) return
    if (terms.some((t) => t.term === label)) {
      setToast({ ok: true, text: t.app.termExists(label) })
      return
    }
    setMeetingGlossary((g) => (g.trim() ? `${g.trimEnd()}\n${label}` : label))
    setToast({ ok: true, text: t.app.termAdded(label) })
  }

  /** 把一张板放到画面上：板书、排版、回放记录都换成它的 */
  function showPage(p: Page) {
    session.graph = p.graph
    board.restore(p.layout)
    editHistory.current.clear()
    lastInk.current = p.graph
    stepsRef.current = p.steps ?? []
    recorded.current = replayTo(stepsRef.current, stepsRef.current.length)
    thumbRef.current = p.thumb
    setSelection(null)
    setRenaming(null)
    setGraph(p.graph)
    setLayoutRev((n) => n + 1)
    // 换了一块板，视角重新适配
    setTidy((t) => t + 1)
  }

  /** 切到第 `i` 张板（当前这张先存起来） */
  function goPage(i: number) {
    if (i === pageRef.current || i < 0 || i >= pagesRef.current.length) return
    exitReplay()
    const all = [...pagesRef.current]
    all[pageRef.current] = capture()
    pagesRef.current = all
    pageRef.current = i
    showPage(all[i])
    setPage(i)
    setThumbs(all.map((p) => p.thumb))
    setSuggest(false)
  }

  /** 新开一张板，接在最后；这张留着可以切回来 */
  function newPage() {
    exitReplay()
    const all = [...pagesRef.current]
    all[pageRef.current] = capture()
    all.push({ graph: emptyGraph() })
    pagesRef.current = all
    pageRef.current = all.length - 1
    showPage(all[all.length - 1])
    setPage(all.length - 1)
    setThumbs(all.map((p) => p.thumb))
    setSuggest(false)
  }

  /** 导入的几页接在后面（当前这张是空板就从它开始），切到第一张，会议记录里记一笔 */
  function addPages(add: Page[], note: string) {
    exitReplay()
    const all = [...pagesRef.current]
    all[pageRef.current] = capture()
    const cur = all[pageRef.current]
    const blank = !cur.underlay && !cur.graph.nodes.some((n) => n.ink === 'ink')
    const at = blank ? pageRef.current : all.length
    // 没打开过的板也要有缩略图，翻页时认得出来
    all.splice(at, blank ? 1 : 0, ...add.map((p) => (p.thumb ? p : { ...p, thumb: pageThumb(p.graph) })))
    pagesRef.current = all
    pageRef.current = at
    showPage(all[at])
    setPage(at)
    setThumbs(all.map((p) => p.thumb))
    setSuggest(false)
    setLines((ls) => [...ls, { text: note, manual: true, ms: 0, page: at }])
  }

  // 一张板上的东西太多（自动适配会把字缩得很小），建议新开一张；每张只提一次
  useEffect(() => {
    if (replayRef.current || suggested.current.has(page)) return
    // 只数板书本身；导入页上的原文不算
    if (session.graph.nodes.filter((n) => n.ink === 'ink' && n.kind !== 'region').length < 30) return
    suggested.current.add(page)
    setSuggest(true)
  }, [graph, page, session])

  // —— 导入：图片、PDF、.excalidraw、arXiv 链接 ——
  const fileInput = useRef<HTMLInputElement>(null)
  const [dropping, setDropping] = useState(false)
  const [importMenu, setImportMenu] = useState(false)
  const [arxivInput, setArxivInput] = useState('')
  const [showDiag, setShowDiag] = useState(false)
  const [pendingPdf, setPendingPdf] = useState<{ name: string; r: PdfImport } | null>(null)

  /** 导入一份 PDF（文件或者从 arXiv 下的）。这场会已经有内容时先问一声：一篇文档和正在开的会搅在一起很难翻 */
  async function importPdfData(name: string, data: ArrayBuffer | Uint8Array) {
    const { importPdf } = await import('./board/importPdf.ts')
    setToast({ ok: true, text: t.app.reading(name) })
    const r = await importPdf(data, (done, total) => setToast({ ok: true, text: t.app.readingPages(name, done, total) }))
    const blank = !lines.length && !pagesRef.current.some((p) => p.underlay || p.graph.nodes.some((n) => n.ink === 'ink'))
    if (blank) {
      placePdf(name, r, true)
      return
    }
    setToast(null)
    setPendingPdf({ name, r })
  }

  /** 把读好的 PDF 放上板；`fresh`：这场会是空的，可以用文档标题当会议名 */
  function placePdf(name: string, r: PdfImport, fresh: boolean) {
    const extra = [
      r.refsSkipped && t.app.refsSkipped(r.refsSkipped),
      r.unread && t.app.unread(r.unread),
      r.scanned && t.app.scanned(r.scanned),
    ].filter((x): x is string => !!x)
    // 这场会还是空的、也没改过名：用文档的标题当会议名
    if (fresh && r.title) setMeeting((m) => (m.titled ? m : { ...m, title: r.title!, titled: true }))
    addPages(r.pages, t.app.pdfNote(name, r.total, r.pages.length, extra))
    setToast({
      ok: true,
      text: r.paper ? t.app.pdfReadyPaper(name) : t.app.pdfReady(name),
    })
  }

  /** 按 arXiv 链接或编号导入论文：桌面版由外壳下载 */
  async function importArxiv(input: string) {
    const { parseArxiv } = await import('./board/arxiv.ts')
    const id = parseArxiv(input)
    if (!id) {
      setToast({ ok: false, text: t.app.arxivBad })
      return
    }
    if (!isDesktop) {
      setToast({ ok: false, text: t.app.arxivWeb })
      return
    }
    exitReplay()
    if (waiting) {
      setToast({ ok: false, text: t.app.busyImport })
      return
    }
    setImportMenu(false)
    setArxivInput('')
    try {
      setToast({ ok: true, text: t.app.arxivDownloading(id) })
      await importPdfData(`arXiv ${id}`, await arxivPdf(id))
    } catch (e) {
      setToast({ ok: false, text: t.app.importFailed(errText(e)) })
    }
  }

  async function importFile(f: File) {
    exitReplay()
    if (waiting) {
      setToast({ ok: false, text: t.app.busyImport })
      return
    }
    try {
      if (/\.pdf$/i.test(f.name) || f.type === 'application/pdf') {
        await importPdfData(f.name, await f.arrayBuffer())
        return
      }
      if (/^image\//.test(f.type) || /\.(png|jpe?g|webp|gif)$/i.test(f.name)) {
        // 图片只交给本机的模型读：桌面版用内置模型（要装看图组件），网页版用填的本地地址
        if (isDesktop && !builtin?.vision) throw new Error(t.app.needVision)
        const url = isDesktop ? await localReady() : baseUrl
        if (!url) throw new Error(t.app.noLocal)
        const { importImage, llamaReader } = await import('./board/importImage.ts')
        setToast({ ok: true, text: t.app.looking(f.name) })
        const r = await importImage(f, llamaReader(url), (done, total) => setToast({ ok: true, text: t.app.readingBlocks(f.name, done, total) }))
        addPages([r.page], t.app.imageNote(f.name, r.read, !!r.capped))
        setToast({ ok: true, text: r.read ? t.app.imageReady(f.name) : t.app.imageNoText(f.name) })
        return
      }
      if (!/\.(excalidraw|json)$/i.test(f.name)) throw new Error(t.app.unsupported)
      const { importExcalidraw } = await import('./board/importExcalidraw.ts')
      const r = importExcalidraw(await f.text())
      // 当前这张板上有东西就新开一张，不和现有的板书搅在一起
      if (session.graph.nodes.some((n) => n.ink === 'ink')) newPage()
      applyEdit(r.graph, t.app.excalidrawNote(f.name, r))
      setTidy((t) => t + 1)
    } catch (e) {
      setToast({ ok: false, text: t.app.importFailed(errText(e)) })
    }
  }

  /** 打开一场会；从搜索结果来的，跳到命中的板和那一句 */
  async function openOther(id: string, hit?: Hit) {
    exitReplay()
    if (id !== meeting.id) {
      if ((lines.length || graph.nodes.length) && meeting.id !== demoMeetingId.current) await meetings.save(snapshot(meeting)).catch(() => undefined)
      const m = await meetings.load(id).catch(() => null)
      if (!m) {
        setToast({ ok: false, text: t.app.openFailed })
        return
      }
      openMeeting(m)
    }
    if (hit?.page !== undefined) goPage(hit.page)
    if (hit?.line !== undefined) {
      setShowLog(true)
      setJumpLine(hit.line)
    }
  }

  async function play() {
    if (recordedOnly) return playRecorded()
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

  /** Replay a recorded sample meeting: no model needed */
  async function playRecorded() {
    if (!demo) return
    try {
      const d = await loadDemo(demo)
      reset()
      const id = newMeetingId()
      demoMeetingId.current = id
      openMeeting(demoMeeting(d, id))
      // reset() only queues leaving an earlier replay; startReplay() checks the ref
      replayRef.current = null
      startReplay()
    } catch (e) {
      setToast({ ok: false, text: t.app.demoFailed(errText(e)) })
    }
  }

  // 现在是哪个模型在画
  const ready = remoteOn || !!online
  /** No model, but recorded sample meetings to replay (the web demo) */
  const recordedOnly = !ready && recordedDemos.length > 0
  const demos = recordedOnly ? inLang(recordedDemos, lang) : inLang(LIVE_DEMOS, lang)
  const demo = demos.some((c) => c.id === demoPick) ? demoPick : demos[0]?.id
  let status: string
  const localName = builtin?.model?.replace(/\.gguf$/, '')
  if (remoteOn) {
    status = remoteView?.model ?? ''
  } else if (remoteState.state === 'checking') {
    status = t.app.checkingRemote
  } else if (builtin?.state === 'ready') {
    status = t.app.builtinReady(localName ?? '', warming)
  } else if (builtin?.state === 'idle') {
    status = t.app.builtinIdle(localName ?? '')
  } else if (builtin?.state === 'starting') {
    status = t.app.builtinStarting(localName)
  } else if (builtin) {
    status = builtin.message ?? t.app.builtinUnavailable
  } else {
    status = online ? `${baseUrl.replace(/^https?:\/\//, '')}` : t.app.noModel
  }
  const remoteBroken = remoteView?.enabled && remoteState.state === 'fail'

  const speakers = useMemo(() => {
    const seen = ['A', 'B', 'C']
    for (const l of lines) if (l.speaker && !seen.includes(l.speaker)) seen.push(l.speaker)
    if (speaker.trim() && !seen.includes(speaker.trim())) seen.push(speaker.trim())
    return seen
  }, [lines, speaker])

  const empty = lines.length === 0 && graph.nodes.length === 0 && !partial
  /** 会上说的话（不算手动修改） */
  const said = lines.filter((l) => !l.manual)
  /** 模型还有没画完的句子 */
  const waiting = lines.some((l) => l.ms === undefined)
  /** 会议记录栏里显示的：回放时只显示到当时说到的那一句 */
  const shown = replay ? lines.slice(0, replay.at ? stepsRef.current[replay.at - 1].line : 0) : lines

  return (
    <div className={`app${showLog ? '' : ' no-log'}`}>
      {splash && <Splash onDone={() => setSplash(false)} />}

      <header className="topbar">
        <div className="brand">
          <Wordmark size={24} className="logo" />
        </div>
        <button type="button" className="meeting-title" onClick={() => setShowMeetings(true)} title={t.app.meetingsTitle}>
          <History size={15} />
          <span>{meeting.title}</span>
        </button>
        <button
          type="button"
          className={`chip status${ready ? ' ok' : ''}${remoteBroken ? ' warn' : ''}`}
          onClick={() => setShowSettings(true)}
          title={t.app.statusTitle}
        >
          <i className="dot" />
          <b className={`where${cloud ? ' cloud' : ''}`}>{cloud ? t.app.cloud : t.app.local}</b>
          <span>{status}</span>
          {remoteBroken && !localOnly && <em>{t.app.remoteDown}</em>}
          {useRemote && localOnly && <em>{t.app.localOnlyMeeting}</em>}
        </button>
        {update && (
          <button type="button" className="chip new-version" onClick={() => setShowUpdate(true)} title={t.app.newVersionTitle}>
            {t.app.newVersion(update.version)}
          </button>
        )}
        {!isDesktop && (
          <a className="chip web-demo" href={RELEASES} target="_blank" rel="noreferrer" title={t.app.webDemoTitle}>
            {t.app.webDemo}
          </a>
        )}
        <span className="spacer" />
        <div className="seg" role="radiogroup" aria-label={t.app.styleAria}>
          <button role="radio" aria-checked={mode === 'ink'} className={mode === 'ink' ? 'on' : ''} onClick={() => setMode('ink')}>
            <Pen size={15} />
            {t.app.ink}
          </button>
          <button role="radio" aria-checked={mode === 'pencil'} className={mode === 'pencil' ? 'on' : ''} onClick={() => setMode('pencil')}>
            <Pencil size={15} />
            {t.app.pencil}
          </button>
        </div>
        <div className="icons">
          <button
            type="button"
            className={`icon${showLog ? ' on' : ''}`}
            onClick={() => setShowLog(!showLog)}
            aria-pressed={showLog}
            aria-label={t.app.log}
            title={t.app.log}
          >
            <TranscriptIcon />
          </button>
          <button
            type="button"
            className={`icon${replay ? ' on' : ''}`}
            onClick={() => (replay ? exitReplay() : startReplay())}
            disabled={!replay && (!stepsRef.current.length || listening || waiting)}
            aria-pressed={!!replay}
            aria-label={t.app.replay}
            title={replay ? t.app.exitReplay : waiting || listening ? t.app.replayBlocked : t.app.replayHint}
          >
            <ReplayIcon />
          </button>
          <button type="button" className="icon" onClick={undo} disabled={!!replay || !editHistory.current.canUndo} aria-label={t.app.undo} title={t.app.undoTitle}>
            <Undo />
          </button>
          <button type="button" className="icon" onClick={redo} disabled={!!replay || !editHistory.current.canRedo} aria-label={t.app.redo} title={t.app.redoTitle}>
            <Redo />
          </button>
          <button type="button" className="icon" onClick={tidyUp} disabled={!graph.nodes.length} aria-label={t.app.tidy} title={t.app.tidyTitle}>
            <Tidy />
          </button>
          <div className="menu-anchor">
            <button
              type="button"
              className={`icon${importMenu ? ' on' : ''}`}
              onClick={() => setImportMenu(!importMenu)}
              disabled={!!replay}
              aria-haspopup="menu"
              aria-expanded={importMenu}
              aria-label={t.app.import}
              title={t.app.importTitle}
            >
              <Upload />
            </button>
            {importMenu && (
              <div className="menu import-menu" role="menu">
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setImportMenu(false)
                    fileInput.current?.click()
                  }}
                >
                  <span>{t.app.chooseFile}</span>
                  <small>{t.app.chooseFileHint}</small>
                </button>
                <form
                  className="arxiv"
                  onSubmit={(e) => {
                    e.preventDefault()
                    void importArxiv(arxivInput)
                  }}
                >
                  <span>{t.app.arxiv}</span>
                  <div>
                    <input
                      value={arxivInput}
                      onChange={(e) => setArxivInput(e.target.value)}
                      onKeyDown={(e) => e.key === 'Escape' && setImportMenu(false)}
                      placeholder={t.app.arxivPlaceholder}
                      spellCheck={false}
                      autoFocus
                    />
                    <button type="submit" className="small" disabled={!arxivInput.trim()}>
                      {t.app.import}
                    </button>
                  </div>
                  {!isDesktop && <small>{t.app.arxivWebHint}</small>}
                </form>
              </div>
            )}
          </div>
          <input
            ref={fileInput}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif,.pdf,application/pdf,.excalidraw,application/json,.json"
            hidden
            onChange={(e) => {
              const f = e.target.files?.[0]
              e.target.value = ''
              if (f) void importFile(f)
            }}
          />
          <div className="menu-anchor">
            <button
              type="button"
              className={`icon${exportMenu ? ' on' : ''}`}
              onClick={() => setExportMenu(!exportMenu)}
              disabled={!graph.nodes.length}
              aria-haspopup="menu"
              aria-expanded={exportMenu}
              aria-label={t.app.export}
              title={t.app.export}
            >
              <Download />
            </button>
            {exportMenu && (
              <div className="menu" role="menu" onMouseLeave={() => setExportMenu(false)}>
                <button type="button" role="menuitem" onClick={() => void exportAs('md')}>
                  <span>Markdown</span>
                  <small>{t.app.exportMd}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('png')}>
                  <span>{t.app.exportPng}</span>
                  <small>{mode === 'ink' ? t.app.exportPngInk : t.app.exportPngPencil}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('svg')}>
                  <span>{t.app.exportSvg}</span>
                  <small>{t.app.exportSvgHint}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('anim')}>
                  <span>{t.app.exportAnim}</span>
                  <small>{t.app.exportAnimHint}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('pdf')}>
                  <span>{t.app.exportPdf}</span>
                  <small>{t.app.exportPdfHint}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('html')}>
                  <span>{t.app.exportHtml}</span>
                  <small>{t.app.exportHtmlHint}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('excalidraw')}>
                  <span>Excalidraw</span>
                  <small>{t.app.exportExcalidrawHint}</small>
                </button>
              </div>
            )}
          </div>
          <button type="button" className="icon" onClick={() => reset()} aria-label={t.app.newMeeting} title={t.app.newMeetingTitle}>
            <Plus />
          </button>
          <button
            type="button"
            className="icon"
            onClick={() => setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length])}
            aria-label={t.theme.aria(t.theme[theme])}
            title={t.theme.title(t.theme[theme])}
          >
            {theme === 'light' ? <Sun /> : theme === 'dark' ? <Moon /> : <Auto />}
          </button>
          <button type="button" className="icon" onClick={() => setShowSettings(true)} aria-label={t.app.settings} title={t.app.settings}>
            <Gear />
          </button>
        </div>
      </header>

      <main
        className={`board${replay ? ' replaying' : ''}${dropping ? ' dropping' : ''}`}
        // 用捕获阶段：铅笔模式下 Excalidraw 自己也接拖进来的文件，会把整块画布换掉
        onDragOverCapture={(e) => {
          if (!e.dataTransfer.types.includes('Files')) return
          e.preventDefault()
          e.stopPropagation()
          setDropping(true)
        }}
        onDragLeave={(e) => {
          if (e.currentTarget === e.target || !e.currentTarget.contains(e.relatedTarget as Node | null)) setDropping(false)
        }}
        onDropCapture={(e) => {
          const f = e.dataTransfer.files[0]
          if (!f) return
          e.preventDefault()
          e.stopPropagation()
          setDropping(false)
          void importFile(f)
        }}
      >
        {mode === 'ink' ? (
          <InkBoard
            graph={graph}
            hot={hot}
            dark={dark}
            messy={messyLater / 100}
            bottomInset={inset}
            tidy={tidy}
            weight={weightLater / 100}
            selection={replay ? null : selection}
            layoutRev={layoutRev}
            underlay={pagesRef.current[page]?.underlay}
            onMoveSel={replay ? undefined : moveSelection}
            onSelect={
              replay
                ? undefined
                : (sel, at) => {
                    selectedAt.current = at
                    setSelection(sel)
                    setRenaming(null)
                  }
            }
            toolbar={
              selection &&
              (renaming !== null ? (
                <form
                  className="rename"
                  onSubmit={(e) => {
                    e.preventDefault()
                    commitRename()
                  }}
                >
                  <input
                    autoFocus
                    value={renaming}
                    onChange={(e) => setRenaming(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Escape') setRenaming(null)
                    }}
                    placeholder={selection.kind === 'edge' ? t.app.renameEdgePlaceholder : t.app.renamePlaceholder}
                    aria-label={t.app.rename}
                  />
                  <button type="submit">{t.app.ok}</button>
                </form>
              ) : (
                <>
                  {!pinned(session.graph, selection) && (
                  <button
                    type="button"
                    onClick={() => {
                      const cur =
                        selection.kind === 'node'
                          ? session.graph.nodes.find((n) => n.id === selection.id)?.label
                          : session.graph.edges.find((e) => e.from === selection.from && e.to === selection.to)?.label
                      setRenaming(cur ?? '')
                    }}
                  >
                    {t.app.rename}
                  </button>
                  )}
                  {selection.kind === 'node' && session.graph.nodes.find((n) => n.id === selection.id)?.rows && (
                    <span className="mark-unit" role="group" aria-label={t.app.markUnitAria}>
                      {(['word', 'sentence', 'block'] as const).map((u) => (
                        <button key={u} type="button" className={markUnit === u ? 'on' : ''} aria-pressed={markUnit === u} onClick={() => setMarkUnit(u)}>
                          {t.app.units[u]}
                        </button>
                      ))}
                    </span>
                  )}
                  {selection.kind === 'node' &&
                    MARKS.map((m) => (
                      <button
                        key={m}
                        type="button"
                        className={(unitSpan(m) ? hasSpan(session.graph.nodes.find((n) => n.id === selection.id), unitSpan(m)) : session.graph.nodes.find((n) => n.id === selection.id)?.marks.includes(m)) ? 'on' : ''}
                        onClick={() => markSelection(m)}
                      >
                        {t.app.marks[m]}
                      </button>
                    ))}
                  {selection.kind === 'node' && !pinned(session.graph, selection) && (
                    <button type="button" onClick={addTerm} title={t.app.addTermTitle}>
                      {t.app.addTerm}
                    </button>
                  )}
                  {!pinned(session.graph, selection) && (
                    <button type="button" className="del" onClick={deleteSelection}>
                      {t.app.delete}
                    </button>
                  )}
                </>
              ))
            }
          />
        ) : (
          <Suspense fallback={<div className="board-loading">{t.app.pencilLoading}</div>}>
            <PencilBoard onApi={setApi} dark={dark} paper={PAPER_LIGHT} />
          </Suspense>
        )}

        {empty && !splash && (
          <div className={`hero${mode === 'ink' && dark ? ' on-board' : ''}`}>
            <span className="kicker">{t.app.kicker}</span>
            <h1>
              {t.app.headline1}
              <br />
              {t.app.headline2}
            </h1>
            <p>
              {t.app.lead(isDesktop)}
            </p>
            <div className="cta">
              {isDesktop && (
                <button type="button" className="primary" onClick={() => void toggleMic()}>
                  {t.app.startMic}
                </button>
              )}
              <button type="button" className={isDesktop ? 'secondary' : 'primary'} onClick={() => void play()} disabled={(!ready && !recordedOnly) || playing}>
                {ready || recordedOnly ? t.app.playDemo : t.app.waitModel}
              </button>
            </div>
            <label className="demo-pick">
              <span>{t.app.demo}</span>
              <select value={demo} onChange={(e) => setDemo(e.target.value)} aria-label={t.app.demoAria}>
                {demos.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            {recordedOnly && (
              <p className="demo-note">
                {t.app.recordedHint}{' '}
                <button type="button" className="link" onClick={() => setShowSettings(true)}>
                  {t.app.connectModel}
                </button>
              </p>
            )}
            {!isDesktop && (
              <p className="demo-note">
                {t.app.webNotice}{' '}
                <a href={RELEASES} target="_blank" rel="noreferrer">
                  {t.app.getDesktop}
                </a>
              </p>
            )}
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
          where={cloud && remoteView ? { cloud: true, host: hostOf(remoteView.baseUrl) } : { cloud: false }}
          onWhere={() => setShowSettings(true)}
        />
        {(pagesRef.current.length > 1 || graph.nodes.length > 0) && (
          <PageStrip
            thumbs={thumbs}
            current={page}
            onGo={goPage}
            onNew={newPage}
            blocked={replay ? t.app.replayNoPage : waiting ? t.app.busyPage : undefined}
            suggest={suggest}
            onDismiss={() => setSuggest(false)}
          />
        )}
        {replay && (
          <ReplayBar
            at={replay.at}
            total={replay.total}
            playing={replay.playing}
            speed={replay.speed}
            elapsed={replay.at ? stepsRef.current[replay.at - 1].t - stepsRef.current[0].t : 0}
            duration={stepsRef.current[replay.total - 1].t - stepsRef.current[0].t}
            onToggle={toggleReplay}
            onSeek={seekReplay}
            onSpeed={() => setReplay({ ...replay, speed: SPEEDS[(SPEEDS.indexOf(replay.speed as 1) + 1) % SPEEDS.length] })}
            onExit={exitReplay}
          />
        )}
      </main>

      <aside className="log" aria-label={t.app.log} hidden={!showLog}>
        <header>
          <h2>{t.app.log}</h2>
          <span className="count">{said.length ? t.app.count(said.length) : ''}</span>
          <button
            type="button"
            className="ghost small"
            onClick={() => void makeSummary()}
            disabled={!summing && (!said.length || !ready)}
            title={
              summing
                ? t.app.stopSumming
                : cloud && remoteView
                  ? t.app.summaryCloud(remoteView.model, hostOf(remoteView.baseUrl))
                  : t.app.summaryLocal
            }
          >
            {summing ? t.app.stop : summary ? t.app.resummarize : t.app.summarize}
          </button>
          <button type="button" className="icon" onClick={() => setShowLog(false)} aria-label={t.app.collapseLog}>
            <Close size={16} />
          </button>
        </header>
        {/* Hidden while replaying, back once the replay reaches the end (e.g. a finished sample meeting) */}
        {(summing || summary) && (!replay || replay.at >= replay.total) && (
          <Suspense fallback={null}>
            <Summary
              text={summary}
              onChange={setSummary}
              busy={!!summing}
              status={summing ?? ''}
              onClear={() => setSummary('')}
              onToast={setToast}
            />
          </Suspense>
        )}
        {renamingWho && (
          <form
            className="rename-who"
            onSubmit={(e) => {
              e.preventDefault()
              renameSpeaker(renamingWho, new FormData(e.currentTarget).get('name') as string)
            }}
          >
            <span>
              {t.app.renameFrom} <b>{renamingWho}</b> {t.app.renameTo}
            </span>
            <input name="name" defaultValue={renamingWho} autoFocus onFocus={(e) => e.currentTarget.select()} onKeyDown={(e) => e.key === 'Escape' && setRenamingWho(null)} />
            <button type="submit" className="small">
              {t.app.renameSpeaker}
            </button>
            <button type="button" className="ghost small" onClick={() => setRenamingWho(null)}>
              {t.app.cancel}
            </button>
          </form>
        )}
        <div className="lines">
          {lines.length === 0 && !partial && <p className="empty">{t.app.logEmpty}</p>}
          {shown.map((l, i) => (
            <Fragment key={i}>
            {pagesRef.current.length > 1 && (i === 0 || (shown[i - 1].page ?? 0) !== (l.page ?? 0)) && (
              <button type="button" className="page-mark" onClick={() => goPage(l.page ?? 0)} disabled={!!replay || waiting} title={t.app.gotoPage}>
                {t.app.pageN((l.page ?? 0) + 1)}
              </button>
            )}
            <article
              className={`line${l.ms === undefined ? ' pending' : ''}${l.manual ? ' manual' : ''}${replay && i === shown.length - 1 ? ' now' : ''}${jumpLine === i ? ' jump' : ''}`}
            >
              {l.manual && <span className="who edit">{t.app.editBadge}</span>}
              {l.speaker && (
                <span
                  className={`who c${speakers.indexOf(l.speaker) % 6}`}
                  role="button"
                  tabIndex={0}
                  title={t.app.renameSpeakerTitle(l.speaker)}
                  onClick={() => setRenamingWho(l.speaker!)}
                  onKeyDown={(e) => e.key === 'Enter' && setRenamingWho(l.speaker!)}
                >
                  {l.speaker}
                </span>
              )}
              <div className="body">
                <p>{l.text}</p>
                {!l.manual && (
                <details>
                  <summary>
                    {l.ms === undefined ? t.app.drawing : `${Math.round(l.ms)} ms`}
                    {l.ms !== undefined && useRemote && l.via && ` · ${l.via}`}
                    {l.fellBack && <em>{t.app.lineFellBack}</em>}
                  </summary>
                  {l.ms !== undefined && (l.heard || l.queueMs) ? <p className="timing">{t.app.timing(lineTiming(l))}</p> : null}
                  {l.ops ? <pre>{l.ops}</pre> : <pre>{t.app.noChange}</pre>}
                </details>
                )}
              </div>
            </article>
            </Fragment>
          ))}
          {partial && !replay && (
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

      {showUpdate && update && (
        <Suspense fallback={null}>
          <UpdatePanel info={update} onClose={() => setShowUpdate(false)} />
        </Suspense>
      )}

      {showSettings && (
        <SettingsPanel
          onClose={() => setShowSettings(false)}
          hotkey={hotkey}
          setHotkey={setHotkey}
          hotkeyErr={hotkeyErr}
          onKeys={() => {
            setShowSettings(false)
            setShowKeys(true)
          }}
          onDiagnostics={() => {
            setShowSettings(false)
            setShowDiag(true)
          }}
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
          localOnly={localOnly}
          glossary={glossary}
          setGlossary={setGlossary}
          meetingGlossary={meetingGlossary}
          setMeetingGlossary={setMeetingGlossary}
          setLocalOnly={setLocalOnly}
          mic={micChoice}
          setMic={setMicChoice}
          source={voiceSource}
          setSource={setVoiceSource}
          diarize={diarize}
          setDiarize={setDiarize}
          autoUpdate={autoUpdate}
          setAutoUpdate={setAutoUpdate}
          onUpdate={(u) => {
            setUpdate(u)
            setShowSettings(false)
            setShowUpdate(true)
          }}
          inkMode={mode === 'ink'}
          messy={messy}
          setMessy={setMessy}
          weight={weight}
          setWeight={setWeight}
        />
      )}
      {showModels && <ModelPanel status={builtin} onClose={() => setShowModels(false)} />}
      {showMeetings && (
        <MeetingsPanel
          currentId={meeting.id}
          onClose={() => setShowMeetings(false)}
          onOpen={(id, hit) => void openOther(id, hit)}
          onNew={(keep) => reset(keep)}
          onRenamed={(id, title) => setMeeting((m) => (m.id === id ? { ...m, title, titled: true } : m))}
        />
      )}
      {pendingPdf && (
        <div className="import-choice" role="dialog" aria-label={t.app.importWhereAria}>
          <span>{t.app.importWhere(pendingPdf.name, pendingPdf.r.pages.length)}</span>
          <button
            type="button"
            className="small"
            onClick={() => {
              const p = pendingPdf
              setPendingPdf(null)
              reset()
              placePdf(p.name, p.r, true)
            }}
          >
            {t.app.importNewMeeting}
          </button>
          <button
            type="button"
            className="ghost small"
            onClick={() => {
              const p = pendingPdf
              setPendingPdf(null)
              placePdf(p.name, p.r, false)
            }}
          >
            {t.app.importHere}
          </button>
        </div>
      )}
      {showKeys && (
        <div className="modal" role="dialog" aria-label={t.app.keys} onClick={(e) => e.target === e.currentTarget && setShowKeys(false)}>
          <div className="panel keys">
            <header>
              <h2>{t.app.keys}</h2>
              <button type="button" className="icon" onClick={() => setShowKeys(false)} aria-label={t.app.close}>
                <Close size={16} />
              </button>
            </header>
            <table>
              <tbody>
                {t.shortcuts.map(([k, what]) => (
                  <tr key={k}>
                    <td>
                      <kbd>{k}</kbd>
                    </td>
                    <td>{what}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="hint">{t.app.keysHint}</p>
          </div>
        </div>
      )}
      {showDiag && (
        <DiagnosticsPanel
          voice={{ listening, device: mic ?? null, choice: micChoice ?? null, source: voiceSource, diarize, error: voiceErr }}
          timing={timingReport(lines, llm.label)}
          onClose={() => setShowDiag(false)}
          onSaved={(ok, text) => setToast({ ok, text })}
        />
      )}
      {toast && (
        <div className={`toast${toast.ok ? '' : ' err'}`} role="status">
          {toast.text}
          {!toast.ok && (
            <button
              type="button"
              className="ghost small"
              onClick={() => {
                void navigator.clipboard
                  ?.writeText(`${toast.text}\n\n${recentErrors().slice(-5).join('\n')}\n${navigator.userAgent}`)
                  .then(() => setToast({ ok: true, text: t.app.copied }))
              }}
            >
              {t.app.copyError}
            </button>
          )}
        </div>
      )}
    </div>
  )
}
