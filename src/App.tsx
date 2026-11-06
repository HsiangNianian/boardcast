import type { ExcalidrawImperativeAPI, ExcalidrawProps } from '@excalidraw/excalidraw/types'
import { type CSSProperties, Fragment, Suspense, lazy, useCallback, useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import cases from '../eval/cases.json'
import { type LayoutMemory, StableLayout, board } from './board/layout.ts'
import type { Look } from './board/scene.ts'
import { type FreeScene, type SceneOverlay, editFreeObject, renameSceneSpeaker, adoptSceneOwner, captureModelCandidates, sceneOwnerAction, applySemanticEdits, moveSceneOwner, mergeScene, protectGraphFields, sceneChanged, sceneEdits, sceneHasContent, sceneBounds, withInheritedInk, withoutInheritedInk, withPageUnderlay, withoutPageUnderlay } from './board/freeScene.ts'
import {
  type Heard,
  type HeardTiming,
  type LlmStatus,
  type VoiceStatus,
  type ShellExitRequest,
  type ShellMenuAction,
  isDesktop,
  shell,
  llmEnsure,
  type UpdateInfo,
  type VoiceSource,
  arxivPdf,
  exportedFiles,
  updates,
  llmStatus,
  llmProblem,
  llmStop,
  localReady,
  localChat,
  on,
  voice,
} from './desktop.ts'
import { type Box, type Graph, emptyGraph } from './engine/graph.ts'
import { type LlmConfig, proposeOps } from './engine/llm.ts'
import { eraseDrafts, formatOps } from './engine/ops.ts'
import type { Utterance } from './engine/prompt.ts'
import { History as EditHistory, type Selection, describe, exists, pinned, remove as removeSel, rename as renameSel, toggleMark } from './engine/edit.ts'
import { MARKS, type Mark } from './engine/graph.ts'
import { BoardModelUnavailable, Session } from './engine/session.ts'
import { MeetingSave, meetingContent, sceneContent, type SaveStatus } from './engine/meetingSave.ts'
import { type Step, diffGraph, inkOnly, replayTo, replaySceneTo } from './engine/steps.ts'
import { latest, MAX_POINTED } from './engine/pointing.ts'
import { firstSeen, renameBy, renameByInSteps, saidAt } from './engine/attribute.ts'
import { settle } from './engine/pending.ts'
import { summarize, summaryFingerprint } from './engine/summary.ts'
import { correctUtterance, parseGlossary } from './engine/glossary.ts'
import { Corrections, casesFile, gather, pairsFile } from './engine/corrections.ts'
import { type Voice, identify, renameVoice } from './engine/voices.ts'
import { type Meeting, type MeetingLine, type Page, MEETING_VERSION, autoTitle, meetings, newMeetingId } from './meetings.ts'
import InkBoard from './ink/InkBoard.tsx'
import ModelPanel from './ModelPanel.tsx'
import { hostOf, isLoopback } from './privacy.ts'
import { type RemoteView, remote } from './remote.ts'
import SettingsPanel, { type RemoteState, errText } from './SettingsPanel.tsx'
import Composer from './ui/Composer.tsx'
import Dialog from './ui/Dialog.tsx'
import ActionMenu from './ui/ActionMenu.tsx'
import ContentList from './ui/ContentList.tsx'
import SpeakersDialog from './ui/SpeakersDialog.tsx'
import type { ImportPreview, ImportTaskState } from './ui/ImportTaskPanel.tsx'
import type { ExportKind } from './exporter.ts'
import { Auto, Close, Download, Gear, History, Lasso, Moon, Pen, Pencil, Plus, Redo, Replay as ReplayIcon, Sun, Tidy, Upload, Transcript as TranscriptIcon, Undo } from './ui/icons.tsx'
import MeetingsPanel from './ui/MeetingsPanel.tsx'
import MeetingTitleDialog from './ui/MeetingTitleDialog.tsx'
import ReplayBar, { SPEEDS, clock } from './ui/ReplayBar.tsx'
import PageStrip from './ui/PageStrip.tsx'
import { SPEAKER_MARKERS, initial, speakerSlot } from './ui/speakers.ts'
import type { PdfImport } from './board/importPdf.ts'
import { type Thumb, pageThumb, thumbOf } from './board/thumb.ts'
import { navTarget, parseNav } from './engine/nav.ts'
import type { Hit } from './engine/search.ts'
import { hasSpan, spanAt, toggleSpan } from './engine/spans.ts'
import DiagnosticsPanel from './ui/DiagnosticsPanel.tsx'
import { errorSummary, lineTiming, recentErrors, recordError, timingReport } from './engine/diagnostics.ts'
import { appleAssetIssue, speechMessage, startVoice } from './engine/speech.ts'
import { type Action, actionFor, shortcutLabel } from './engine/shortcuts.ts'
import { currentLang, msg, useT } from './i18n/index.ts'
import { type SpokenLang, spokenLang } from './engine/lang.ts'
import { type DemoInfo, demoIndex, demoMeeting, loadDemo } from './demo.ts'
import Splash from './ui/Splash.tsx'
import Wordmark from './ui/Wordmark.tsx'
import { DEFAULT_ENGLISH_WEIGHT, DEFAULT_MESSY, DEFAULT_WEIGHT } from './ink/defaults.ts'

type Mode = 'pencil' | 'ink'
type Theme = 'system' | 'light' | 'dark'

type Line = MeetingLine

const LIGHT: Look = { ink: '#1e1e1e', pencil: '#868e96', highlight: '#ffec99', group: '#1971c2', question: '#e03131', note: '#1971c2' }

const THEMES: Theme[] = ['system', 'light', 'dark']
// Excalidraw 很大（连同 mermaid、katex 好几 MB），只在切到铅笔模式时才加载
// 摘要要用 Markdown 渲染库，第一次显示摘要时再加载
const Summary = lazy(() => import('./ui/Summary.tsx'))
const Preparation = lazy(() => import('./ui/Preparation.tsx'))
const UpdatePanel = lazy(() => import('./ui/UpdatePanel.tsx'))
const PencilBoard = lazy(() => import('./board/PencilBoard.tsx'))
const ImportTaskPanel = lazy(() => import('./ui/ImportTaskPanel.tsx'))

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

function storageReason(error: string) {
  return errorSummary(error).signals.find((signal) => signal === 'disk-full' || signal === 'permission-denied' || signal === 'storage-quota')
}

function useMedia(q: string): boolean {
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
  const systemDark = useMedia('(prefers-color-scheme: dark)')
  const compact = useMedia('(max-width: 900px)')
  const dark = theme === 'dark' || (theme === 'system' && systemDark)
  useEffect(() => {
    const el = document.documentElement
    if (theme === 'system') delete el.dataset.theme
    else el.dataset.theme = theme
    save('boardcast.theme', theme)
  }, [theme])

  // 桌面版等待内置服务地址；网页版默认不探测，保留旧版手动配置过的非默认地址。
  const [baseUrl, setBaseUrl] = useState(() => {
    if (isDesktop) return ''
    const previous = load('boardcast.llm', '')
    return load('boardcast.localLlm', previous === 'http://127.0.0.1:8080' ? '' : previous)
  })
  const [online, setOnline] = useState<boolean | null>(null)
  // 桌面版：内置的 llama-server 由外壳拉起，就绪后自动用它的地址
  const [builtin, setBuiltin] = useState<LlmStatus | null>(null)
  // 第一次请求很慢（Vulkan 现场编译着色器，提示词前缀也还没进缓存），就绪后先空跑一次
  const [warming, setWarming] = useState(false)
  const [warmupError, setWarmupError] = useState<string | null>(null)
  const [showModels, setShowModels] = useState<boolean | 'speech'>(false)
  const [showSettings, setShowSettings] = useState<boolean | 'models'>(false)
  const [showPreparation, setShowPreparation] = useState(false)
  const [announceTranscript, setAnnounceTranscript] = useState(() => load('boardcast.announce-transcript', 'on') !== 'off')
  const warmed = useRef<string | null>(null)
  // 正在用远程模型时不预热内置模型（它这时只是兜底）；轮询的闭包里拿不到最新的 state，用 ref
  const remoteFirst = useRef(false)
  useEffect(() => {
    if (!isDesktop) return
    let alive = true
    let timer: ReturnType<typeof setTimeout>
    let warmup: AbortController | undefined
    const poll = async () => {
      const s = await llmStatus().catch(() => null)
      if (!alive) return
      setBuiltin(s)
      setOnline(s?.state === 'ready')
      if (s?.state === 'ready' && s.url) {
        setBaseUrl(s.url)
        if (warmed.current !== s.url && !remoteFirst.current) {
          warmed.current = s.url
          warmup?.abort()
          const current = warmup = new AbortController()
          const timeout = setTimeout(() => current.abort(new DOMException(msg().errors.builtinTimeout, 'TimeoutError')), 180_000)
          setWarming(true)
          setWarmupError(null)
          void proposeOps({ baseUrl: s.url, noThink: true }, emptyGraph(), [], { text: '大家好。' }, current.signal)
            .catch(error => {
              if (!alive || warmup !== current) return
              console.error('[local warmup]', error)
              setWarmupError(errText(error))
            })
            .finally(() => { clearTimeout(timeout); if (alive && warmup === current) setWarming(false) })
        }
      } else {
        warmup?.abort()
        warmup = undefined
        warmed.current = null
        setWarming(false)
        setWarmupError(null)
      }
      // 还在启动就勤快点问；定下来以后慢慢问
      timer = setTimeout(poll, s?.state === 'starting' ? 700 : 5000)
    }
    void poll()
    return () => {
      alive = false
      warmup?.abort()
      warmed.current = null
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
  const remoteCheck = useRef(0)
  const checkRemote = useCallback(async () => {
    const check = ++remoteCheck.current
    const v = await remote.get().catch(() => null)
    if (check !== remoteCheck.current) return
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
      if (check !== remoteCheck.current) return
      remoteFirst.current = !localOnlyRef.current
      setRemoteState({ state: 'ok', ms: t.ms })
      // 远程能用：内置模型停下来，把显存让出来；某一句远程出错时再临时拉起（这场会只用本地时不停）
      if (isDesktop && !localOnlyRef.current) {
        const b = await llmStatus().catch(() => null)
        if (check === remoteCheck.current && !localOnlyRef.current && b && (b.state === 'ready' || b.state === 'starting')) await llmStop().catch(() => undefined)
      }
    } catch (e) {
      if (check !== remoteCheck.current) return
      remoteFirst.current = false
      setRemoteState({ state: 'fail', message: errText(e) })
      if (isDesktop) void llmEnsure().catch(() => undefined)
    }
  }, [])
  useEffect(() => {
    void checkRemote()
    return () => { remoteCheck.current++ }
  }, [checkRemote])
  const useRemote = remoteState.state === 'ok' && !!remoteView?.enabled
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

  // 实际用来出板书的接口：远程优先，本地兜底
  const llm = useMemo<LlmConfig>(() => {
    const local: LlmConfig = {
      baseUrl, noThink: true, label: isDesktop ? t.app.builtinLabel : t.app.localLabel,
      send: baseUrl ? (body, signal) => localChat(baseUrl, body, signal) : undefined,
    }
    if (!remoteOn || !remoteView) return local
    // 桌面版的内置模型这时多半在待命：某一句远程失败了，先把它拉起来、等就绪，再用它画这一句
    const fallback: LlmConfig | undefined = isDesktop
      ? {
          ...local,
          baseUrl: 'builtin',
          send: async (body, signal) => localChat(await localReady(), body, signal),
        }
      : baseUrl
        ? local
        : undefined
    return {
      baseUrl: remoteView.baseUrl,
      model: remoteView.model,
      flavor: 'openai',
      send: async (body, signal) => {
        if (localOnlyRef.current) throw new Error(msg().app.localOnlyMeeting)
        return remote.send(body, signal)
      },
      label: t.app.remoteLabel(remoteView.model),
      fallback,
    }
  }, [baseUrl, remoteOn, remoteView, t])

  const [lines, setLines] = useState<Line[]>([])
  // 会议摘要：一键生成，可以手改，跟着会议一起存、一起导出
  const [summary, setSummary] = useState('')
  const summaryRef = useRef('')
  summaryRef.current = summary
  const [summarySource, setSummarySource] = useState<string | undefined>(undefined)
  const summarySourceRef = useRef<string | undefined>(undefined)
  summarySourceRef.current = summarySource
  const [summaryCandidate, setSummaryCandidate] = useState<{ text: string; source: string } | null>(null)
  const [clearedSummary, setClearedSummary] = useState<{ text: string; source?: string } | null>(null)
  const summaryEditRevision = useRef(0)
  /** 正在生成摘要：给界面看的进度；null 表示没在生成 */
  const [summing, setSumming] = useState<string | null>(null)
  const summingFor = useRef<AbortController | null>(null)
  const taskEpoch = useRef(0)
  const transitionRef = useRef(false)
  const transitionError = useRef<string | null>(null)
  const [transitionBusy, setTransitionBusy] = useState(false)
  const pendingSwitch = useRef<((proceed: boolean) => void) | null>(null)
  const [confirmSwitch, setConfirmSwitch] = useState(false)
  useEffect(() => () => pendingSwitch.current?.(false), [])
  const [reviewing, setReviewing] = useState(false)
  const [reviewDialog, setReviewDialog] = useState(false)
  const [reviewStage, setReviewStage] = useState<'stopping' | 'waiting' | 'saving' | null>(null)
  const [reviewError, setReviewError] = useState<string | null>(null)
  const finishWaiting = useRef<(() => void) | null>(null)
  const [graph, setGraph] = useState<Graph>(emptyGraph)
  const [hot, setHot] = useState<Set<string>>(new Set())
  /** 溯源：鼠标停在哪个说话人上，白板就标出他提的东西 */
  const [traceWho, setTraceWho] = useState<string | null>(null)
  const [speaker, setSpeaker] = useState('A')
  // 会议记录栏：窄屏默认收起
  const [showLog, setShowLog] = useState(() => load('boardcast.log', innerWidth > 1100 ? 'open' : 'closed') === 'open')
  const [sidebarTab, setSidebarTab] = useState<'transcript' | 'summary' | 'contents'>('transcript')
  const [sidebarWidth, setSidebarWidth] = useState(() => Math.max(280, Math.min(400, Number(load('boardcast.sidebar-width', '320')) || 320)))
  const [focusContent, setFocusContent] = useState<{ selection?: Selection; box?: Box } | null>(null)
  useEffect(() => {
    if (!focusContent || compact && showLog) return
    // Wait for the compact visibility change to paint before moving keyboard focus.
    let frame = requestAnimationFrame(() => { frame = requestAnimationFrame(() => document.getElementById('board-panel')?.focus()) })
    return () => cancelAnimationFrame(frame)
  }, [focusContent, compact, showLog])
  const [followTranscript, setFollowTranscript] = useState(true)
  const [unreadFrom, setUnreadFrom] = useState<number | null>(null)
  const followTranscriptRef = useRef(true)
  const followLatest = (follow: boolean) => {
    if (follow) setUnreadFrom(null)
    else if (followTranscriptRef.current) setUnreadFrom(linesRef.current.filter((line) => !line.manual).length)
    followTranscriptRef.current = follow
    setFollowTranscript(follow)
  }
  // 会议记录收起来了，悬停的说话人也就没了
  useEffect(() => {
    if (!showLog) setTraceWho(null)
  }, [showLog])
  // 启动动画每次打开应用放一次
  const [splash, setSplash] = useState(true)
  // 全板美化与实时增量排版分开，避免每句话都打乱正在看的位置。
  const [tidy, setTidy] = useState(0)
  const [tidying, setTidying] = useState(false)
  // 导出菜单和保存后的提示
  const [exportMenu, setExportMenu] = useState(false)
  const exportTrigger = useRef<HTMLButtonElement>(null)
  const moreTrigger = useRef<HTMLButtonElement>(null)
  const [meetingMenu, setMeetingMenu] = useState(false)
  useEffect(() => { setMeetingMenu(false) }, [compact])
  const [exportScope, setExportScope] = useState<'meeting' | 'board'>('meeting')
  const [exporting, setExporting] = useState(false)
  const exportingRef = useRef(false)
  const exportFocusEpoch = useRef<number | null>(null)
  useLayoutEffect(() => {
    if (exporting || exportFocusEpoch.current === null) return
    const epoch = exportFocusEpoch.current
    exportFocusEpoch.current = null
    if (epoch === taskEpoch.current && document.activeElement === document.body && !document.querySelector('dialog[open]')) {
      (moreTrigger.current ?? exportTrigger.current)?.focus()
    }
  })
  const [openingExport, setOpeningExport] = useState(false)
  const openingExportRef = useRef(false)
  const [showKeys, setShowKeys] = useState(false)
  const [hotkey, setHotkey] = useState(() => load('boardcast.hotkey', 'Ctrl+Shift+Space'))
  const [hotkeyErr, setHotkeyErr] = useState<{ accel: string; message: string } | null>(null)
  const [hotkeyBusy, setHotkeyBusy] = useState(false)
  const hotkeyChange = useRef({ applied: '', version: 0, pending: Promise.resolve() })
  const [toast, setToast] = useState<{ ok: boolean; text: string; path?: string; pending?: boolean; retryExport?: ExportKind } | null>(null)
  useEffect(() => {
    if (!toast || toast.path || toast.pending || toast.retryExport) return
    // 报错多留一会儿，来得及复制
    const t = setTimeout(() => setToast(null), toast.ok ? 4000 : 10000)
    return () => clearTimeout(t)
  }, [toast])
  // Input and playback controls occupy their own row below the canvas.
  const inset = 0
  // Without a ready model, replay bundled samples on both web and desktop.
  const lang = currentLang()
  const [recordedDemos, setRecordedDemos] = useState<DemoInfo[]>([])
  useEffect(() => {
    void demoIndex().then(setRecordedDemos)
  }, [])
  const [demoPick, setDemo] = useState('')
  /** Untouched samples stay out of personal meeting history. */
  const recordedMeeting = useRef<{ id: string; content: string } | null>(null)
  const [playing, setPlaying] = useState(false)
  // 用 state 而不是 ref：切回铅笔模式时 Excalidraw 重新挂载，要让重画的 effect 再跑一次
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null)
  const [mode, setMode] = useState<Mode>(() => (load('boardcast.mode', 'ink') === 'pencil' ? 'pencil' : 'ink'))
  const [modeHint, setModeHint] = useState<'hidden' | 'visible' | 'dismissed'>(() => load('boardcast.mode-hint', '') === 'dismissed' ? 'dismissed' : 'hidden')
  const [messy, setMessy] = useState(() => {
    const v = Number(load('boardcast.messy', String(DEFAULT_MESSY * 100)))
    return Number.isFinite(v) ? Math.round(Math.min(100, Math.max(0, v))) : DEFAULT_MESSY * 100
  })
  // 笔画粗细：相对基准笔宽的百分比，默认 65
  const [weight, setWeight] = useState(() => {
    const v = Number(load('boardcast.weight', String(DEFAULT_WEIGHT * 100)))
    return Number.isFinite(v) ? Math.round(Math.min(200, Math.max(50, v)) / 5) * 5 : DEFAULT_WEIGHT * 100
  })
  const [englishWeight, setEnglishWeight] = useState(() => {
    const v = Number(load('boardcast.englishWeight', String(DEFAULT_ENGLISH_WEIGHT * 100)))
    return Number.isFinite(v) ? Math.round(Math.min(200, Math.max(50, v)) / 5) * 5 : DEFAULT_ENGLISH_WEIGHT * 100
  })
  // 拖滑杆时每一格都要重算全部笔画，让它慢半拍，拖动本身不卡
  const messyLater = useDeferredValue(messy)
  const weightLater = useDeferredValue(weight)
  const englishWeightLater = useDeferredValue(englishWeight)
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
  const lineList = useRef<HTMLDivElement>(null)
  const transcriptScroller = () => {
    const list = lineList.current
    if (!list) return null
    // 低矮窗口由整个侧栏滚动。
    return getComputedStyle(list).overflowY === 'visible' ? list.closest<HTMLElement>('.log') : list
  }
  const transcriptAnchor = useRef<{ element: Element; offset: number } | null>(null)
  const resizingTranscript = useRef(false)
  const rememberTranscript = (scroller: HTMLElement) => {
    const children = lineList.current?.children
    if (!children?.length || followTranscriptRef.current) return
    const middle = scroller.getBoundingClientRect().top + scroller.clientHeight / 2
    let start = 0, end = children.length - 1
    while (start < end) {
      const index = Math.floor((start + end) / 2)
      if (children[index].getBoundingClientRect().bottom < middle) start = index + 1
      else end = index
    }
    const element = children[start], box = element.getBoundingClientRect()
    transcriptAnchor.current = { element, offset: (middle - box.top) / Math.max(1, box.height) }
  }
  const restoreTranscript = () => {
    if (!resizingTranscript.current || !lineList.current?.clientHeight) return
    const scroller = transcriptScroller()
    if (!scroller?.clientHeight) return
    resizingTranscript.current = false
    if (followTranscriptRef.current) scroller.scrollTop = scroller.scrollHeight
    else {
      const anchor = transcriptAnchor.current
      if (!anchor || !lineList.current?.contains(anchor.element)) return
      const box = anchor.element.getBoundingClientRect()
      scroller.scrollTop += box.top + anchor.offset * box.height - scroller.getBoundingClientRect().top - scroller.clientHeight / 2
    }
  }
  useEffect(() => {
    let frame = 0
    const resize = () => {
      // 等紧凑布局切换滚动容器后再恢复，期间的滚动事件不能改掉阅读锚点。
      resizingTranscript.current = true
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => { frame = requestAnimationFrame(() => {
        restoreTranscript()
      }) })
    }
    window.addEventListener('resize', resize)
    return () => { window.removeEventListener('resize', resize); cancelAnimationFrame(frame) }
  }, [])
  useEffect(() => { restoreTranscript() }, [showLog, sidebarTab])

  // —— 手动修改：撤销历史、选中 ——
  /** 撤销的一步：板书 + 排版记住的位置（拖动只改位置；撤销删除时东西也回到原处） */
  type Snap = { graph: Graph; layout: LayoutMemory; scene?: FreeScene }
  const sceneRef = useRef<FreeScene | undefined>(undefined)
  const displayedSceneRef = useRef<FreeScene | undefined>(undefined)
  const [sceneOverlay, setSceneOverlay] = useState<SceneOverlay | undefined>(undefined)
  const [sceneRev, setSceneRev] = useState(0)
  const pencilBatch = useRef<Snap | null>(null)
  const pencilTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pencilPointerDown = useRef(false)
  const pencilTextEditing = useRef(false)
  const applyingPencilScene = useRef(false)
  const livePencilApi = useRef<ExcalidrawImperativeAPI | null>(null)
  const pencilReady = useRef(false)
  const [pencilInitialized, setPencilInitialized] = useState(false)
  const pencilView = useRef({ had: false, tidy: 0, reading: false })
  const [pencilReading, setPencilReading] = useState(false)
  const missingAssets = useRef<string[] | undefined>(undefined)
  const [assetIssues, setAssetIssues] = useState<NonNullable<Meeting['assetIssues']>>([])
  const editHistory = useRef(new EditHistory<Snap>())
  /** 模型刚画完就被手动改了的那几轮，记成纠错样本（跟着会议存） */
  const fixes = useRef(new Corrections())
  /** 排版记忆变了（拖动、撤销）就加一，让白板重新排版 */
  const [layoutRev, setLayoutRev] = useState(0)
  /** 最近一张墨线板（不含铅笔稿）：撤销时退回到这里 */
  const lastInk = useRef<Graph>(emptyGraph())
  const [, setUndoTick] = useState(0)
  const [selection, setSelection] = useState<Selection | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)
  const inkRenameButton = useRef<HTMLButtonElement>(null)

  // —— 回放：墨线板书每变一次记一步（和上一版的差异），回放时从空板依次打上 ——
  const stepsRef = useRef<Step[]>([])
  /** 最近记下的那一版墨线板书 */
  const recorded = useRef<Graph>(emptyGraph())
  const recordedScene = useRef<FreeScene | undefined>(undefined)
  const pencilReflowStep = useRef<Step | null>(null)
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
    base: pagesRef.current[pageRef.current]?.base,
    scene: sceneRef.current,
  })

  // The visible board is independent of the workers that finish on their original pages.
  const sessionRef = useRef<Session | null>(null)
  if (!sessionRef.current) {
    sessionRef.current = new Session(llm)
    sessionRef.current.graph = graph
  }
  const session = sessionRef.current
  useEffect(() => {
    session.llm = llm
    for (const worker of pageSessions.current.values()) worker.llm = llm
  }, [llm, session])
  useEffect(() => {
    session.terms = terms
    for (const worker of pageSessions.current.values()) worker.terms = terms
  }, [terms, session])

  // —— 会议：自动保存，下次打开接着开 ——
  type MeetingMeta = Pick<Meeting, 'id' | 'title' | 'titled' | 'createdAt'>
  const [meeting, setMeeting] = useState<MeetingMeta>(() => ({ id: newMeetingId(), title: msg().app.untitled, createdAt: Date.now() }))
  const meetingRef = useRef(meeting)
  meetingRef.current = meeting
  const [saveStatus, setSaveStatus] = useState<SaveStatus>({ state: 'idle' })
  const writerRef = useRef<MeetingSave | null>(null)
  writerRef.current ??= new MeetingSave((m) => meetings.save(m), setSaveStatus)
  const writer = writerRef.current
  const autosave = useRef<ReturnType<typeof setTimeout> | null>(null)
  const loadedMeeting = useRef<string | null>(null)
  const [restored, setRestored] = useState(false)
  const [showMeetings, setShowMeetings] = useState(false)
  const [libraryHit, setLibraryHit] = useState<Hit | null>(null)
  const [showMeetingTitle, setShowMeetingTitle] = useState(false)
  const [libraryAction, setLibraryAction] = useState<'replay' | 'export' | null>(null)
  const linesRef = useRef<Line[]>([])
  linesRef.current = lines
  const pageSessions = useRef(new Map<number, Session>())
  const meetingIdRef = useRef(meeting.id)
  meetingIdRef.current = meeting.id

  /** 把一场存过的会议放回界面上：板书、上下文、会议记录、排版位置 */
  const openMeeting = useCallback(
    (m: Meeting) => {
      cancelMeetingWork()
      cancelImport()
      setSidebarTab('transcript')
      writer.reset(m.updatedAt)
      loadedMeeting.current = m.id
      meetingIdRef.current = m.id
      meetingRef.current = { id: m.id, title: m.title, titled: m.titled, createdAt: m.createdAt }
      missingAssets.current = m.missingAssets
      setAssetIssues(m.assetIssues ?? m.missingAssets?.map((key) => ({ key, reason: 'missing' as const })) ?? [])
      restoreScene(m.scene ?? m.pages?.[m.page ?? 0]?.scene)
      board.restore(m.layout)
      editHistory.current.clear()
      fixes.current.load(m.corrections)
      lastInk.current = m.graph
      setSelection(null)
      session.graph = m.graph
      session.history = m.history
      setGraph(m.graph)
      setLines(m.lines)
      linesRef.current = m.lines
      setReplay(null)
      replayRef.current = null
      // 旧版本存的会议没有逐步记录：整块板算一步
      const seed = m.steps?.length ? null : diffGraph(emptyGraph(), inkOnly(m.graph))
      stepsRef.current = m.steps?.length ? m.steps : seed || sceneRef.current ? [{ t: m.updatedAt, line: m.lines.length, d: seed ?? {}, scene: sceneRef.current }] : []
      recorded.current = replayTo(stepsRef.current, stepsRef.current.length)
      recordedScene.current = replaySceneTo(stepsRef.current, stepsRef.current.length)
      // Seed every legacy board before remembering content, so browsing is not an edit.
      pagesRef.current = (m.pages?.length ? m.pages : [{ graph: m.graph, layout: m.layout, steps: stepsRef.current, scene: sceneRef.current }]).map((p) => {
        if (p.steps?.length) return p
        const seed = diffGraph(emptyGraph(), inkOnly(p.graph))
        return { ...p, steps: seed || p.scene ? [{ t: m.updatedAt, line: m.lines.length, d: seed ?? {}, scene: p.scene }] : [] }
      })
      const cur = Math.min(m.page ?? 0, pagesRef.current.length - 1)
      pageRef.current = cur
      setPage(cur)
      thumbRef.current = pagesRef.current[cur].thumb
      setThumbs(pagesRef.current.map((p) => p.thumb))
      suggested.current = new Set()
      setSuggest(false)
      summingFor.current?.abort()
      setSummary(m.summary ?? '')
      summaryRef.current = m.summary ?? ''
      const basis = m.summarySource ?? (m.summary ? summaryFingerprint(m.pages?.length ? m.pages.map((p, i) => i === (m.page ?? 0) ? m.graph : p.graph) : [m.graph], m.lines) : undefined)
      setSummarySource(basis)
      summarySourceRef.current = basis
      setSummaryCandidate(null)
      setClearedSummary(null)
      summaryEditRevision.current++
      voicesRef.current = m.voices ?? []
      setRenamingWho(null)
      setShowMeetingTitle(false)
      setLocalOnly(!!m.localOnly)
      localOnlyRef.current = !!m.localOnly
      setMeetingGlossary(m.glossary ?? '')
      meetingGlossaryRef.current = m.glossary ?? ''
      setMeeting({ id: m.id, title: m.title, titled: m.titled, createdAt: m.createdAt })
      save('boardcast.meeting', m.id)
    },
    [session, writer],
  )

  // 启动时打开上次那场会
  useEffect(() => {
    const last = load('boardcast.meeting', '')
    const epoch = taskEpoch.current
    const id = meetingIdRef.current
    void (async () => {
      const m = last ? await meetings.load(last).catch((error) => {
        if (epoch === taskEpoch.current) setToast({ ok: false, text: msg().app.openFailedReason(errText(error)) })
        return null
      }) : null
      if (m && epoch === taskEpoch.current && id === meetingIdRef.current && writer.status.state === 'idle' && !hasMeetingContent()) openMeeting(m)
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
      scene: sceneRef.current,
      missingAssets: missingAssets.current,
      summary: summaryRef.current || undefined,
      summarySource: summaryRef.current ? summarySourceRef.current : undefined,
      voices: voicesRef.current.length ? voicesRef.current : undefined,
      steps: stepsRef.current.length ? stepsRef.current : undefined,
      localOnly: localOnlyRef.current || undefined,
      glossary: meetingGlossaryRef.current.trim() || undefined,
      corrections: fixes.current.list.length ? fixes.current.list : undefined,
      // 只有一张板、但它垫着导入页或者带着底板时也要存 pages：底图、底板存在这里
      ...(pagesRef.current.length > 1 || pagesRef.current.some((p) => p.underlay || p.base)
        ? { pages: pagesRef.current.map((p, i) => (i === pageRef.current ? capture() : p)), page: pageRef.current }
        : {}),
    }),
    [session],
  )

  function hasMeetingContent() {
    return meetingRef.current.titled || linesRef.current.length > 0 || pagesRef.current.some((p, i) => {
      const g = i === pageRef.current ? session.graph : p.graph
      return !!p.underlay || g.nodes.length > 0 || sceneHasContent(i === pageRef.current ? sceneRef.current : p.scene)
    }) || !!summaryRef.current || !!meetingGlossaryRef.current.trim()
  }

  function untouchedDemo() {
    const demo = recordedMeeting.current
    if (demo?.id !== meetingIdRef.current) return false
    if (demo.content === meetingContent(snapshot(meetingRef.current))) return true
    recordedMeeting.current = null
    return false
  }

  async function retrySave(commitEdit = true): Promise<boolean> {
    if (commitEdit) commitPencilEdit()
    if (autosave.current) clearTimeout(autosave.current)
    if (untouchedDemo() || (!hasMeetingContent() && writer.status.state === 'idle')) return true
    if (writer.unchanged(snapshot(meetingRef.current))) return true
    const id = meetingIdRef.current
    try {
      await refreshCurrentScene()
      await refreshOtherScenes()
      if (meetingIdRef.current !== id) return false
      await writer.flush(() => snapshot(meetingRef.current))
      if (meetingIdRef.current === id) {
        save('boardcast.meeting', id)
        const title = snapshot(meetingRef.current).title
        if (title !== meetingRef.current.title) setMeeting((m) => m.id === id ? { ...m, title } : m)
      }
      return true
    } catch (error) {
      if (meetingIdRef.current === id) writer.failed(error)
      console.error('saving the meeting failed', error)
      return false
    }
  }

  // 内容或名称一变，停半秒存一次；尚未命名的空会议不存
  useEffect(() => {
    // 回放画面和未修改的示例不存；示例的首次编辑进入正常保存流程。
    // New work can be saved while the previous meeting is still loading.
    if (meeting.id !== meetingIdRef.current || replayRef.current || untouchedDemo()) return
    if (loadedMeeting.current === meeting.id) {
      loadedMeeting.current = null
      writer.remember(snapshot(meetingRef.current))
      return
    }
    if (!hasMeetingContent() && writer.status.state === 'idle') return
    if (transitionRef.current) return
    if (writer.unchanged(snapshot(meetingRef.current))) return
    writer.changed()
    autosave.current = setTimeout(() => void retrySave(!isPencilEditing()), 500)
    return () => { if (autosave.current) clearTimeout(autosave.current) }
  }, [graph, lines, summary, summarySource, localOnly, meetingGlossary, layoutRev, sceneRev, page, meeting, snapshot, writer, transitionBusy])

  useEffect(() => {
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (writer.status.state === 'idle' || writer.status.state === 'saved') return
      event.preventDefault()
      event.returnValue = ''
    }
    addEventListener('beforeunload', beforeUnload)
    return () => removeEventListener('beforeunload', beforeUnload)
  }, [writer])

  // 墨线板书变了就记一步（铅笔稿不记）；会议记录说到第几句也记下，回放时字幕跟着走
  useEffect(() => {
    if (replayRef.current) return
    // A scene-bearing step is recorded after its regenerated objects are merged below.
    if (!sceneRef.current) recordBoardStep()
  }, [graph, session])

  useEffect(() => {
    // 桌面版的地址是内置服务每次启动时随机分配的端口，记下来下次也用不上
    if (isDesktop) return
    save('boardcast.localLlm', baseUrl)
    if (!baseUrl) {
      // 没填地址：不是「连不上」，只是没配置，设置里的状态点不标红
      setOnline(null)
      return
    }
    if (!isLoopback(baseUrl)) { setOnline(false); return }
    let alive = true
    // 连得上就每 5 秒看一眼；网页版连不上就越等越久（最多一分钟），免得没开本地模型时满屏报错。
    // 页面切到后台时先不看，切回来立刻看一次
    const backoff = (w: number) => Math.min(60000, w * 2)
    let wait = 5000
    let t = 0
    const next = () => {
      clearTimeout(t)
      if (alive && document.visibilityState === 'visible') t = window.setTimeout(ping, wait)
    }
    const ping = () =>
      fetch(`${baseUrl}/health`, { signal: AbortSignal.timeout(5000), redirect: 'error' })
        .then((r) => {
          if (!alive) return
          setOnline(r.ok)
          wait = r.ok ? 5000 : backoff(wait)
        })
        .catch(() => {
          if (!alive) return
          setOnline(false)
          wait = backoff(wait)
        })
        .finally(next)
    const onVisible = () => {
      if (document.visibilityState !== 'visible') return clearTimeout(t)
      wait = 5000
      clearTimeout(t)
      void ping()
    }
    void ping()
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      alive = false
      clearTimeout(t)
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [baseUrl])

  function restoreScene(state: FreeScene | undefined) {
    setFocusContent(null)
    if (pencilTimer.current) clearTimeout(pencilTimer.current)
    pencilBatch.current = null
    pencilTextEditing.current = false
    pencilPointerDown.current = false
    sceneRef.current = state
    // Ignore an old canvas's unmount callback until the new scene is displayed.
    displayedSceneRef.current = undefined
    pencilReady.current = false
    setPencilInitialized(false)
    setSceneOverlay(state?.overlay)
    setSceneRev((n) => n + 1)
  }

  function recordBoardStep(state = sceneRef.current, finishEdit = false) {
    if (replayRef.current || isPencilEditing() || pencilBatch.current) return
    const ink = inkOnly(session.graph)
    const d = diffGraph(recorded.current, ink)
    if (!d && !sceneHasContent(state) && !sceneHasContent(recordedScene.current)) return
    // The first generated view establishes a baseline without extending the recording.
    if (!recordedScene.current && state && !sceneChanged(mergeScene(undefined, state.generated), state)) {
      recordedScene.current = structuredClone(state)
    }
    if (!d && !sceneChanged(recordedScene.current, state)) {
      pencilReflowStep.current = null
      return
    }
    if (!d && state && recordedScene.current && pencilReflowStep.current === stepsRef.current.at(-1)
      && !sceneChanged(mergeScene(recordedScene.current, state.generated), state)) {
      // Only the semantic edit just committed can absorb its generated reflow.
      const scene = structuredClone(state)
      stepsRef.current = stepsRef.current.map((step, i, all) => i === all.length - 1 ? { ...step, scene } : step)
      recordedScene.current = scene
      pencilReflowStep.current = null
      return
    }
    const step: Step = {
      t: Date.now(), line: linesRef.current.length, d: d ?? {},
      ...(state || recordedScene.current ? { scene: state ? structuredClone(state) : null } : {}),
    }
    stepsRef.current = [...stepsRef.current, step]
    pencilReflowStep.current = finishEdit && d ? step : null
    recorded.current = ink
    recordedScene.current = state ? structuredClone(state) : undefined
  }

  function isPencilEditing() {
    return pencilPointerDown.current || pencilTextEditing.current
  }

  /** One pointer gesture or text editing session is one undo/replay step. */
  function commitPencilEdit(force = true) {
    if (!force && isPencilEditing()) return
    pencilPointerDown.current = false
    if (pencilTimer.current) clearTimeout(pencilTimer.current)
    const before = pencilBatch.current
    if (!before) {
      // Scene-backed history waits until generated elements match the graph.
      if (!sceneRef.current) recordBoardStep()
      return
    }
    pencilBatch.current = null
    editHistory.current.record(before)
    lastInk.current = sessionRef.current!.graph
    setGraph(lastInk.current)
    setUndoTick((n) => n + 1)
    setLayoutRev((n) => n + 1)
    setSceneRev((n) => n + 1)
    recordBoardStep(sceneRef.current, true)
  }

  async function waitForPencilEdit(signal: AbortSignal, at = pageRef.current) {
    while (!signal.aborted && at === pageRef.current && (isPencilEditing() || pencilBatch.current)) {
      await new Promise((resolve) => setTimeout(resolve, 5))
    }
  }

  const pencilSceneChanged: NonNullable<ExcalidrawProps['onChange']> = (elements, appState, files) => {
    const before = displayedSceneRef.current
    if (!before || !pencilReady.current || !api || livePencilApi.current !== api || applyingPencilScene.current || mode !== 'pencil' || replayRef.current || transitionRef.current) return
    // An unmounted canvas can deliver its last onChange after a replacement is ready.
    if (sceneChanged({ ...before, elements, files }, { ...before, elements: api.getSceneElementsIncludingDeleted(), files: api.getFiles() })) return
    const textEnded = pencilTextEditing.current && !appState.editingTextElement
    pencilTextEditing.current = !!appState.editingTextElement
    const next = withoutInheritedInk(withoutPageUnderlay({ ...before, elements, files }))
    const changed = sceneChanged(before, next)
    if (!changed && !textEnded) return
    if (changed) {
      pencilBatch.current ??= { graph: session.graph, layout: board.snapshot(), scene: sceneRef.current }
      const changes = sceneEdits(session.graph, before.elements, next.elements, pencilBatch.current.graph)
      const state = mergeScene(structuredClone(next), before.generated)
      sceneRef.current = state
      displayedSceneRef.current = state
      session.graph = changes.graph
      for (const move of changes.moves) board.move(move.id, move.dx, move.dy)
      writer.changed()
      if (autosave.current) clearTimeout(autosave.current)
      autosave.current = setTimeout(() => void retrySave(false), 500)
    }
    if (pencilTimer.current) clearTimeout(pencilTimer.current)
    if (!isPencilEditing() && pencilBatch.current) pencilTimer.current = setTimeout(() => commitPencilEdit(false), textEnded ? 0 : 500)
  }

  useEffect(() => {
    if (mode !== 'pencil' || !api) return
    return api.onPointerDown(() => { pencilPointerDown.current = true })
  }, [api, mode])

  const pencilApiReady = useCallback((next: ExcalidrawImperativeAPI) => {
    livePencilApi.current = next
    pencilReady.current = false
    setPencilInitialized(false)
    displayedSceneRef.current = undefined
    setApi(next)
  }, [])

  function changeMode(next: Mode) {
    if (compact) setShowLog(false)
    if (mode === next) return
    commitPencilEdit()
    livePencilApi.current = null
    pencilReady.current = false
    setPencilInitialized(false)
    displayedSceneRef.current = undefined
    setApi(null)
    setMode(next)
    if (modeHint === 'hidden') setModeHint('visible')
  }

  /** Saving also catches edits made just before React's layout effect has run. */
  async function refreshCurrentScene() {
    if (!sceneRef.current || isPencilEditing()) return
    const epoch = taskEpoch.current
    const at = pageRef.current
    const g = session.graph
    const current = sceneRef.current
    const layout = new StableLayout()
    layout.restore(board.snapshot())
    const [{ scene }, { withSceneOverlay }, l] = await Promise.all([
      import('./board/scene.ts'), import('./board/sceneOverlay.ts'), layout.layout(g),
    ])
    const merged = await withSceneOverlay(mergeScene(current, await scene(g, l, new Set(), LIGHT)), g, pagesRef.current[at]?.base, pagesRef.current[at]?.steps?.[0]?.scene)
    if (epoch !== taskEpoch.current || at !== pageRef.current || isPencilEditing()) return
    if (g !== session.graph || current !== sceneRef.current) return refreshCurrentScene()
    sceneRef.current = merged
    setSceneOverlay(merged.overlay)
    recordBoardStep(merged)
  }

  async function refreshOtherScenes() {
    const epoch = taskEpoch.current
    await Promise.all(pagesRef.current.map(async (page, at) => {
      if (at === pageRef.current || !page.scene) return
      const layout = new StableLayout()
      layout.restore(page.layout)
      const [{ scene }, { withSceneOverlay }, l] = await Promise.all([
        import('./board/scene.ts'), import('./board/sceneOverlay.ts'), layout.layout(page.graph),
      ])
      const state = await withSceneOverlay(mergeScene(page.scene, await scene(page.graph, l, new Set(), LIGHT)), page.graph, page.base, page.steps?.[0]?.scene)
      if (epoch !== taskEpoch.current || pagesRef.current[at] !== page || at === pageRef.current) return
      const steps = page.steps?.map((s, i, all) => i === all.length - 1 ? { ...s, scene: state } : s)
      pagesRef.current[at] = { ...page, layout: layout.snapshot(), scene: state, steps }
    }))
  }

  // 图变了就重新排版、重画。排版有记忆：旧内容原地不动，新内容找空地（见 board/layout.ts）
  useEffect(() => {
    pencilView.current = { had: false, tidy, reading: false }
    setPencilReading(false)
  }, [meeting.id, page, api])
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
      if (!sceneRef.current && (mode !== 'pencil' || !api) && !replayRef.current) return
      // 只有铅笔模式用得到；跟着 Excalidraw 一起按需加载
      const [{ scene }, { withSceneOverlay }] = await Promise.all([import('./board/scene.ts'), import('./board/sceneOverlay.ts')])
      if (stale) return
      // Excalidraw 的深色模式是给整块画布加反色滤镜，所以永远按浅色画，深色由它反过来
      const look = LIGHT
      const r = replayRef.current
      const saved = r ? (r.at === r.total ? r.saved.scene : replaySceneTo(stepsRef.current, r.at)) : sceneRef.current
      const state = await withSceneOverlay(mergeScene(saved, await scene(graph, l, new Set(), look)), graph, pagesRef.current[pageRef.current]?.base, stepsRef.current[0]?.scene)
      if (stale || isPencilEditing() || (!r && graph !== session.graph)) return
      if (!r) {
        // onChange may have accepted a gesture while SVG generation was pending.
        if (sceneRef.current !== saved) return
        sceneRef.current = state
        recordBoardStep(state)
      }
      setSceneOverlay(state.overlay)
      displayedSceneRef.current = state
      if (mode !== 'pencil' || !api || livePencilApi.current !== api) return
      applyingPencilScene.current = true
      try {
        const displayed = withPageUnderlay(withInheritedInk(state, graph, pagesRef.current[pageRef.current]?.base, stepsRef.current[0]?.scene, true), graph, pagesRef.current[pageRef.current]?.underlay)
        if (Object.keys(displayed.files).length) api.addFiles(structuredClone(Object.values(displayed.files).filter((f) => f.dataURL.startsWith('data:'))))
        // Excalidraw mutates element objects during gestures. Its working scene must
        // never share the immutable comparison/history/save snapshot held by the app.
        api.updateScene({ elements: structuredClone(displayed.elements), appState: { viewBackgroundColor: PAPER_LIGHT } })
        pencilReady.current = true
        setPencilInitialized(true)
      } finally { applyingPencilScene.current = false }
      const seen = pencilView.current
      const refit = seen.tidy !== tidy
      const first = !seen.had || refit
      pencilView.current = { had: graph.nodes.length > 0 || sceneHasContent(state), tidy, reading: !refit && seen.reading }
      if (refit) setPencilReading(false)
      if (!graph.nodes.length && !sceneHasContent(state)) return
      if (!refit && (seen.reading || (seen.had && matchMedia('(prefers-reduced-motion: reduce)').matches))) {
        setPencilReading(true)
        return
      }
      const { x0, y0, x1, y1 } = sceneBounds(l.bounds, state.overlay)
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
  }, [graph, hot, api, inset, tidy, layoutRev, mode, replay?.at, page])

  useEffect(() => {
    save('boardcast.mode', mode)
    save('boardcast.messy', String(messy))
    save('boardcast.weight', String(weight))
    save('boardcast.englishWeight', String(englishWeight))
    save('boardcast.mic', micChoice)
    save('boardcast.source', voiceSource)
    save('boardcast.diarize', diarize ? 'on' : 'off')
    save('boardcast.glossary', glossary)
    save('boardcast.autoupdate', autoUpdate ? 'on' : 'off')
    save('boardcast.log', showLog ? 'open' : 'closed')
  }, [mode, messy, weight, englishWeight, micChoice, voiceSource, diarize, autoUpdate, glossary, showLog])

  // 只滚动字幕面板，避免嵌入官网时 scrollIntoView 连带滚动外层页面。
  // 每次定位使用新请求，相同原话再次被选中时也要滚到它、亮一下。
  const [jumpLine, setJumpLine] = useState<{ index: number } | null>(null)
  useEffect(() => { transcriptAnchor.current = null; resizingTranscript.current = false; followLatest(true); setReviewing(false); setReviewDialog(false) }, [meeting.id])
  useEffect(() => {
    const list = transcriptScroller()
    if (followTranscriptRef.current && jumpLine === null && list?.clientHeight) list.scrollTop = list.scrollHeight
  }, [lines, replay?.at, jumpLine, followTranscript, sidebarTab, showLog])
  useEffect(() => {
    if (jumpLine === null) return
    followLatest(false)
    const t = setTimeout(() => {
      const list = transcriptScroller()
      const line = lineList.current?.querySelector<HTMLElement>('.line.jump')
      if (list && line) list.scrollTop += line.getBoundingClientRect().top - list.getBoundingClientRect().top - (list.clientHeight - line.offsetHeight) / 2
    }, 60)
    const off = setTimeout(() => setJumpLine(null), 2500)
    return () => {
      clearTimeout(t)
      clearTimeout(off)
    }
  }, [jumpLine])

  function settleLines(batch: Utterance[], patch: (index: number) => Partial<Line>) {
    const next = settle(linesRef.current, batch, patch)
    linesRef.current = next
    setLines(next)
  }

  function submitBoardLine(updated: Line, retry = false, previous?: Line): boolean {
    const at = updated.page ?? 0
    const scope = meetingIdRef.current
    if (!pagesRef.current[at]) return false
    let worker = pageSessions.current.get(at)
    if (!worker) {
      const remember = (batch: Utterance[]) => {
        for (const { replaces: _previous, ...line } of batch) {
          const index = session.history.findIndex((u) => u.id === line.id)
          if (index < 0) session.history.push(line)
          else session.history[index] = line
        }
      }
      worker = new Session(session.llm, {
        beforeApply: async (signal) => {
          await waitForPencilEdit(signal, at)
          if (scope === meetingIdRef.current && !signal.aborted)
            worker!.graph = at === pageRef.current ? session.graph : pagesRef.current[at].graph
        },
        graph: (raw, why) => {
          if (meetingIdRef.current !== scope) return
          const current = pagesRef.current[at]
          const visible = at === pageRef.current
          const beforeScene = visible ? sceneRef.current : current.scene
          const candidate = why === 'ink' ? captureModelCandidates(raw, beforeScene) : beforeScene
          const g = protectGraphFields(raw, candidate)
          worker!.graph = g
          if (visible) {
            sceneRef.current = candidate
            session.graph = g
            if (why === 'ink') {
              editHistory.current.record({ graph: lastInk.current, layout: board.snapshot(), scene: beforeScene })
              lastInk.current = g
              setUndoTick((n) => n + 1)
            }
            if (!replayRef.current && !isPencilEditing()) setGraph(g)
          } else {
            const steps = current.steps ?? []
            const d = why === 'ink' ? diffGraph(replayTo(steps, steps.length), inkOnly(g)) : null
            pagesRef.current[at] = { ...current, graph: g, scene: candidate, thumb: pageThumb(g),
              steps: d ? [...steps, { t: Date.now(), line: linesRef.current.length, d, ...(candidate ? { scene: candidate } : {}) }] : steps }
            setThumbs((ts) => ts.map((t, i) => i === at ? pagesRef.current[at].thumb : t))
          }
        },
        mention: (ids) => {
          if (scope !== meetingIdRef.current || at !== pageRef.current) return
          setHot(new Set(ids))
          const epoch = taskEpoch.current
          if (ids.length) setTimeout(() => { if (epoch === taskEpoch.current && at === pageRef.current) setHot(new Set()) }, 3000)
        },
        llm: (r) => {
          if (meetingIdRef.current !== scope) return
          if (at === pageRef.current) fixes.current.round({
            at: Date.now(), page: at, seen: r.seen, context: r.context, said: r.said,
            terms: worker!.terms, ops: r.ops, drawn: worker!.graph,
          })
          remember(r.batch)
          settleLines(r.batch, (k) => ({ ms: r.ms, queueMs: r.queuedMs[k],
            ops: k === r.batch.length - 1 ? formatOps(r.ops) || '-' : '', via: r.via, fellBack: !!r.fellBack, failed: undefined, boardFailure: undefined }))
        },
        error: (error, batch) => {
          if (meetingIdRef.current !== scope) return
          console.error(error)
          setOnline(false)
          remember(batch)
          settleLines(batch, () => ({ ms: 0, ops: '', failed: true, boardFailure: error instanceof BoardModelUnavailable ? 'unavailable' : 'request' }))
        },
      })
      worker.history = linesRef.current.filter((l) => !l.manual && (l.page ?? 0) === at && (l.id === previous?.id || l.ms !== undefined && l.id !== updated.id))
        .map((l) => previous && l.id === previous.id ? previous : l)
      pageSessions.current.set(at, worker)
    }
    worker.graph = at === pageRef.current ? session.graph : pagesRef.current[at].graph
    worker.llm = session.llm
    worker.terms = termsRef.current
    const accepted = retry ? worker.retry(updated) : (worker.say(updated), true)
    void worker.drain().then(() => {
      if (pageSessions.current.get(at) === worker) pageSessions.current.delete(at)
    })
    return accepted
  }

  function retryLine(index: number) {
    if (transitionRef.current || replayRef.current) return
    const line = linesRef.current[index]
    if (!line?.failed || line.manual || line.ms === undefined) return
    const original = { ...line, id: line.id ?? crypto.randomUUID() }
    if (!line.id) {
      let occurrence = linesRef.current.slice(0, index).filter((l) => !l.id && l.text === line.text && l.speaker === line.speaker).length
      session.history = session.history.map((u) => !u.id && u.text === line.text && u.speaker === line.speaker && occurrence-- === 0 ? { ...u, id: original.id } : u)
    }
    const pending = { ...original, ms: undefined, ops: undefined, failed: undefined, boardFailure: undefined }
    linesRef.current = linesRef.current.map((l, i) => i === index ? pending : l)
    setLines(linesRef.current)
    if (!submitBoardLine({ ...pending, failed: true }, true)) {
      linesRef.current = linesRef.current.map((l, i) => i === index ? original : l)
      setLines(linesRef.current)
    }
  }

  function say(said: Utterance, heard?: HeardTiming, audio?: Line['audio']) {
    if (transitionRef.current && !audio) return
    setReviewing(false)
    // 先按术语表纠正（说的和打的都纠）：内部叫法常被听成、打成别的字
    const u = correctUtterance({ ...said, id: said.id ?? crypto.randomUUID() }, termsRef.current)
    const previous = u.id ? linesRef.current.find((l) => l.id === u.id) : undefined
    if (previous) {
      if ((u.revision ?? 0) <= (previous.revision ?? 0)) return
      const changed = previous.text !== u.text
      const patch = { text: u.text, rawText: u.rawText, revision: u.revision, heard, audio,
        ...(changed ? { ms: undefined, ops: undefined, failed: undefined, boardFailure: undefined } : {}) }
      const updated = { ...previous, ...patch }
      const revise = (ls: Line[]) => ls.map((l) => l.id === u.id ? { ...l, ...patch } : l)
      linesRef.current = revise(linesRef.current)
      setLines(revise)
      if (!changed) {
        pageSessions.current.get(previous.page ?? 0)?.say(updated)
        session.history = session.history.map((line) => line.id === updated.id ? { ...line, text: updated.text, rawText: updated.rawText, revision: updated.revision } : line)
        return
      }
      submitBoardLine(updated, false, previous)
      return
    }
    if ((u.revision ?? 0) > 0) return // Late results from a different meeting must not become new speech.
    // 整句就是翻页：不用问模型，也不写板书
    const nav = parseNav(u.text)
    if (nav) {
      const boards = pagesRef.current.map((p, i) => (i === pageRef.current ? session.graph : p.graph).underlay ?? {})
      const to = navTarget(nav, boards, pageRef.current)
      const line = { ...u, page: pageRef.current, ms: 0, heard, audio }
      linesRef.current = [...linesRef.current, line]
      setLines((ls) => [...ls, line])
      if (to < 0) setToast({ ok: false, text: t.app.noSuchPage })
      else goPage(to)
      return
    }
    // 板上圈着的东西（还在的）：随这句话告诉模型；圈用掉了就擦掉，圈选也收起来
    const pointing = latest(circlesRef.current.flatMap((c) => c.ids).filter((id) => session.graph.nodes.some((n) => n.id === id)))
    const v = pointing.length ? { ...u, pointing } : u
    const line = { ...v, page: pageRef.current, heard, audio }
    linesRef.current = [...linesRef.current, line]
    setLines((ls) => [...ls, line])
    submitBoardLine(line)
    if (circlesRef.current.length) clearCircles()
  }

  /** 第几张板上某个节点现在叫什么（会议记录里显示用；删掉了就显示编号） */
  const nameOn = (at: number, id: string) =>
    ((at === page ? graph : pagesRef.current[at]?.graph) ?? graph).nodes.find((n) => n.id === id)?.label ?? id

  // —— 圈着说（#48）：打开圈选，在板上用虚线画一圈，圈住的东西就是下一句话里的「这里」「这个」「这俩」 ——
  // 只认画出来的圈：鼠标停一下、点选都不算，免得误触；手机、电子屏上用手指画
  const [lasso, setLasso] = useState(false)
  const circleButton = useRef<HTMLButtonElement>(null)
  type Circle = { loop: [number, number][]; ids: string[]; grouped: boolean }
  const [circles, setCircles] = useState<Circle[]>([])
  const circlesRef = useRef(circles)
  circlesRef.current = circles
  /** 画圈之前的样子，撤销一步一步退回去 */
  const circlePast = useRef<Circle[][]>([])
  /** 叠在一起的几圈（`merged`）已经合成了这一个大圈 */
  function onCircle(loop: [number, number][], ids: string[], merged: number[], grouped: boolean) {
    if (!ids.length) {
      setToast({ ok: true, text: t.app.circleEmpty })
      return
    }
    const cs = circlesRef.current
    circlePast.current.push(cs)
    // 最多留三圈：「从这个连到那个」用两圈就够了
    circlesRef.current = [...cs.filter((_, i) => !merged.includes(i)), { loop, ids, grouped }].slice(-3)
    setCircles(circlesRef.current)
  }
  function undoCircle() {
    const prev = circlePast.current.pop()
    if (prev) {
      circlesRef.current = prev
      setCircles(prev)
    }
  }
  function clearCircles() {
    if (document.activeElement?.closest('.lasso-hint')) circleButton.current?.focus()
    circlePast.current = []
    circlesRef.current = []
    setCircles([])
    setLasso(false)
  }
  function toggleLasso() {
    if (lasso) clearCircles()
    else {
      setLasso(true)
      setSelection(null)
    }
  }
  // 换了板、换了会、回放、换到铅笔模式：圈都不算了
  const replaying = !!replay
  useEffect(clearCircles, [page, meeting.id, replaying, mode])
  const circleIds = latest(circles.flatMap((c) => c.ids).filter((id) => graph.nodes.some((n) => n.id === id)), Infinity)
  const circleTargets = circleIds.slice(-MAX_POINTED)
  /** 圈着的东西叫什么（提示条里显示，最多列三样） */
  const circledNames = () => {
    const names = circleTargets.slice(0, 3).map((id) => {
      const name = nameOn(page, id)
      return name.length > 16 ? `${name.slice(0, 16)}…` : name
    })
    return circleTargets.length > 3 ? t.app.andMore(names.join(t.app.listSep), circleTargets.length) : names.join(t.app.listSep)
  }

  // —— 语音输入（桌面版）：外壳负责收音、断句、识别，这里只接结果 ——
  const [listening, setListening] = useState(false)
  const listeningRef = useRef(listening)
  listeningRef.current = listening
  const acceptSpeech = useRef(false)
  const [micPending, setMicPending] = useState<'starting' | 'stopping' | null>(null)
  const micRequest = useRef<{ stop: boolean } | null>(null)
  const micOperation = useRef<Promise<boolean> | null>(null)
  const [level, setLevel] = useState(0)
  const [partial, setPartial] = useState('')
  const [mic, setMic] = useState<string | undefined>()
  const [voiceErr, setVoiceErr] = useState<string | null>(null)
  const showVoiceError = useCallback((error: unknown) => {
    const text = errText(error)
    recordError('voice', text)
    setVoiceErr(text)
    if (appleAssetIssue(text) || text === 'APPLE_AUTO_LANGUAGE_UNCERTAIN') setShowModels('speech')
  }, [])
  // 监听器只注册一次，用 ref 拿最新的 say 和说话人
  const sayRef = useRef(say)
  sayRef.current = say
  const speakerRef = useRef(speaker)
  speakerRef.current = speaker
  useEffect(() => {
    if (!isDesktop) return
    const offs: Promise<() => void>[] = [
      on<Heard>('voice-final', (h) => {
        if (!acceptSpeech.current || !h.text.trim()) return
        setPartial('')
        const previous = linesRef.current.find((l) => l.id === h.id)
        if (h.revision > 0 && !previous) return
        // 有声纹就认人；没有（句子太短、没带声纹模型、关掉了）就用当前的说话人，也就是上一个说话的人
        let who = previous?.speaker ?? (speakerRef.current.trim() || undefined)
        if (!previous && diarizeRef.current && h.voiceprint?.length) {
          const taken = linesRef.current.flatMap((l) => (l.speaker ? [l.speaker] : []))
          const r = identify(voicesRef.current, h.voiceprint, taken)
          voicesRef.current = r.voices
          who = r.name
          speakerRef.current = r.name
          setSpeaker(r.name)
        }
        sayRef.current({ id: h.id, revision: h.revision, speaker: who, text: h.text },
          h.timing ? { ...h.timing, reviewMs: h.review?.asrMs } : undefined,
          { start: h.start, end: h.end, sources: h.sources, quality: h.quality, review: h.review })
      }),
      on<string>('voice-partial', (text) => { if (acceptSpeech.current) setPartial(text) }),
      on<number>('voice-level', setLevel),
      on<VoiceStatus>('voice-status', (s) => {
        listeningRef.current = s.listening
        if (s.listening && !transitionRef.current) acceptSpeech.current = true
        setListening(s.listening)
        setMic(s.device)
        if (!s.listening) {
          setLevel(0)
          setPartial('')
        }
        if (s.message) showVoiceError(s.message)
      }),
      on<string>('voice-error', showVoiceError),
      on('voice-models-changed', () => setVoiceErr(null)),
    ]
    return () => offs.forEach((p) => void p.then((off) => off()))
  }, [showVoiceError])

  function stopMic(): Promise<boolean> {
    if (micRequest.current) {
      micRequest.current.stop = true
      setMicPending('stopping')
      return micOperation.current ?? Promise.resolve(false)
    }
    const request = { stop: true }
    micRequest.current = request
    setMicPending('stopping')
    return micOperation.current = (async () => {
      try {
        await voice.stop()
        // Let final transcription events delivered with stop settle before the save snapshot.
        await new Promise((resolve) => setTimeout(resolve, 0))
        acceptSpeech.current = false
        listeningRef.current = false
        setListening(false)
        return true
      } catch (e) {
        showVoiceError(e)
        return false
      } finally {
        if (micRequest.current === request) micRequest.current = null
        setMicPending(null)
      }
    })()
  }

  function startMic(device = micChoice, source = voiceSource, restart = false): Promise<boolean> {
    if (micRequest.current || transitionRef.current) return Promise.resolve(false)
    setReviewing(false)
    setVoiceErr(null)
    const request = { stop: false }
    micRequest.current = request
    setMicPending('starting')
    return micOperation.current = (async () => { try {
      if (!restart) {
        const s = await voice.status()
        if (request.stop) return true
        if (!s.installed) {
          setVoiceErr('Download the speech recognition model first')
          setShowModels('speech')
          return false
        }
      }
      acceptSpeech.current = true
      await startVoice(voice, request, device || undefined, source, restart)
      if (request.stop) {
        await new Promise((resolve) => setTimeout(resolve, 0))
        acceptSpeech.current = false
      }
      return true
    } catch (e) {
      acceptSpeech.current = false
      showVoiceError(e)
      return false
    } finally {
      if (micRequest.current === request) micRequest.current = null
      setMicPending(null)
    } })()
  }

  async function toggleMic() {
    setVoiceErr(null)
    if (listening || micRequest.current) return stopMic()
    return startMic()
  }

  function changeAudio(device: string, source: VoiceSource) {
    if (micRequest.current || (device === micChoice && source === voiceSource)) return
    setMicChoice(device)
    setVoiceSource(source)
    if (listening) void startMic(device, source, true)
  }

  const currentSummarySource = useMemo(() => summaryFingerprint(pagesRef.current.map((p, i) => i === pageRef.current ? session.graph : p.graph), lines), [graph, lines, page, session])

  function writeSummary(text: string, source: string | undefined) {
    summaryRef.current = text
    summarySourceRef.current = source
    setSummary(text)
    setSummarySource(source)
  }

  function editSummary(text: string) {
    if (replayRef.current || transitionRef.current) return
    summaryEditRevision.current++
    setClearedSummary(null)
    writeSummary(text, summarySourceRef.current ?? currentSummarySource)
  }

  function clearSummary() {
    if (replayRef.current || transitionRef.current) return
    summaryEditRevision.current++
    setClearedSummary({ text: summaryRef.current, source: summarySourceRef.current })
    writeSummary('', undefined)
  }

  function undoClearSummary() {
    if (replayRef.current || transitionRef.current) return
    if (!clearedSummary) return
    summaryEditRevision.current++
    writeSummary(clearedSummary.text, clearedSummary.source)
    setClearedSummary(null)
  }

  function useGeneratedSummary() {
    if (replayRef.current || transitionRef.current) return
    if (!summaryCandidate) return
    summaryEditRevision.current++
    writeSummary(summaryCandidate.text, summaryCandidate.source)
    setSummaryCandidate(null)
    setClearedSummary(null)
  }

  /** Freeze inputs; an existing or concurrently edited draft requires an explicit replacement. */
  async function makeSummary() {
    if (transitionRef.current || replayRef.current) return
    if (summingFor.current) {
      summingFor.current.abort()
      return
    }
    if (summaryCandidate) return
    const ctl = new AbortController()
    const epoch = taskEpoch.current
    const editRevision = summaryEditRevision.current
    const hadSummary = !!summaryRef.current.trim()
    const boards = structuredClone(pagesRef.current.map((p, i) => i === pageRef.current ? session.graph : p.graph))
    const said = structuredClone(linesRef.current.filter((l) => !l.manual))
    const source = summaryFingerprint(boards, said)
    summingFor.current = ctl
    setSumming(t.app.summing)
    try {
      const r = await summarize(session.llm, boards, said, ctl.signal, (done, total) => {
        if (epoch === taskEpoch.current && !ctl.signal.aborted) setSumming(done < total - 1 ? t.app.summingPart(done + 1, total - 1) : t.app.summingMerge)
      })
      if (ctl.signal.aborted || epoch !== taskEpoch.current) return
      if (hadSummary || summaryRef.current.trim() || summaryEditRevision.current !== editRevision) setSummaryCandidate({ text: r.text, source })
      else writeSummary(r.text, source)
      if (r.fellBack) setToast({ ok: true, text: t.app.summaryFellBack })
    } catch (e) {
      if (!ctl.signal.aborted && epoch === taskEpoch.current) setToast({ ok: false, text: t.app.summaryFailed(errText(e)) })
    } finally {
      if (summingFor.current === ctl && epoch === taskEpoch.current) {
        summingFor.current = null
        setSumming(null)
      }
    }
  }

  async function exportAs(kind: ExportKind) {
    if (exportingRef.current) return
    const epoch = taskEpoch.current
    exportingRef.current = true
    setExporting(true)
    setExportMenu(false)
    setToast({ ok: true, text: t.app.exportPreparing, pending: true })
    let saving = false
    try {
      commitPencilEdit()
      const ink = document.querySelector('.ink')
      const paper = mode === 'ink' && ink ? getComputedStyle(ink).backgroundColor : PAPER_LIGHT
      const playing = replayRef.current
      const frozen: Parameters<typeof import('./exporter.ts')['build']>[1] = {
        graph: playing ? replayTo(stepsRef.current, playing.at) : session.graph,
        scene: playing ? (playing.at === playing.total ? playing.saved.scene : replaySceneTo(stepsRef.current, playing.at)) : sceneRef.current,
        layout: board.snapshot(),
        lines: linesRef.current,
        title: meetingRef.current.title,
        summary: summaryRef.current,
        underlay: pagesRef.current[pageRef.current]?.underlay,
        base: pagesRef.current[pageRef.current]?.base,
        baseScene: stepsRef.current[0]?.scene ?? undefined,
        messy: messyLater / 100, weight: weightLater / 100, englishWeight: englishWeightLater / 100,
        pages: pagesRef.current.length > 1 ? pagesRef.current.map((p, i) => (i === pageRef.current ? session.graph : p.graph)) : undefined,
        replay:
          kind === 'html' || kind === 'pdf' || kind === 'md'
            ? {
                title: meetingRef.current.title,
                createdAt: meetingRef.current.createdAt,
                lang: currentLang(),
                summary: summaryRef.current || undefined,
                messy: messyLater / 100,
                weight: weightLater / 100,
                englishWeight: englishWeightLater / 100,
                lines: linesRef.current,
                pages: pagesRef.current.map((p, i) => {
                  const cur = i === pageRef.current ? capture() : p
                  return { graph: cur.graph, steps: cur.steps, underlay: cur.underlay, layout: cur.layout, base: cur.base, scene: cur.scene }
                }),
              }
            : undefined,
        ...(kind === 'anim'
          ? { steps: stepsRef.current }
          : {}),
        mode, api: null, look: LIGHT, paper,
      }
      // Freeze before loading the exporter: changing a page or mode cannot change this file.
      const payload = structuredClone(frozen)
      const { build, saveFile } = await import('./exporter.ts')
      setToast({ ok: true, text: t.app.exportGenerating, pending: true })
      const { name, blob } = await build(kind, payload)
      saving = true
      setToast({ ok: true, text: isDesktop ? t.app.exportWaiting : t.app.exportDownloading, pending: true })
      const where = await saveFile(name, blob)
      if (where) setToast({ ok: true, text: isDesktop ? t.app.saved(where) : t.app.downloaded(where), path: isDesktop ? where : undefined })
      else setToast(null)
    } catch (e) {
      const error = errText(e)
      const reason = saving && storageReason(error)
      setToast({ ok: false, text: reason ? `${t.app.exportStorageFailed[reason]} ${error}` : saving ? t.app.exportSaveFailed(error) : t.app.exportFailed(error), retryExport: kind })
    } finally {
      exportingRef.current = false
      exportFocusEpoch.current = epoch
      setExporting(false)
    }
  }

  async function openExport(reveal: boolean) {
    if (!toast?.path || openingExportRef.current) return
    const source = toast
    openingExportRef.current = true
    setOpeningExport(true)
    try {
      await (reveal ? exportedFiles.reveal(toast.path) : exportedFiles.open(toast.path))
      setToast((current) => current === source ? { ...source, ok: true, text: reveal ? t.app.exportRevealRequested(source.path!) : t.app.exportOpenRequested(source.path!) } : current)
    } catch (error) {
      setToast((current) => current === source ? { ...source, ok: false, text: t.app.exportOpenFailed(source.path!, errText(error)) } : current)
    } finally {
      openingExportRef.current = false
      setOpeningExport(false)
    }
  }

  /** 所有会议里的纠错样本：存着的，加上这场会里还没来得及存的 */
  async function allFixes() {
    const current = { id: meeting.id, corrections: structuredClone(fixes.current.list) }
    const saved = await meetings.corrections()
    return [current, ...saved.filter((m) => m.id !== current.id)]
  }

  /** 导出纠错样本：评测用例（eval/cases.json 的格式）或者偏好数据（JSONL） */
  async function exportFixes(kind: 'cases' | 'pairs'): Promise<string | null> {
    const all = await allFixes()
    const d = new Date()
    const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
    const { saveFile } = await import('./exporter.ts')
    return kind === 'cases'
      ? saveFile(`boardcast-corrections-${day}.cases.json`, new Blob([casesFile(all)], { type: 'application/json' }))
      : saveFile(`boardcast-corrections-${day}.dpo.jsonl`, new Blob([pairsFile(all)], { type: 'application/x-ndjson' }))
  }

  function cancelMeetingWork() {
    commitPencilEdit()
    // A completed preview survives a failed save; a reader never outlives its meeting.
    if (importWork.current?.state.status !== 'ready') cancelImport()
    taskEpoch.current++
    if (autosave.current) clearTimeout(autosave.current)
    session.cancel()
    for (const [at, worker] of pageSessions.current) {
      worker.graph = at === pageRef.current ? session.graph : pagesRef.current[at].graph
      worker.cancel()
      if (at === pageRef.current) session.graph = worker.graph
      else pagesRef.current[at] = { ...pagesRef.current[at], graph: worker.graph }
    }
    pageSessions.current.clear()
    summingFor.current?.abort()
    summingFor.current = null
    setSumming(null)
    setPlaying(false)
    setTidying(false)
    // An idle cancellation must preserve the archive's saved model context.
    if (linesRef.current.some((line) => line.ms === undefined)) {
      const kept = linesRef.current.map((line) => line.ms === undefined ? { ...line, ms: 0, failed: true, boardFailure: 'cancelled' as const, ops: '' } : line)
      linesRef.current = kept
      setLines(kept)
      session.history = kept.filter((line) => !line.manual)
    }
    setGraph(session.graph)
  }

  async function switchMeeting(change: () => void | Promise<void>, { keepCurrent = true, confirm = true }: { keepCurrent?: boolean; confirm?: boolean } = {}): Promise<boolean> {
    if (transitionRef.current || pendingSwitch.current) return false
    transitionError.current = null
    if (confirm && isDesktop && (listeningRef.current || micRequest.current)) {
      const trigger = document.activeElement
      const proceed = await new Promise<boolean>((resolve) => {
        pendingSwitch.current = resolve
        setConfirmSwitch(true)
      })
      pendingSwitch.current = null
      setConfirmSwitch(false)
      if (!proceed) {
        // The caller may disable its trigger while waiting; restore after it becomes available again.
        requestAnimationFrame(() => { if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus() })
        return false
      }
    }
    transitionRef.current = true
    setTransitionBusy(true)
    try {
      exitReplay()
      if (isDesktop) await voice.inputCheck.stop()
      if (isDesktop && (listeningRef.current || micRequest.current) && !await stopMic()) {
        transitionError.current = t.review.stopFailed
        return false
      }
      cancelMeetingWork()
      if (keepCurrent && !await retrySave()) {
        transitionError.current = writer.status.error ?? t.workbench.saveFailed
        return false
      }
      await change()
      return true
    } catch (error) {
      transitionError.current = errText(error)
      setToast({ ok: false, text: transitionError.current })
      return false
    } finally {
      transitionRef.current = false
      setTransitionBusy(false)
    }
  }

  async function finishForReview(waitForBoard: boolean) {
    if (transitionRef.current) return
    transitionRef.current = true
    setTransitionBusy(true)
    setReviewError(null)
    try {
      setReviewStage('stopping')
      if (isDesktop) await voice.inputCheck.stop()
      if (isDesktop && (listeningRef.current || micRequest.current) && !await stopMic()) throw new Error(t.review.stopFailed)
      if (waitForBoard && linesRef.current.some((line) => line.ms === undefined)) {
        setReviewStage('waiting')
        await Promise.race([
          Promise.all([session, ...pageSessions.current.values()].map((active) => active.drain())),
          new Promise<void>((resolve) => { finishWaiting.current = resolve }),
        ])
      }
      cancelMeetingWork()
      setReviewStage('saving')
      if (!await retrySave()) throw new Error(writer.status.error ?? t.workbench.saveFailed)
      setReviewing(true)
      setSidebarTab('summary')
      setShowLog(true)
      setReviewDialog(false)
    } catch (error) {
      setReviewError(errText(error))
    } finally {
      finishWaiting.current = null
      setReviewStage(null)
      transitionRef.current = false
      setTransitionBusy(false)
    }
  }

  /** Only run after the old meeting has been saved, or explicitly deleted. */
  function resetNow() {
    writer.reset()
    setSidebarTab('transcript')
    restoreScene(undefined)
    missingAssets.current = undefined
    setAssetIssues([])
    cancelImport()
    board.reset()
    session.graph = emptyGraph()
    session.history = []
    editHistory.current.clear()
    fixes.current.load([])
    lastInk.current = session.graph
    setSelection(null)
    setGraph(session.graph)
    setLines([])
    linesRef.current = []
    summingFor.current?.abort()
    setSummary('')
    summaryRef.current = ''
    setSummarySource(undefined)
    summarySourceRef.current = undefined
    setSummaryCandidate(null)
    setClearedSummary(null)
    summaryEditRevision.current++
    voicesRef.current = []
    setRenamingWho(null)
    setShowMeetingTitle(false)
    stepsRef.current = []
    recorded.current = emptyGraph()
    recordedScene.current = undefined
    setReplay(null)
    replayRef.current = null
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
    meetingIdRef.current = id
    meetingRef.current = { id, title: t.app.untitled, createdAt: Date.now() }
    setMeeting(meetingRef.current)
    save('boardcast.meeting', id)
  }

  function reset(keepCurrent = true): Promise<boolean> {
    return switchMeeting(resetNow, { keepCurrent, confirm: keepCurrent })
  }

  function deleteCurrentMeeting(): Promise<boolean> {
    return switchMeeting(async () => {
      await meetings.remove(meetingIdRef.current)
      resetNow()
    }, { keepCurrent: false, confirm: false })
  }

  async function saveBeforeMaintenance(): Promise<void> {
    if (!await switchMeeting(() => undefined, { confirm: false })) throw new Error(transitionError.current ?? t.app.openFailed)
  }

  /** 手动改了板书：记一步撤销，记进会议记录（模型下一句看到的就是改过的板） */
  function applyEdit(next: Graph, note: string) {
    if (transitionRef.current || replayRef.current) return
    if (next === session.graph) return
    commitPencilEdit()
    editHistory.current.record(now())
    sceneRef.current = applySemanticEdits(session.graph, next, sceneRef.current)
    session.graph = next
    lastInk.current = next
    setGraph(next)
    setUndoTick((t) => t + 1)
    // 改的是模型刚画的：这一轮记成纠错样本
    const fix = fixes.current.edit(next, note) || undefined
    setLines((ls) => [...ls, { text: note, manual: true, ms: 0, page: pageRef.current, fix }])
    setSelection((sel) => (sel && exists(next, sel) ? sel : null))
  }

  function stepTo(snap: Snap | null, note: string) {
    if (!snap || transitionRef.current) return
    restoreScene(snap.scene)
    board.restore(snap.layout)
    const g = snap.graph
    session.graph = g
    lastInk.current = g
    setGraph(g)
    setLayoutRev((n) => n + 1)
    setUndoTick((t) => t + 1)
    setSelection(null)
    // 撤销模型刚画的一轮，也是在说它画错了
    const fix = fixes.current.edit(g, note) || undefined
    // 也记进会议记录：不然记录里还写着「删掉了……」，看着像没恢复
    setLines((ls) => [...ls, { text: note, manual: true, ms: 0, page: pageRef.current, fix }])
  }
  const now = (): Snap => ({ graph: session.graph, layout: board.snapshot(), scene: sceneRef.current })
  // 板上画着圈时，撤销先撤圈（不进板书的撤销记录）
  const undo = () => (circlePast.current.length ? undoCircle() : stepTo(editHistory.current.undo(now()), t.app.undoNote))
  const redo = () => stepTo(editHistory.current.redo(now()), t.app.redoNote)

  async function tidyUp() {
    if (transitionRef.current || tidying || replayRef.current || linesRef.current.some((line) => line.ms === undefined) || !session.graph.nodes.length) return
    const epoch = taskEpoch.current
    setTidying(true)
    setToast(null)
    try {
      const graph = session.graph
      const page = pageRef.current
      await board.layout(graph)
      commitPencilEdit()
      const before = now()
      const { beautify } = await import('./board/beautify.ts')
      const layout = await beautify(graph, before.layout)
      if (epoch !== taskEpoch.current) return
      if (session.graph !== graph || pageRef.current !== page || replayRef.current || JSON.stringify(sceneContent(before.scene)) !== JSON.stringify(sceneContent(sceneRef.current)) || JSON.stringify(board.snapshot()) !== JSON.stringify(before.layout)) {
        setToast({ ok: false, text: t.app.tidyChanged })
        return
      }
      editHistory.current.record(before)
      board.restore(layout)
      setSelection(null)
      setUndoTick((n) => n + 1)
      setTidy((n) => n + 1)
      setLines((ls) => [...ls, { text: t.app.tidiedNote, manual: true, ms: 0, page }])
    } catch (e) {
      if (epoch !== taskEpoch.current) return
      recordError('beautify', e)
      setToast({ ok: false, text: `${t.app.tidyFailed}: ${errText(e)}` })
    } finally {
      if (epoch === taskEpoch.current) setTidying(false)
    }
  }

  /** 拖动：把新位置写进排版记忆，记一步撤销 */
  function moveSelection(sel: Selection, dx: number, dy: number) {
    if (sel.kind !== 'node' || transitionRef.current) return
    const before = now()
    if (!board.move(sel.id, dx, dy)) return
    sceneRef.current = moveSceneOwner(sceneRef.current, sel.id, dx, dy)
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

  function locateContent(target: { selection?: Selection; box?: Box }) {
    changeMode('ink')
    setSelection(target.selection ?? null)
    setRenaming(null)
    selectedAt.current = undefined
    setMarkUnit('block')
    setFocusContent(target)
    if (compact) setShowLog(false)
  }

  function updateFreeObject(id: string, name: string | null) {
    if (transitionRef.current || replayRef.current) return
    commitPencilEdit()
    const state = sceneRef.current
    if (!state) return
    const next = editFreeObject(state, id, name)
    if (next === state) return
    editHistory.current.record(now())
    restoreScene(next)
    setUndoTick((n) => n + 1)
    writer.changed()
    recordBoardStep(next)
  }

  const selectedSceneAction = selection ? sceneOwnerAction(sceneRef.current, selection) : undefined

  function adoptSelectedScene() {
    if (!selection || transitionRef.current || replayRef.current) return
    commitPencilEdit()
    try {
      const result = adoptSceneOwner(session.graph, sceneRef.current, selection)
      if (!result.changed) return
      editHistory.current.record(now())
      restoreScene(result.scene)
      session.graph = result.graph
      lastInk.current = result.graph
      setGraph(result.graph)
      setLayoutRev((n) => n + 1)
      setUndoTick((n) => n + 1)
      recordBoardStep(result.scene)
    } catch (error) {
      setToast({ ok: false, text: errText(error) })
    }
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
    if (transitionRef.current || replayRef.current) return
    to = to.trim()
    if (!to || to === from) return
    commitPencilEdit()
    setRenamingWho(null)
    linesRef.current = linesRef.current.map((line) => line.speaker === from ? { ...line, speaker: to } : line)
    setLines(linesRef.current)
    session.renameSpeaker(from, to)
    for (const revision of pageSessions.current.values()) revision.renameSpeaker(from, to)
    voicesRef.current = renameVoice(voicesRef.current, from, to)
    if (speakerRef.current.trim() === from) { speakerRef.current = to; setSpeaker(to) }
    lastInk.current = renameBy(lastInk.current, from, to)
    recorded.current = renameBy(recorded.current, from, to)
    sceneRef.current = renameSceneSpeaker(sceneRef.current, from, to)
    recordedScene.current = renameSceneSpeaker(recordedScene.current, from, to)
    stepsRef.current = renameByInSteps(stepsRef.current, from, to)
    pagesRef.current = pagesRef.current.map((p) => ({ ...p, graph: renameBy(p.graph, from, to),
      ...(p.base ? { base: renameBy(p.base, from, to) } : {}), scene: renameSceneSpeaker(p.scene, from, to),
      ...(p.steps ? { steps: renameByInSteps(p.steps, from, to) } : {}) }))
    editHistory.current.map((snap) => ({ ...snap, graph: renameBy(snap.graph, from, to), scene: renameSceneSpeaker(snap.scene, from, to) }))
    setTraceWho(null)
    setGraph(session.graph)
    writer.changed()
  }

  /** 选中的那条板书是谁提的，和它的名字（找会议记录里那句话用） */
  const raisedBy = (sel: Selection | null): { who: string; names: string[] } | null => {
    if (!sel) return null
    const g = session.graph
    if (sel.kind === 'node') {
      const n = g.nodes.find((x) => x.id === sel.id)
      return n?.by ? { who: n.by, names: [n.label, ...n.aliases] } : null
    }
    const e = g.edges.find((x) => x.from === sel.from && x.to === sel.to)
    const name = (id: string) => g.nodes.find((x) => x.id === id)?.label ?? ''
    return e?.by ? { who: e.by, names: [name(e.from), name(e.to), e.label ?? ''] } : null
  }

  /** 跳到会议记录里提出这条板书的那句话 */
  function jumpToRaised(sel: Selection) {
    const r = raisedBy(sel)
    if (!r) return
    const i = saidAt(linesRef.current, r.who, r.names, pageRef.current, firstSeen(stepsRef.current, sel))
    if (i === undefined) return
    setShowLog(true)
    setSidebarTab('transcript')
    setJumpLine({ index: i })
  }

  function finishRename() {
    // End native input editing before removing the focused field.
    document.querySelector<HTMLElement>('.ink')?.focus()
    setRenaming(null)
    requestAnimationFrame(() => inkRenameButton.current?.focus())
  }

  function commitRename() {
    if (!selection || renaming === null || selection.kind === 'node' && !renaming.trim()) return
    const before = describe(session.graph, selection, t.doc)
    const next = renameSel(session.graph, selection, renaming)
    finishRename()
    if (next !== session.graph) applyEdit(next, t.app.renamedNote(before, renaming.trim()))
  }

  // 键盘：撤销 / 重做 / 删除 / 取消选中。焦点在输入框里时不管；铅笔模式交给 Excalidraw 自己处理
  const run = (a: Action) => {
    const replaying = !!replayRef.current
    if (a === 'help') setShowKeys((v) => !v)
    else if (a === 'replay') (replaying ? exitReplay() : startReplay())
    else if (replaying) return
    else if (a === 'import') fileInput.current?.click()
    else if (a === 'summary') { setSidebarTab('summary'); setShowLog(true); void makeSummary() }
    else if (transitionRef.current) return
    else if (a === 'newPage') newPage()
    else goPage(pageRef.current + (a === 'nextPage' ? 1 : -1))
  }
  const keys = useRef({ undo, redo, deleteSelection, toggleReplay, seekReplay, exitReplay, run, toggleMic, stopMic, listening })
  keys.current = { undo, redo, deleteSelection, toggleReplay, seekReplay, exitReplay, run, toggleMic, stopMic, listening }
  // 按住空格说话：只有这次是按住才开的，松开才停
  const held = useRef(false)
  useEffect(() => {
    let stopTimer: ReturnType<typeof setTimeout> | undefined
    const blocked = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement
      // 键盘移到按钮上时，空格还是点按钮；鼠标点过的按钮留着焦点，不算
      return !!t.closest('input:not([type=range]), textarea, select, [contenteditable]') || !!t.closest('.modal') || (t.tagName === 'BUTTON' && t.matches(':focus-visible'))
    }
    const down = (e: KeyboardEvent) => {
      if (e.isComposing || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return
      if (e.code !== 'Space' || !isDesktop || mode !== 'ink' || replayRef.current || document.querySelector('.modal') || blocked(e)) return
      e.preventDefault()
      if (e.repeat || held.current) return
      if (stopTimer) {
        clearTimeout(stopTimer)
        stopTimer = undefined
        held.current = true
        return
      }
      if (keys.current.listening || micRequest.current) return
      held.current = true
      void keys.current.toggleMic()
    }
    const up = (e: KeyboardEvent) => {
      if (e.code !== 'Space' || !held.current) return
      e.preventDefault()
      held.current = false
      // 多收一小会儿，免得切掉最后一个字；停下时引擎会把没说完的这句收尾
      stopTimer = setTimeout(() => {
        stopTimer = undefined
        void keys.current.stopMic()
      }, 300)
    }
    const stopHeld = () => {
      const shouldStop = held.current || stopTimer !== undefined
      held.current = false
      if (stopTimer) clearTimeout(stopTimer)
      stopTimer = undefined
      if (shouldStop) void keys.current.stopMic()
    }
    const onHidden = () => { if (document.hidden) stopHeld() }
    addEventListener('keydown', down)
    addEventListener('keyup', up)
    addEventListener('blur', stopHeld)
    document.addEventListener('visibilitychange', onHidden)
    return () => {
      removeEventListener('keydown', down)
      removeEventListener('keyup', up)
      removeEventListener('blur', stopHeld)
      document.removeEventListener('visibilitychange', onHidden)
      stopHeld()
    }
  }, [mode])
  // 全局快捷键、托盘、boardcast --toggle-mic
  useEffect(() => {
    if (!isDesktop || !restored) return
    let disposed = false
    const applyToggle = async () => {
      if (disposed) return
      const recordingShortcut = document.hasFocus() && document.activeElement?.matches('[data-shortcut-recorder]')
      if (!await shell.takeToggle() || disposed || recordingShortcut) return
      if (!await keys.current.toggleMic()) await shell.showWindow()
    }
    const off = shell.onToggle(() => { void applyToggle().catch((error) => recordError('shortcut', errText(error))) })
    void off.then(applyToggle).catch((error) => recordError('shortcut', errText(error)))
    return () => { disposed = true; void off.then((f) => f()).catch(() => undefined) }
  }, [restored])
  const closingRequest = useRef<number | null>(null)
  const checkingNativeUpdate = useRef(false)
  const nativeHandlers = useRef({ menu: (_action: ShellMenuAction) => {}, exit: async (_request: ShellExitRequest) => {} })
  nativeHandlers.current = {
    menu: (action) => {
      if (transitionRef.current) return
      if (action === 'undo' || action === 'redo' || action === 'select-all') {
        const target = document.activeElement
        if (target instanceof HTMLElement && target.closest('input:not([type=range]), textarea, [contenteditable]')) {
          // Keep editing commands in the focused field, including fields inside dialogs.
          document.execCommand(action === 'select-all' ? 'selectAll' : action)
          return
        }
        if (document.querySelector('dialog[open]')) return
        if (mode === 'pencil') {
          const mac = /Mac|iPhone|iPad|iPod/.test(navigator.platform)
          document.querySelector('.excalidraw')?.dispatchEvent(new KeyboardEvent('keydown', {
            key: action === 'select-all' ? 'a' : 'z', code: action === 'select-all' ? 'KeyA' : 'KeyZ',
            ctrlKey: !mac, metaKey: mac, shiftKey: action === 'redo', bubbles: true, cancelable: true,
          }))
        } else if (action !== 'select-all') keys.current[action]()
        return
      }
      if (document.querySelector('dialog[open]')) return
      if (action === 'settings') setShowSettings(true)
      else if (action === 'new-meeting') void reset().then((ok) => { if (ok) setShowPreparation(true) })
      else if (action === 'meetings') setShowMeetings(true)
      else if (action === 'import') keys.current.run('import')
      else if (action === 'export') { if (canExportMeeting && !exportingRef.current) { setImportMenu(false); setExportMenu(true) } }
      else if (action === 'keys') setShowKeys(true)
      else if (action === 'diagnostics') setShowDiag(true)
      else if (action === 'check-updates' && !checkingNativeUpdate.current) {
        checkingNativeUpdate.current = true
        setToast({ ok: true, text: t.settings.checking })
        void updates.check().then((info) => {
          if (info) { setUpdate(info); setShowUpdate(true); setToast(null) }
          else setToast({ ok: true, text: t.settings.upToDate })
        }).catch((error) => setToast({ ok: false, text: errText(error) }))
          .finally(() => { checkingNativeUpdate.current = false })
      }
    },
    exit: async (request) => {
      if (closingRequest.current === request.id) return
      if (closingRequest.current !== null) { await shell.resolveExit(request.id, false); return }
      closingRequest.current = request.id
      try {
        if (pendingSwitch.current) {
          pendingSwitch.current(false)
          await Promise.resolve()
        }
        const exited = await switchMeeting(async () => {
          if (!await shell.resolveExit(request.id, true)) throw new Error(msg().workbench.exitExpired)
        }, { confirm: false })
        if (!exited) await shell.resolveExit(request.id, false)
      } catch (error) {
        setToast({ ok: false, text: errText(error) })
        await shell.resolveExit(request.id, false).catch(() => undefined)
      } finally { closingRequest.current = null }
    },
  }
  useEffect(() => {
    if (!isDesktop || !restored) return
    let disposed = false
    const menu = shell.onMenuAction((action) => nativeHandlers.current.menu(action))
    void menu.catch((error) => recordError('menu', errText(error)))
    const exit = shell.onExitRequested((request) => { void nativeHandlers.current.exit(request).catch((error) => recordError('exit', errText(error))) })
    void exit.then(async () => {
      if (disposed) return
      const pending = await shell.pendingExit()
      if (pending && !disposed) await nativeHandlers.current.exit(pending)
    }).catch((error) => recordError('exit', errText(error)))
    return () => { disposed = true; for (const subscription of [menu, exit]) void subscription.then((off) => off()).catch(() => undefined) }
  }, [restored])
  useEffect(() => {
    if (isDesktop) void shell.setLocale(currentLang()).catch((error) => recordError('menu', errText(error)))
  }, [t])
  function changeHotkey(accel: string) {
    const change = hotkeyChange.current
    const version = ++change.version
    setHotkeyBusy(true)
    setHotkeyErr(null)
    change.pending = change.pending.then(async () => {
      try {
        await shell.setShortcut(accel || null)
        change.applied = accel
        save('boardcast.hotkey', accel)
      } catch (error) {
        if (version === change.version) setHotkeyErr({ accel, message: errText(error) })
      } finally {
        if (version === change.version) {
          setHotkey(change.applied)
          setHotkeyBusy(false)
        }
      }
    })
  }
  useEffect(() => {
    if (isDesktop) changeHotkey(hotkey)
  }, [])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing || document.querySelector('.modal')) return
      // Native inputs own their arrows, and Space activates a focused button.
      if ((e.target as HTMLElement).closest('input:not([type="range"]), textarea, select, [contenteditable]')) return
      if (e.key === ' ' && (e.target as HTMLElement).closest('button, [role="button"]')) return
      const slider = (e.target as HTMLElement).closest('input[type="range"]')
      // 回放：空格播放 / 暂停，左右键逐步走，Esc 或回放组合键退出
      const r = replayRef.current
      if (r) {
        if (e.key === 'Escape' || actionFor(e) === 'replay') keys.current.exitReplay()
        else if (slider) return
        else if (e.key === ' ') keys.current.toggleReplay()
        else if (e.key === 'ArrowLeft') keys.current.seekReplay(r.at - 1)
        else if (e.key === 'ArrowRight') keys.current.seekReplay(r.at + 1)
        else return
        e.preventDefault()
        return
      }
      if (slider || mode !== 'ink') return
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
        // 收起圈选，画好的圈也不要了
        clearCircles()
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
    if (replayRef.current) return true
    if (transitionRef.current || listeningRef.current || micRequest.current || linesRef.current.some((line) => line.ms === undefined)) {
      setToast({ ok: false, text: t.app.replayBlocked })
      return false
    }
    commitPencilEdit()
    const total = stepsRef.current.length
    if (!total) { setToast({ ok: false, text: t.meetings.noReplay }); return false }
    summingFor.current?.abort()
    const saved = now()
    setSelection(null)
    setRenaming(null)
    setExportMenu(false)
    board.reset()
    setGraph(emptyGraph())
    replayRef.current = { at: 0, playing: true, speed: 1, total, saved }
    setReplay(replayRef.current)
    return true
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
    restoreScene(p.scene)
    session.graph = p.graph
    board.restore(p.layout)
    editHistory.current.clear()
    fixes.current.reset()
    lastInk.current = p.graph
    stepsRef.current = p.steps ?? []
    recorded.current = replayTo(stepsRef.current, stepsRef.current.length)
    recordedScene.current = replaySceneTo(stepsRef.current, stepsRef.current.length)
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
    if (transitionRef.current || i === pageRef.current || i < 0 || i >= pagesRef.current.length) return
    commitPencilEdit()
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
    if (transitionRef.current) return
    commitPencilEdit()
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

  /**
   * 回放到哪一刻，就从哪一刻另起一张板接在最后：带着那一刻的板书、排版位置和走到那一刻的回放记录，
   * 原来这张不动。会议记录里记一笔是从哪张板的哪一刻分出来的。
   */
  function forkPage() {
    const r = replayRef.current
    if (!r?.at) return
    const s = stepsRef.current
    const from = pageRef.current
    const fork: Page = {
      graph: replayTo(s, r.at),
      // 回放时排出来的位置就是现在画面上的样子；退出回放会换回原来的排版，所以先记下
      layout: board.snapshot(),
      steps: s.slice(0, r.at),
      scene: r.at === r.total ? r.saved.scene : replaySceneTo(s, r.at),
      underlay: pagesRef.current[from]?.underlay,
    }
    const when = clock(s[r.at - 1].t - s[0].t)
    exitReplay()
    const all = [...pagesRef.current]
    all[from] = capture()
    all.push(fork)
    const to = all.length - 1
    pagesRef.current = all
    pageRef.current = to
    showPage(fork)
    setPage(to)
    setThumbs(all.map((p) => p.thumb))
    setSuggest(false)
    setLines((ls) => [...ls, { text: t.app.forkNote(from + 1, when, to + 1), manual: true, ms: 0, page: to }])
  }

  /** 导入的几页接在后面（当前这张是空板就从它开始），切到第一张，会议记录里记一笔 */
  function addPages(add: Page[], note: string) {
    commitPencilEdit()
    exitReplay()
    const all = [...pagesRef.current]
    all[pageRef.current] = capture()
    const cur = all[pageRef.current]
    const blank = !cur.underlay && !cur.graph.nodes.some((n) => n.ink === 'ink') && !sceneHasContent(cur.scene)
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
  const importTrigger = useRef<HTMLButtonElement>(null)
  const [dropping, setDropping] = useState(false)
  const [importMenu, setImportMenu] = useState(false)
  const [arxivInput, setArxivInput] = useState('')
  const [showDiag, setShowDiag] = useState(false)
  const [importTask, setImportTask] = useState<ImportTaskState | null>(null)
  const [importSaveError, setImportSaveError] = useState<string>()
  const importWork = useRef<{
    state: ImportTaskState
    controller: AbortController
    source: File | string
    current: () => boolean
  } | null>(null)

  function cancelImport() {
    importWork.current?.controller.abort()
    importWork.current = null
    setImportTask(null)
    setImportSaveError(undefined)
  }

  function beginImport(name: string, source: File | string) {
    const trigger = compact ? moreTrigger : importTrigger
    trigger.current?.focus()
    cancelImport()
    const epoch = taskEpoch.current
    const controller = new AbortController()
    const state: ImportTaskState = {
      name, meetingId: meetingIdRef.current, meetingTitle: snapshot(meetingRef.current).title, page: pageRef.current,
      status: 'reading', progress: t.importTask.reading,
    }
    const work = {
      state, controller, source,
      current: (): boolean => importWork.current === work && !controller.signal.aborted && epoch === taskEpoch.current && state.meetingId === meetingIdRef.current && !transitionRef.current,
    }
    importWork.current = work
    setImportTask(state)
    setImportMenu(false)
    setToast(null)
    return work
  }

  function importProgress(work: NonNullable<typeof importWork.current>, progress: string) {
    if (!work.current()) return
    work.state = { ...work.state, status: 'reading', progress }
    setImportTask(work.state)
  }

  function importReady(work: NonNullable<typeof importWork.current>, preview: ImportPreview) {
    if (!work.current()) return
    work.state = { ...work.state, status: 'ready', preview }
    setImportTask(work.state)
  }

  function importFailed(work: NonNullable<typeof importWork.current>, error: unknown, configureLocal = false) {
    if (!work.current()) return
    work.state = { ...work.state, status: 'error', error: t.app.importFailed(errText(error)), configureLocal }
    setImportTask(work.state)
  }

  /** Readers only prepare a preview. Choosing a destination commits it after the old content is safe. */
  async function placeImport(fresh: boolean) {
    const task = importWork.current?.state
    if (task?.status !== 'ready' || task.meetingId !== meetingIdRef.current || transitionRef.current) return
    const preview = task.preview
    setImportSaveError(undefined)
    const success = await switchMeeting(() => {
      if (fresh) resetNow()
      else goPage(task.page)
      cancelImport()
      if (fresh && preview.title) {
        meetingRef.current = { ...meetingRef.current, title: preview.title, titled: true }
        setMeeting(meetingRef.current)
      }
      addPages(preview.pages, preview.note)
      if (preview.undoable) {
        editHistory.current.record({ graph: emptyGraph(), layout: new StableLayout().snapshot() })
        setUndoTick((n) => n + 1)
      }
    }, { confirm: fresh })
    if (!success && importWork.current?.state === task) setImportSaveError(transitionError.current ?? undefined)
  }

  async function importPdfData(name: string, data: ArrayBuffer | Uint8Array, work: NonNullable<typeof importWork.current>) {
    if (!work.current()) return
    const { importPdf } = await import('./board/importPdf.ts')
    if (!work.current()) return
    const r: PdfImport = await importPdf(data, (done, total) => importProgress(work, t.app.readingPages(name, done, total)), work.controller.signal)
    const details = [
      r.refsSkipped && t.app.refsSkipped(r.refsSkipped),
      r.unread && t.app.unread(r.unread),
      r.scanned && t.app.scanned(r.scanned),
    ].filter((x): x is string => !!x)
    importReady(work, { pages: r.pages, note: t.app.pdfNote(name, r.total, r.pages.length, details), details, title: r.title })
  }

  async function importArxiv(input: string) {
    if (transitionRef.current) return
    exitReplay()
    if (waiting) { setToast({ ok: false, text: t.app.busyImport }); return }
    const work = beginImport(input, input)
    try {
      const { parseArxiv } = await import('./board/arxiv.ts')
      if (!work.current()) return
      const id = parseArxiv(input)
      if (!id) throw new Error(t.app.arxivBad)
      if (!isDesktop) throw new Error(t.app.arxivWeb)
      setArxivInput('')
      importProgress(work, t.app.arxivDownloading(id))
      await importPdfData(`arXiv ${id}`, await arxivPdf(id, work.controller.signal), work)
    } catch (error) { importFailed(work, error) }
  }

  async function importFile(f: File) {
    if (transitionRef.current) return
    exitReplay()
    if (waiting) { setToast({ ok: false, text: t.app.busyImport }); return }
    const work = beginImport(f.name, f)
    const isImage = /^image\//.test(f.type) || /\.(png|jpe?g|webp|gif)$/i.test(f.name)
    try {
      if (/\.pdf$/i.test(f.name) || f.type === 'application/pdf') {
        await importPdfData(f.name, await f.arrayBuffer(), work)
        return
      }
      if (isImage) {
        if (isDesktop && !builtin?.vision) throw new Error(t.app.needVision)
        const url = isDesktop ? await localReady() : baseUrl
        if (!work.current()) return
        if (!url) throw new Error(t.app.noLocal)
        if (!isLoopback(url)) throw new Error(t.importTask.localOnly)
        const { importImage, llamaReader, MAX_AREAS } = await import('./board/importImage.ts')
        if (!work.current()) return
        const signal = work.controller.signal
        const r = await importImage(f, llamaReader(url, signal), (done, total) => importProgress(work, t.app.readingBlocks(f.name, done, total)), signal)
        importReady(work, {
          pages: [r.page], note: t.app.imageNote(f.name, r.read, r.capped),
          details: [t.importTask.imageSummary(r.found, r.read), ...(r.capped ? [t.importTask.imageLimit(MAX_AREAS)] : []), ...(!r.read ? [t.importTask.imageNoText] : [])],
        })
        return
      }
      if (!/\.(excalidraw|json)$/i.test(f.name) && f.type !== 'application/json') throw new Error(t.app.unsupported)
      const { importExcalidraw } = await import('./board/importExcalidraw.ts')
      if (!work.current()) return
      const raw = await f.text()
      if (!work.current()) return
      const r = importExcalidraw(raw)
      importReady(work, {
        pages: [{ graph: r.graph, layout: r.layout, scene: r.scene, underlay: r.underlay }],
        note: t.app.excalidrawNote(f.name, r), undoable: true,
        details: [t.importTask.excalidrawSummary(r.scene.elements.filter((e) => !e.isDeleted).length)],
      })
    } catch (error) { importFailed(work, error, isImage) }
  }

  function importFiles(files: File[]) {
    if (files.length > 1) { setToast({ ok: false, text: t.importTask.oneFile }); return }
    if (files[0]) void importFile(files[0])
  }

  /** 打开一场会；从搜索结果来的，跳到命中的板和那一句 */
  /** 一场会有哪几张板（缩略图）：选底板用 */
  async function boardsOf(id: string): Promise<(Thumb | undefined)[]> {
    if (id === meeting.id) return pagesRef.current.map((p, i) => (i === pageRef.current ? thumbRef.current : p.thumb))
    const m = await meetings.load(id)
    if (!m) throw new Error(t.meetings.missing)
    return m.pages?.length ? m.pages.map((p) => p.thumb) : [undefined]
  }

  /**
   * 带着旧板开新会（#49）：拿那场会的第 `at` 张板当底板，排版位置、导入页、这场会的术语都带过来。
   * 底板上原样留着的东西用淡墨画，新会上加的、改的、划掉的用正常墨迹。
   */
  async function continueOn(id: string, at: number): Promise<boolean> {
    return switchMeeting(async () => {
      let src: Page | undefined
      let from: string
      let glossary: string
      let onlyLocal: boolean
      let sourceIssues: Meeting['assetIssues']
      if (id === meetingIdRef.current) {
        const all = [...pagesRef.current]
        all[pageRef.current] = capture()
        src = all[at]
        from = snapshot(meetingRef.current).title
        glossary = meetingGlossaryRef.current
        onlyLocal = localOnlyRef.current
        sourceIssues = assetIssues
      } else {
        const m = await meetings.load(id)
        if (!m) throw new Error(t.app.openFailed)
        src = m.pages?.length ? m.pages[at] : { graph: m.graph, layout: m.layout, scene: m.scene }
        from = m.title
        glossary = m.glossary ?? ''
        onlyLocal = !!m.localOnly
        sourceIssues = m.assetIssues
      }
      if (!src) throw new Error(t.app.openFailed)
      // An unresolved asset belongs to the original meeting; a new ID cannot recover it.
      const unavailable = [src.underlay, ...Object.values(src.scene?.files ?? {}).map((file) => file.dataURL)].find((url) => url?.startsWith('asset:'))
      if (unavailable) throw new Error(t.errors.assetIntegrity(unavailable.slice(6), sourceIssues?.find((issue) => issue.key === unavailable.slice(6))?.reason ?? 'missing'))
      // A missing preview is rebuildable and must not enter the new replay seed.
      const scene = src.scene?.overlay?.url.startsWith('asset:') ? { ...src.scene, overlay: undefined } : src.scene
      const base = eraseDrafts(src.graph)
      const page: Page = {
        graph: base,
        layout: src.layout,
        base,
        underlay: src.underlay,
        thumb: src.thumb,
        scene,
        steps: [{ t: Date.now(), line: 1, d: diffGraph(emptyGraph(), base) ?? {}, scene }],
      }
      resetNow()
      pagesRef.current = [page]
      showPage(page)
      setThumbs([page.thumb])
      setMeetingGlossary(glossary)
      setLocalOnly(onlyLocal)
      setToast(null)
      setMeeting((m) => ({ ...m, title: t.app.continuedTitle(from), titled: true }))
      const initial = [{ text: t.app.continuedNote(from, at + 1), manual: true, ms: 0, page: 0 }]
      linesRef.current = initial
      setLines(initial)
    })
  }

  async function openOther(id: string, hit?: Hit): Promise<boolean> {
    if (id === meetingIdRef.current) { setLibraryHit(hit ?? null); return true }
    return switchMeeting(async () => {
      const m = await meetings.load(id)
      if (!m) throw new Error(t.app.openFailed)
      if (m.id !== id) await meetings.save(m)
      openMeeting(m)
      setLibraryHit(hit ?? null)
    })
  }

  useEffect(() => {
    if (showMeetings || !libraryHit) return
    // Reveal after the library releases modal focus and the opened meeting has rendered.
    const hit = libraryHit
    setLibraryHit(null)
    if (hit.page !== undefined) goPage(hit.page)
    if (hit.line !== undefined) {
      setShowLog(true)
      setSidebarTab('transcript')
      setJumpLine({ index: hit.line })
    }
    if (hit.where === 'summary') { setShowLog(true); setSidebarTab('summary') }
    if (hit.where === 'board' && hit.node) locateContent({ selection: { kind: 'node', id: hit.node } })
  }, [libraryHit, showMeetings])

  async function renameMeeting(id: string, title: string) {
    if (id !== meetingIdRef.current) {
      const saved = await meetings.load(id)
      if (!saved) throw new Error(t.meetings.missing)
      await meetings.save({ ...saved, title, titled: true })
      return
    }
    if (transitionRef.current || replayRef.current) throw new Error(t.meetings.stayOnFailure)
    meetingRef.current = { ...meetingRef.current, title, titled: true }
    setMeeting(meetingRef.current)
    writer.changed()
    if (!await retrySave()) throw new Error(writer.status.error ?? t.workbench.saveFailed)
  }

  async function openFromLibrary(id: string, action: 'replay' | 'export') {
    if (!await openOther(id)) return false
    if (action === 'replay') {
      if (listeningRef.current || micRequest.current || linesRef.current.some((line) => line.ms === undefined)) throw new Error(t.app.replayBlocked)
      if (!stepsRef.current.length) throw new Error(t.meetings.noReplay)
    }
    setLibraryAction(action)
    return true
  }

  useEffect(() => {
    if (showMeetings || !libraryAction) return
    if (libraryAction === 'replay') startReplay()
    else { setExportScope('meeting'); setExportMenu(true) }
    setLibraryAction(null)
  }, [showMeetings, libraryAction])

  async function play() {
    if (recordedOnly) return playRecorded()
    const c = cases.find((x) => x.id === demo)
    if (!c || !await reset()) return
    const epoch = taskEpoch.current
    setPlaying(true)
    try {
      for (const turn of c.turns) {
        if (epoch !== taskEpoch.current) return
        say(turn)
        await new Promise((r) => setTimeout(r, 2500))
      }
    } finally {
      if (epoch === taskEpoch.current) setPlaying(false)
    }
  }

  /** Replay a recorded sample meeting: no model needed */
  async function playRecorded() {
    if (!demo) return
    setToast(null)
    const opened = await switchMeeting(async () => {
      const d = await loadDemo(demo).catch((error) => { throw new Error(t.app.demoFailed(errText(error))) })
      resetNow()
      const id = newMeetingId()
      openMeeting(demoMeeting(d, id))
      recordedMeeting.current = { id, content: meetingContent(snapshot(meetingRef.current)) }
      // Samples have no stored revision; their first personal edit must trigger a real save.
      loadedMeeting.current = null
      writer.reset()
      replayRef.current = null
    })
    if (opened) startReplay()
  }

  // 现在是哪个模型在画
  const ready = remoteOn || (!!online && !warming && !warmupError)
  const localProblem = warmupError ? t.errors.modelWarmupFailed(warmupError) : null
  const modelStatus: LlmStatus | null = builtin && localProblem ? { ...builtin, state: 'error', message: localProblem } : builtin
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
  } else if (localProblem) {
    status = localProblem
  } else if (builtin?.state === 'ready') {
    status = t.app.builtinReady(localName ?? '', warming)
  } else if (builtin?.state === 'idle') {
    status = t.app.builtinIdle(localName ?? '')
  } else if (builtin?.state === 'starting') {
    status = t.app.builtinStarting(localName)
  } else if (builtin) {
    status = llmProblem(builtin)
  } else {
    status = online ? `${baseUrl.replace(/^https?:\/\//, '')}` : t.app.noModel
  }
  const remoteBroken = remoteView?.enabled && remoteState.state === 'fail'
  const needsModelManagement = !remoteOn && remoteState.state !== 'checking' && (!!localProblem || builtin?.state === 'no-model' || builtin?.state === 'error')

  const speakers = useMemo(() => {
    const seen = ['A', 'B', 'C']
    for (const l of lines) if (l.speaker && !seen.includes(l.speaker)) seen.push(l.speaker)
    if (speaker.trim() && !seen.includes(speaker.trim())) seen.push(speaker.trim())
    return seen
  }, [lines, speaker])
  const trace = traceWho ? { who: traceWho, color: SPEAKER_MARKERS[speakerSlot(speakers, traceWho)] } : null
  const selRaised = raisedBy(selection)

  const empty = lines.length === 0 && graph.nodes.length === 0 && !graph.underlay && !sceneHasContent(sceneRef.current) && !partial
  /** 会上说的话（不算手动修改） */
  const said = lines.filter((l) => !l.manual)
  const unreadLines = replay || unreadFrom === null ? 0 : Math.max(0, said.length - unreadFrom)
  /** 模型还有没画完的句子 */
  const waiting = lines.some((l) => l.ms === undefined)
  const queuedLines = lines.filter((l) => !l.manual && l.ms === undefined).length
  const failedLines = lines.filter((l) => !l.manual && l.failed).length
  const contentScene = replay ? replay.at === replay.total ? replay.saved.scene : replaySceneTo(stepsRef.current, replay.at) : sceneRef.current
  /** 会议记录栏里显示的：回放时只显示到当时说到的那一句 */
  const shown = replay ? lines.slice(0, replay.at ? stepsRef.current[replay.at - 1].line : 0) : lines
  const hasBoardContent = graph.nodes.length > 0 || !!graph.underlay || sceneHasContent(sceneRef.current)
  const showPages = mode === 'pencil' || pagesRef.current.length > 1 || hasBoardContent
  const sourcePages = pagesRef.current.map((p) => p.graph.underlay?.no)
  const pageSource = t.pages.sourcePage(sourcePages[page])
  const savedDate = new Date(saveStatus.savedAt ?? NaN)
  const savedTime = Number.isFinite(savedDate.getTime()) ? savedDate : null
  const saveReason = storageReason(saveStatus.error ?? '')
  const canExportMeeting = hasBoardContent || lines.length > 0 || !!summary || pagesRef.current.some((p) => p.graph.nodes.length > 0 || !!p.underlay || sceneHasContent(p.scene))
  const libraryCurrent = useMemo(() => (canExportMeeting || meeting.titled || !!meetingGlossary.trim()) && meeting.id !== recordedMeeting.current?.id ? {
    info: { ...meeting, updatedAt: saveStatus.savedAt ?? meeting.createdAt, lines: said.length, pages: pagesRef.current.length, unfinished: queuedLines + failedLines },
    content: { id: meeting.id, title: meeting.title, lines, summary, graph, pages: pagesRef.current.map((p, i) => ({ graph: i === page ? graph : p.graph })) },
    state: saveStatus.state,
  } : undefined, [canExportMeeting, meetingGlossary, meeting, lines, summary, graph, page, saveStatus])


  function closeMeetingMenu() {
    if (compact) moreTrigger.current?.focus()
    setMeetingMenu(false)
  }
  const meetingActions = <>
          <button role={compact ? 'menuitem' : undefined} type="button" className="ghost small review-entry" disabled={transitionBusy || !!replay || (!canExportMeeting && !listening && !micPending)} onClick={() => {
            closeMeetingMenu()
            if (reviewing) { setSidebarTab('summary'); setShowLog(true) }
            else { setReviewError(null); setReviewDialog(true) }
          }}>{reviewing ? t.review.title : t.review.finish}</button>
          <div className="menu-anchor">
            <button role={compact ? 'menuitem' : undefined}
              type="button"
              className={`icon labeled${importMenu ? ' on' : ''}`}
              ref={importTrigger}
              onClick={() => { closeMeetingMenu(); setExportMenu(false); setImportMenu(!importMenu) }}
              disabled={!!replay || transitionBusy}
              aria-haspopup="menu"
              aria-expanded={importMenu}
              aria-label={t.app.import}
              title={t.app.importTitle}
            >
              <Upload /><span>{t.app.import}</span>
            </button>

          </div>

          <div className="menu-anchor">
            <button role={compact ? 'menuitem' : undefined}
              type="button"
              ref={exportTrigger}
              className={`icon labeled${exportMenu ? ' on' : ''}`}
              onClick={() => { closeMeetingMenu(); setImportMenu(false); setExportMenu(!exportMenu) }}
              disabled={!canExportMeeting || exporting}
              aria-haspopup="menu"
              aria-expanded={exportMenu}
              aria-label={t.app.export}
              title={t.app.export}
            >
              <Download /><span>{t.app.export}</span>
            </button>

          </div>
          <button role={compact ? 'menuitem' : undefined} type="button" className="icon" disabled={transitionBusy} onClick={() => { closeMeetingMenu(); void reset() }} aria-label={t.app.newMeeting} title={t.app.newMeetingTitle}>
            <Plus /><span className="meeting-action-label">{t.app.newMeeting}</span>
          </button>
          <button role={compact ? 'menuitem' : undefined}
            type="button"
            className="icon"
            onClick={() => { closeMeetingMenu(); setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length]) }}
            aria-label={t.theme.aria(t.theme[theme])}
            title={t.theme.title(t.theme[theme])}
          >
            {theme === 'light' ? <Sun /> : theme === 'dark' ? <Moon /> : <Auto />}
            <span className="meeting-action-label">{t.theme.aria(t.theme[theme])}</span>
          </button>
          <button role={compact ? 'menuitem' : undefined} type="button" className="icon" onClick={() => { closeMeetingMenu(); setShowSettings(true) }} aria-label={t.app.settings} title={t.app.settings}>
            <Gear /><span className="meeting-action-label">{t.app.settings}</span>
          </button>
  </>

  return (
    <div className={`app${showLog ? '' : ' no-log'}${showPages ? ' has-pages' : ''}`} style={{ '--sidebar-width': `${sidebarWidth}px` } as CSSProperties}>
      {splash && <Splash onDone={() => setSplash(false)} />}

      <header className="topbar">
        <div className="meeting-bar">
        <div className="brand">
          <Wordmark size={24} className="logo" />
        </div>
        <div className="meeting-heading">
          <button type="button" className="meeting-library" disabled={transitionBusy} onClick={() => setShowMeetings(true)} title={t.app.meetingsTitle} aria-label={t.workbench.library}>
            <History size={15} /><b className="library-label">{compact ? t.workbench.libraryCompact : t.workbench.library}</b>
          </button>
          <button type="button" className="meeting-title" disabled={transitionBusy || !restored || !!replay} onClick={() => setShowMeetingTitle(true)} title={`${t.meetings.renameTitle}: ${meeting.title}`} aria-label={`${t.meetings.renameTitle}: ${meeting.title}`}>
            <span>{meeting.title}</span><Pen size={13} />
          </button>
        </div>
        {!restored && saveStatus.state === 'idle' && <span className="save-state" role="status">{t.workbench.openingPrevious}</span>}
        {saveStatus.state !== 'idle' && <span className={`save-state ${saveStatus.state}`} role="status" title={saveStatus.error}>
          <i className="dot" />
          {saveStatus.state === 'saved' ? t.workbench.saved : saveStatus.state === 'saving' ? t.workbench.saving : saveStatus.state === 'error' ? t.workbench.saveFailed : t.workbench.unsaved}
          {saveStatus.state === 'saved' && savedTime && <time dateTime={savedTime.toISOString()} title={savedTime.toLocaleString(lang)}>{savedTime.toLocaleTimeString(lang, { hour: '2-digit', minute: '2-digit', hour12: false })}</time>}
        </span>}
        <button
          type="button"
          className={`chip status${ready ? ' ok' : ''}${remoteBroken ? ' warn' : ''}`}
          onClick={() => needsModelManagement ? setShowModels(true) : setShowSettings(true)}
          title={needsModelManagement ? t.settings.manageModels : t.app.statusTitle}
        >
          <i className="dot" />
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
          <div className="meeting-actions">
            {compact ? <>
              <button type="button" className="ghost meeting-more" ref={moreTrigger} aria-haspopup="menu" aria-expanded={meetingMenu || importMenu || exportMenu} onClick={() => { setImportMenu(false); setExportMenu(false); setMeetingMenu(!meetingMenu) }}>{t.workbench.more}</button>
              {meetingMenu && <ActionMenu label={t.workbench.meetingActions} trigger={moreTrigger} className="meeting-menu" onClose={() => setMeetingMenu(false)}>{meetingActions}</ActionMenu>}
            </> : meetingActions}
          </div>
        </div>
        <div className="topbar-scroll">
        <div className="boardbar">
        <div className="seg" role="radiogroup" aria-label={t.app.styleAria} aria-describedby={modeHint === 'visible' ? 'board-mode-hint' : undefined} onKeyDown={(event) => {
          if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || event.nativeEvent.isComposing) return
          const next = event.key === 'Home' ? 'ink' : event.key === 'End' ? 'pencil' : ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key) ? mode === 'ink' ? 'pencil' : 'ink' : null
          if (next === null) return
          event.preventDefault()
          event.stopPropagation()
          changeMode(next)
          event.currentTarget.querySelectorAll<HTMLButtonElement>('button')[Number(next === 'pencil')]?.focus()
        }}>
          <button role="radio" aria-checked={mode === 'ink'} tabIndex={mode === 'ink' ? 0 : -1} className={mode === 'ink' ? 'on' : ''} onClick={() => changeMode('ink')}>
            <Pen size={15} />
            {t.app.ink}
          </button>
          <button role="radio" aria-checked={mode === 'pencil'} tabIndex={mode === 'pencil' ? 0 : -1} className={mode === 'pencil' ? 'on' : ''} onClick={() => changeMode('pencil')}>
            <Pencil size={15} />
            {t.app.pencil}
          </button>
        </div>
        <div className="icons">
          {compact ? <div className="content-tabs" role="tablist" aria-label={t.workbench.meetingView} onKeyDown={(event) => {
            const next = event.key === 'Home' ? false : event.key === 'End' ? true : ['ArrowLeft', 'ArrowRight'].includes(event.key) ? !showLog : null
            if (next === null) return
            event.preventDefault()
            event.stopPropagation()
            setShowLog(next)
            event.currentTarget.querySelectorAll<HTMLButtonElement>('button')[Number(next)]?.focus()
          }}>
            <button type="button" role="tab" id="board-tab" aria-controls="board-panel" aria-selected={!showLog} tabIndex={showLog ? -1 : 0} onClick={() => setShowLog(false)}>{t.workbench.board}</button>
            <button type="button" role="tab" id="transcript-tab" aria-controls="transcript-panel" aria-selected={showLog} tabIndex={showLog ? 0 : -1} onClick={() => setShowLog(true)}>{t.contents.tabs}</button>
          </div> : <button
            type="button"
            className={`icon${showLog ? ' on' : ''}`}
            onClick={() => setShowLog(!showLog)}
            aria-pressed={showLog}
            aria-label={t.app.log}
            title={t.app.log}
          >
            <TranscriptIcon />
          </button>}
          <button
            type="button"
            className={`icon${replay ? ' on' : ''}`}
            onClick={() => (replay ? exitReplay() : startReplay())}
            disabled={!replay && (!stepsRef.current.length || listening || !!micPending || waiting)}
            aria-pressed={!!replay}
            aria-label={t.app.replay}
            title={replay ? t.app.exitReplay : waiting || listening || micPending ? t.app.replayBlocked : t.app.replayHint}
          >
            <ReplayIcon />
          </button>
          <button
            type="button"
            className={`icon${lasso ? ' on' : ''}`}
            onClick={toggleLasso}
            disabled={!!replay || mode !== 'ink'}
            aria-pressed={lasso}
            aria-label={t.app.lasso}
            ref={circleButton}
            title={t.app.lassoTitle}
          >
            <Lasso />
          </button>
          <button
            type="button"
            className="icon"
            onClick={undo}
            disabled={!!replay || (!editHistory.current.canUndo && !circles.length)}
            aria-label={t.app.undo}
            title={t.app.undoTitle}
          >
            <Undo />
          </button>
          <button type="button" className="icon" onClick={redo} disabled={!!replay || !editHistory.current.canRedo} aria-label={t.app.redo} title={t.app.redoTitle}>
            <Redo />
          </button>
          <button type="button" className="icon" onClick={tidyUp} disabled={!graph.nodes.length || tidying || waiting || transitionBusy || !!replay} aria-busy={tidying} aria-label={tidying ? t.app.tidying : t.app.tidy} title={tidying ? t.app.tidying : waiting ? t.app.tidyBlocked : t.app.tidyTitle}>
            <Tidy />
          </button>
          </div>
          {mode === 'ink' && sceneOverlay && <button type="button" className="ghost small scene-edit-hint" onClick={() => changeMode('pencil')}>{t.workbench.editFreeObjects}</button>}
          <span className="board-location">{t.pages.board(page + 1)} / {pagesRef.current.length}{pageSource && ` · ${pageSource}`}</span>
        </div>
        {saveStatus.state === 'error' && <div className="save-error" role="alert">
          <div><strong>{t.workbench.saveFailed}</strong>{saveReason && <p>{t.workbench.saveRecovery[saveReason]}</p>}<p>{saveStatus.error}</p></div>
          <button type="button" className="secondary" onClick={() => void retrySave()}>{t.workbench.retrySave}</button>
          <button type="button" onClick={() => void exportAs('html')}>{t.workbench.exportCopy}</button>
        </div>}
        {!!assetIssues.length && <div className="asset-warning" role="status">
          <details>
            <summary>{t.workbench.assetWarning(assetIssues.length)}</summary>
            <p>{t.workbench.assetRecovery}</p>
            <ul>{assetIssues.map((issue) => <li key={issue.key}><code>{issue.key}</code> · {t.workbench.assetReasons[issue.reason]}</li>)}</ul>
            <button type="button" className="secondary small" onClick={() => void exportAs('md')}>{t.workbench.exportText}</button>
          </details>
        </div>}
        {transitionBusy && <div className="transition-state" role="status">{reviewStage ? t.review[reviewStage] : closingRequest.current !== null ? t.workbench.preparingExit : t.workbench.switchingMeeting}</div>}
        </div>
      </header>
          <input
            ref={fileInput}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif,.pdf,application/pdf,.excalidraw,application/json,.json"
            hidden
            onChange={(e) => {
              const files = Array.from(e.target.files ?? [])
              e.target.value = ''
              importFiles(files)
            }}
          />
            {importMenu && (
              <ActionMenu trigger={compact ? moreTrigger : importTrigger} label={t.app.import} className="import-menu" onClose={() => setImportMenu(false)}>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setImportMenu(false)
                    const trigger = compact ? moreTrigger : importTrigger
                    trigger.current?.focus()
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
                      onKeyDown={(e) => !e.nativeEvent.isComposing && e.key === 'Escape' && setImportMenu(false)}
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
              </ActionMenu>
            )}
            {exportMenu && (
              <ActionMenu trigger={compact ? moreTrigger : exportTrigger} label={t.app.export} onClose={() => setExportMenu(false)}>
                <div className="export-scope" role="group" aria-label={t.app.exportScope}>
                  <button type="button" role="menuitemradio" aria-checked={exportScope === 'meeting'} onClick={() => setExportScope('meeting')}>{t.app.exportMeeting}</button>
                  <button type="button" role="menuitemradio" aria-checked={exportScope === 'board'} onClick={() => setExportScope('board')}>{t.app.exportBoard}</button>
                </div>
                {exportScope === 'meeting' ? <>
                <button type="button" role="menuitem" onClick={() => void exportAs('md')}>
                  <span>Markdown</span>
                  <small>{t.app.exportMd}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('pdf')}>
                  <span>{t.app.exportPdf}</span>
                  <small>{t.app.exportPdfHint}</small>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportAs('html')}>
                  <span>{t.app.exportHtml}</span>
                  <small>{t.app.exportHtmlHint}</small>
                </button>
                </> : <>
                {!hasBoardContent && <p className="menu-note">{t.workbench.emptyBoardExport}</p>}
                <button type="button" role="menuitem" disabled={!hasBoardContent} onClick={() => void exportAs('png')}>
                  <span>{t.app.exportPng}</span>
                  <small>{mode === 'ink' ? t.app.exportPngInk : t.app.exportPngPencil}</small>
                </button>
                <button type="button" role="menuitem" disabled={!hasBoardContent} onClick={() => void exportAs('svg')}>
                  <span>{t.app.exportSvg}</span>
                  <small>{t.app.exportSvgHint}</small>
                </button>
                <button type="button" role="menuitem" disabled={!hasBoardContent} onClick={() => void exportAs('anim')}>
                  <span>{t.app.exportAnim}</span>
                  <small>{t.app.exportAnimHint}</small>
                </button>
                <button type="button" role="menuitem" disabled={!hasBoardContent} onClick={() => void exportAs('excalidraw')}>
                  <span>Excalidraw</span>
                  <small>{t.app.exportExcalidrawHint}</small>
                </button>
                </>}
              </ActionMenu>
            )}

      <main
        id="board-panel"
        tabIndex={-1}
        role={compact ? 'tabpanel' : undefined}
        aria-labelledby={compact ? 'board-tab' : undefined}
        inert={compact && showLog}
        aria-hidden={compact && showLog}
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
          const files = Array.from(e.dataTransfer.files)
          if (!files.length) return
          e.preventDefault()
          e.stopPropagation()
          setDropping(false)
          importFiles(files)
        }}
      >
        {(pagesRef.current[page]?.base && lines[0]?.manual || modeHint === 'visible') && <div className="board-notices">
        {pagesRef.current[page]?.base && lines[0]?.manual && <p className="board-origin">{lines[0].text}</p>}
        {modeHint === 'visible' && <div className="mode-note">
          <p id="board-mode-hint" role="status">{t.workbench.modeHint}</p>
          <button type="button" className="secondary small" onClick={() => {
            setModeHint('dismissed')
            save('boardcast.mode-hint', 'dismissed')
            document.querySelector<HTMLButtonElement>('.boardbar [role="radio"][aria-checked="true"]')?.focus()
          }}>{t.workbench.modeHintDismiss}</button>
        </div>}
        </div>}
        {lasso && !replay && mode === 'ink' && (
          <div className="lasso-hint" role="status">
            <Lasso size={15} />
            <span>
              {circles.length ? circleTargets.length ? t.app.lassoCircled(circleTargets.length, circledNames()) : t.app.circleEmpty : t.app.lassoHint}
              {circleTargets.length > 0 && circles.some((c) => c.grouped) && <small>{t.app.circleGrouped}</small>}
              {circleIds.length > MAX_POINTED && <small>{t.app.circleLimited(MAX_POINTED)}</small>}
            </span>
            {circles.length > 0 && (
              <button type="button" className="ghost" onClick={undoCircle}>
                {t.app.undo}
              </button>
            )}
            <button type="button" className="ghost" onClick={clearCircles}>
              {t.app.lassoCancel}
            </button>
          </div>
        )}
        {mode === 'ink' ? (
          <InkBoard
            key={`${meeting.id}:${page}`}
            graph={graph}
            hot={hot}
            dark={dark}
            messy={messyLater / 100}
            bottomInset={inset}
            tidy={tidy}
            weight={weightLater / 100}
            englishWeight={englishWeightLater / 100}
            selection={replay ? null : selection}
            focusContent={focusContent}
            layoutRev={layoutRev}
            underlay={pagesRef.current[page]?.underlay}
            overlay={sceneOverlay}
            lasso={!replay && lasso}
            onCircle={onCircle}
            circles={circles.map((c) => c.loop)}
            base={pagesRef.current[page]?.base}
            trace={trace}
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
                      if (e.nativeEvent.isComposing) { if (e.key === 'Enter') e.preventDefault(); return }
                      if (e.key === 'Escape') { e.stopPropagation(); finishRename() }
                    }}
                    placeholder={selection.kind === 'edge' ? t.app.renameEdgePlaceholder : t.app.renamePlaceholder}
                    aria-label={t.app.rename}
                  />
                  <button type="submit" disabled={selection.kind === 'node' && !renaming.trim()}>{t.app.ok}</button>
                </form>
              ) : (
                <>
                  {selRaised && (
                    <button type="button" className="by" onClick={() => jumpToRaised(selection)} title={t.app.raisedByTitle(selRaised.who)}>
                      <span className={`who c${speakerSlot(speakers, selRaised.who)}`}>{selRaised.who}</span>
                    </button>
                  )}
                  {!pinned(session.graph, selection) && (
                  <button
                    ref={inkRenameButton}
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
                    <span className="mark-unit" role="group" aria-label={selectedAt.current ? t.app.markUnitAria : `${t.app.markUnitAria}. ${t.app.markPointHint}`} title={selectedAt.current ? undefined : t.app.markPointHint}>
                      {(['word', 'sentence', 'block'] as const).map((u) => (
                        <button key={u} type="button" className={markUnit === u ? 'on' : ''} aria-pressed={markUnit === u} disabled={u !== 'block' && !selectedAt.current} onClick={() => setMarkUnit(u)}>
                          {t.app.units[u]}
                        </button>
                      ))}
                    </span>
                  )}
                  {selection.kind === 'node' &&
                    MARKS.map((m) => {
                      const node = session.graph.nodes.find((n) => n.id === selection.id)
                      const span = unitSpan(m)
                      const on = span ? hasSpan(node, span) : !!node?.marks.includes(m)
                      return (
                        <button
                          key={m}
                          type="button"
                          className={on ? 'on' : ''}
                          aria-pressed={on}
                          onClick={() => markSelection(m)}
                        >
                          {t.app.marks[m]}
                        </button>
                      )
                    })}
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
                  {selectedSceneAction && <button type="button" onClick={adoptSelectedScene}>
                    {selectedSceneAction === 'model' ? t.workbench.adoptModel : t.workbench.resetStyle}
                  </button>}
                </>
              ))
            }
          />
        ) : (
          <Suspense fallback={<div className="board-loading">{t.app.pencilLoading}</div>}>
            <PencilBoard key={`${meeting.id}:${page}`} onApi={pencilApiReady} ready={pencilInitialized} readOnly={!!replay || transitionBusy} dark={dark} paper={PAPER_LIGHT} onChange={pencilSceneChanged} onPointerUp={() => {
              // Excalidraw finalizes the shape and emits onChange after onPointerUp.
              pencilPointerDown.current = false
              if (pencilTimer.current) clearTimeout(pencilTimer.current)
              pencilTimer.current = setTimeout(() => commitPencilEdit(false), 0)
            }}
              onInteract={() => { pencilView.current.reading = true; setPencilReading(true) }} />
          </Suspense>
        )}
        {mode === 'pencil' && pencilInitialized && pencilReading && <button type="button" className="secondary ink-follow pencil-follow" onClick={(event) => {
          pencilView.current.reading = false
          setPencilReading(false)
          setTidy((n) => n + 1)
          event.currentTarget.closest('main')?.focus({ preventScroll: true })
        }}>{t.workbench.followBoard}</button>}

        {empty && !splash && mode === 'ink' && (
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
              <button type="button" onClick={() => setShowPreparation(true)}>{t.preparation.startMeeting}</button>
              <button type="button" className="secondary" onClick={() => { setImportMenu(true); setExportMenu(false) }}>{t.app.import}</button>
              <button type="button" className="ghost" onClick={() => void play()} disabled={(!ready && !recordedOnly) || playing}>
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
                <button type="button" className="link" onClick={() => setShowSettings('models')}>
                  {isDesktop ? t.app.configureBoard : t.app.connectModel}
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

      </main>

        {showPages && (
          <PageStrip
            thumbs={thumbs}
            sourcePages={sourcePages}
            current={page}
            onGo={goPage}
            onNew={newPage}
            blocked={replay ? t.app.replayNoPage : transitionBusy ? t.workbench.saving : undefined}
            suggest={suggest}
            onDismiss={() => setSuggest(false)}
          />
        )}

      <footer className={`meeting-footer${replay ? " is-replaying" : ""}`}>
        {!replay && (isDesktop || said.length > 0 || remoteOn || localOnly) && <div className="work-status" role="status">
          {localOnly ? <span>{t.workbench.onlyLocal}</span> : cloud && remoteView && <span>{t.workbench.remoteHost(hostOf(remoteView.baseUrl))}</span>}
          {isDesktop && <span>{micPending ? t.composer[micPending] : listening ? `${t.workbench.recording} · ${t.settings.sources[voiceSource]}` : t.preparation.notListening}</span>}
          <span>{partial ? t.workbench.partialText : listening ? t.workbench.waitingForSpeech : t.workbench.transcriptCount(said.length)}</span>
          <span>{queuedLines ? t.workbench.boardQueue(queuedLines) : failedLines ? t.workbench.boardFailed(failedLines) : ready ? t.workbench.boardIdle : t.workbench.boardUnavailable}</span>
        </div>}
        <Composer
          onSend={(t, who) => say({ speaker: who.trim() || undefined, text: t })}
          disabled={transitionBusy || !!replay}
          speaker={speaker}
          setSpeaker={setSpeaker}
          speakers={speakers}
          onManageSpeakers={() => setRenamingWho(speaker.trim() || speakers[0])}
          canListen={isDesktop}
          listening={listening}
          pending={micPending}
          level={level}
          partial={partial}
          announceTranscript={announceTranscript}
          micName={mic}
          voiceErr={voiceErr ? speechMessage(voiceErr) : null}
          toggleMic={() => void toggleMic()}
        />
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
            onFork={waiting ? undefined : forkPage}
            onExit={exitReplay}
          />
        )}
      </footer>

      <aside id="transcript-panel" className="log" role={compact ? 'tabpanel' : undefined} aria-labelledby={compact ? 'transcript-tab' : undefined} aria-label={t.app.log} hidden={!showLog}
        onScrollCapture={(event) => {
          if (sidebarTab !== 'transcript' || resizingTranscript.current) return
          const list = transcriptScroller()
          if (list?.clientHeight && event.target === list) {
            followLatest(list.scrollHeight - list.clientHeight - list.scrollTop < 32)
            rememberTranscript(list)
          }
        }}
        onWheel={(event) => {
          if (sidebarTab !== 'transcript' || event.deltaY >= 0) return
          const list = transcriptScroller()
          if (list && list.scrollHeight > list.clientHeight) followLatest(false)
        }}
      >
        <header>
          <h2>{t.contents.tabs}</h2>
          <span className="count">{said.length ? t.app.count(said.length) : ''}</span>
          <button
            type="button"
            className="ghost small"
            onClick={() => { setSidebarTab('summary'); void makeSummary() }}
            disabled={!!replay || (!summing && (!said.length || !ready || !!summaryCandidate))}
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
        {reviewing && <section className="review-summary" aria-label={t.review.title}>
          <h3>{t.review.title}</h3>
          <p>{t.review.counts(said.length, pagesRef.current.length, queuedLines + failedLines)}</p>
          {failedLines > 0 && <p>{t.review.unfinished}</p>}
          <button type="button" className="ghost small" onClick={() => { setReviewing(false); if (compact) setShowLog(false) }}>{t.review.continue}</button>
        </section>}
        <div className="sidebar-tabs" role="tablist" aria-label={t.contents.tabs} onKeyDown={(event) => {
          const tabs = ['transcript', 'summary', 'contents'] as const
          const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button')]
          const at = buttons.indexOf(document.activeElement as HTMLButtonElement)
          const next = event.key === 'Home' ? 0 : event.key === 'End' ? 2 : event.key === 'ArrowRight' ? (at + 1) % 3 : event.key === 'ArrowLeft' ? (at + 2) % 3 : -1
          if (next < 0) return
          event.preventDefault()
          event.stopPropagation()
          setSidebarTab(tabs[next])
          buttons[next]?.focus()
        }}>
          {(['transcript', 'summary', 'contents'] as const).map((tab) => <button type="button" role="tab" key={tab} id={`sidebar-${tab}-tab`} aria-controls={`sidebar-${tab}-panel`} aria-selected={sidebarTab === tab} tabIndex={sidebarTab === tab ? 0 : -1} onClick={() => setSidebarTab(tab)}>{tab === 'contents' ? t.contents.title : t.workbench[tab]}</button>)}
        </div>
        <div id="sidebar-summary-panel" className="sidebar-panel" role="tabpanel" aria-labelledby="sidebar-summary-tab" hidden={sidebarTab !== 'summary'}>
        {!summing && !summary && !summaryCandidate && !clearedSummary && <p className="empty">{t.contents.summaryEmpty}</p>}
        {replay && replay.at < replay.total && <p className="empty">{t.contents.summaryReplay}</p>}
        {/* Hidden while replaying, back once the replay reaches the end (e.g. a finished sample meeting) */}
        {(summing || summary || summaryCandidate || clearedSummary) && (!replay || replay.at >= replay.total) && (
          <Suspense fallback={null}>
            <Summary
              text={summary}
              readOnly={!!replay || transitionBusy}
              onChange={editSummary}
              busy={!!summing}
              status={summing ?? ''}
              onClear={clearSummary}
              stale={!!summary && !!summarySource && summarySource !== currentSummarySource}
              pending={summaryCandidate ? { text: summaryCandidate.text, stale: summaryCandidate.source !== currentSummarySource } : undefined}
              canUndoClear={!!clearedSummary}
              onUndoClear={undoClearSummary}
              onUseGenerated={useGeneratedSummary}
              onKeepCurrent={() => { if (!replayRef.current && !transitionRef.current) setSummaryCandidate(null) }}
              onToast={setToast}
            />
          </Suspense>
        )}
        </div>
        <div id="sidebar-contents-panel" className="sidebar-panel" role="tabpanel" aria-labelledby="sidebar-contents-tab" hidden={sidebarTab !== 'contents'}>
          <ContentList key={`${meeting.id}:${page}`} graph={graph} scene={contentScene} page={page} sourcePages={sourcePages} blocked={!!replay || transitionBusy} readOnly={!!replay || transitionBusy}
            onPage={goPage} onLocate={(selection) => locateContent({ selection })}
            onRename={(selection, text) => applyEdit(renameSel(session.graph, selection, text), t.app.renamedNote(describe(session.graph, selection, t.doc), text.trim()))}
            onDelete={(selection) => applyEdit(removeSel(session.graph, selection), t.app.deletedNote(describe(session.graph, selection, t.doc)))}
            onSource={jumpToRaised} onEditFree={updateFreeObject}
            onLocateFree={(id) => { const element = contentScene?.elements.find((e) => e.id === id); if (element) locateContent({ box: { x: element.x, y: element.y, w: element.width, h: element.height } }) }} />
        </div>
        <div id="sidebar-transcript-panel" className="sidebar-panel" role="tabpanel" aria-labelledby="sidebar-transcript-tab" hidden={sidebarTab !== 'transcript'}>
        <label className="transcript-announcement"><input type="checkbox" checked={announceTranscript} onChange={(event) => { setAnnounceTranscript(event.target.checked); save('boardcast.announce-transcript', event.target.checked ? 'on' : 'off') }} />{t.workbench.announceTranscript}</label>
        {/* 悬停的那句可能在鼠标底下消失（回放往回跳），移出整个列表时总归取消溯源 */}
        {!followTranscript && <button type="button" className="follow-transcript" onClick={() => { setJumpLine(null); followLatest(true) }}>{unreadLines > 0 && `${t.workbench.newTranscript(unreadLines)} · `}{t.workbench.followLatest}</button>}
        <div className="lines" ref={lineList} tabIndex={0} onMouseLeave={() => setTraceWho(null)} onKeyDown={(event) => {
          if (['ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown', ' '].includes(event.key)) event.stopPropagation()
          if (['ArrowUp', 'Home', 'PageUp'].includes(event.key)) followLatest(false)
        }}>
          {lines.length === 0 && !partial && <p className="empty">{t.app.logEmpty}</p>}
          {shown.map((l, i) => {
            const processing = l.ms === undefined && (session.isProcessing(l) || pageSessions.current.get(l.page ?? 0)?.isProcessing(l))
            const state = l.failed ? t.app.lineFailed : l.ms === undefined ? (processing ? t.app.drawing : t.app.lineQueued)
              : l.ops === '-' ? t.app.lineUnchanged : l.ops ? t.app.lineDrawn : t.app.lineProcessed
            const failure = l.boardFailure === 'unavailable' ? t.app.lineModelUnavailable : l.boardFailure === 'cancelled' ? t.app.lineCancelled : t.app.lineFailedDetail
            return (
            <Fragment key={i}>
            {pagesRef.current.length > 1 && (i === 0 || (shown[i - 1].page ?? 0) !== (l.page ?? 0)) && (
              <button type="button" className="page-mark" onClick={() => goPage(l.page ?? 0)} disabled={!!replay || waiting} title={t.app.gotoPage}>
                {t.app.pageN((l.page ?? 0) + 1)}
              </button>
            )}
            <article
              className={`line${l.ms === undefined ? ' pending' : ''}${l.failed ? ' failed' : ''}${l.manual ? ' manual' : ''}${replay && i === shown.length - 1 ? ' now' : ''}${jumpLine?.index === i ? ' jump' : ''}`}
            >
              {l.manual && <span className="who edit">{t.app.editBadge}</span>}
              {l.speaker && (
                <button type="button"
                  className={`who c${speakerSlot(speakers, l.speaker)}`}
                  disabled={!!replay || transitionBusy}
                  title={t.app.renameSpeakerTitle(l.speaker)}
                  aria-label={t.app.renameSpeakerTitle(l.speaker)}
                  onClick={() => setRenamingWho(l.speaker!)}
                  onMouseEnter={() => setTraceWho(l.speaker!)}
                  onMouseLeave={() => setTraceWho(null)}
                  onFocus={() => setTraceWho(l.speaker!)}
                  onBlur={() => setTraceWho(null)}
                >
                  {initial(l.speaker)}
                </button>
              )}
              <div className="body">
                {!l.manual && l.speaker && (
                  <div className="head">
                      <span className="name" onMouseEnter={() => setTraceWho(l.speaker!)} onMouseLeave={() => setTraceWho(null)}>
                        {l.speaker}
                      </span>
                  </div>
                )}
                <p title={l.rawText ? `${t.app.originalText}: ${l.rawText}` : undefined}>
                  {l.text}
                  {l.fix && (
                    <span className="fix-tag" title={t.app.fixTitle}>
                      {t.app.fixBadge}
                    </span>
                  )}
                  {l.pointing?.length ? (
                    <span className="point-tag" title={t.app.pointedTitle}>
                      {t.app.pointedAt(l.pointing.map((id) => nameOn(l.page ?? 0, id)).join(t.app.listSep))}
                    </span>
                  ) : null}
                </p>
                {l.failed && !l.manual && (
                  <div className="line-recovery" role="status">
                    <span>{l.boardFailure === 'unavailable' ? t.app.lineModelUnavailable : l.boardFailure === 'cancelled' ? t.app.lineCancelled : t.app.lineRecordedNotDrawn}</span>
                    <button type="button" className="ghost small" disabled={transitionBusy || !!replay} onClick={() => retryLine(i)}>{t.app.retryLine}</button>
                    {l.boardFailure === 'unavailable' && <button type="button" className="ghost small" onClick={() => setShowSettings('models')}>{t.app.configureBoard}</button>}
                  </div>
                )}
                {!l.manual && (
                <details>
                  <summary>
                    {l.failed ? <em>{state}</em> : state}
                    {l.ms !== undefined && !l.failed && l.via && ` · ${l.via}`}
                    {l.ms !== undefined && !l.failed && l.fellBack && <em>{t.app.lineFellBackShort}</em>}
                  </summary>
                  {l.ms !== undefined && !l.failed && l.fellBack && <p className="timing warn">{t.app.lineFellBack.replace(/^ · /, '')}</p>}
                  {l.ms !== undefined && !l.failed && <p className="timing">{t.app.timing(lineTiming(l))}</p>}
                  {/* 空串：和后面几句攒成一批画的，画了什么记在那批的最后一句上；'-'：这一轮没改板书 */}
                  <pre>{l.failed ? failure : l.ms === undefined ? state : l.ops === '' ? t.app.drawnWithNext : l.ops === '-' ? t.app.noChange : l.ops ?? t.app.lineDetailsUnavailable}</pre>
                </details>
                )}
              </div>
            </article>
            </Fragment>
          )})}
          {partial && !replay && (
            <article className="line partial" aria-live="off">
              {speaker.trim() && <span className={`who c${speakerSlot(speakers, speaker.trim())}`}>{initial(speaker.trim())}</span>}
              <div className="body">
                <div className="head">{speaker.trim() && <span className="name">{speaker.trim()}</span>}</div>
                <p>{partial}…</p>
              </div>
            </article>
          )}
        </div>
        </div>
        <label className="sidebar-width">{t.contents.width}<input type="range" min={280} max={400} step={10} value={sidebarWidth} onChange={(event) => { const value = Number(event.target.value); setSidebarWidth(value); save('boardcast.sidebar-width', String(value)) }} /></label>
      </aside>

      {reviewDialog && <Dialog label={t.review.finish} onClose={() => setReviewDialog(false)} dismissible={!reviewStage}>
        <section className="panel review-dialog">
          <h2>{t.review.finish}</h2><p>{t.review.intro}</p>
          <p>{t.review.counts(said.length, pagesRef.current.length, queuedLines + failedLines)}</p>
          {reviewStage && <p role="status">{t.review[reviewStage]}</p>}
          {reviewError && <p role="alert">{reviewError}</p>}
          {!reviewStage && <div className="review-actions">
            <button type="button" onClick={() => void finishForReview(true)}>{queuedLines ? t.review.wait : t.review.finish}</button>
            {queuedLines > 0 && <button type="button" className="secondary" onClick={() => void finishForReview(false)}>{t.review.keep}</button>}
            {reviewError && <button type="button" className="secondary" onClick={() => void exportAs('html')}>{t.workbench.exportCopy}</button>}
            <button type="button" className="ghost" onClick={() => setReviewDialog(false)}>{t.app.cancel}</button>
          </div>}
          {reviewStage === 'waiting' && <button type="button" className="secondary" onClick={() => finishWaiting.current?.()}>{t.review.keep}</button>}
        </section>
      </Dialog>}

      {update && (
        <Suspense fallback={null}>
          <UpdatePanel key={update.version} info={update} open={showUpdate} onBeforeInstall={saveBeforeMaintenance} onClose={() => setShowUpdate(false)} />
        </Suspense>
      )}

      {showPreparation && <Suspense fallback={null}>
        <Preparation
          source={voiceSource}
          setSource={(source) => changeAudio(micChoice, source)}
          mic={micChoice}
          setMic={(device) => changeAudio(device, voiceSource)}
          boardReady={ready}
          boardLabel={status}
          localOnly={localOnly || !cloud}
          audioBusy={!!micPending}
          voiceError={voiceErr}
          onStart={() => startMic()}
          onCancelStart={stopMic}
          onManageSpeech={() => setShowModels('speech')}
          onManageBoard={() => setShowSettings('models')}
          onType={() => {
            document.querySelector<HTMLButtonElement>('.dock .write.idle')?.click()
            requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>('.dock textarea')?.focus())
          }}
          onClose={() => setShowPreparation(false)}
        />
      </Suspense>}

      {showSettings && (
        <SettingsPanel
          initialSection={showSettings === 'models' ? 'models' : undefined}
          onClose={() => setShowSettings(false)}
          hotkey={hotkey}
          setHotkey={changeHotkey}
          hotkeyErr={hotkeyErr}
          hotkeyBusy={hotkeyBusy}
          onKeys={() => {
            setShowKeys(true)
          }}
          onDiagnostics={() => {
            setShowDiag(true)
          }}
          builtin={modelStatus}
          warming={warming}
          localUrl={baseUrl}
          setLocalUrl={setBaseUrl}
          localOnline={online}
          onManageModels={() => {
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
          setMic={(device) => changeAudio(device, voiceSource)}
          source={voiceSource}
          setSource={(source) => changeAudio(micChoice, source)}
          audioBusy={!!micPending}
          diarize={diarize}
          setDiarize={setDiarize}
          autoUpdate={autoUpdate}
          setAutoUpdate={setAutoUpdate}
          onUpdate={(u) => {
            setUpdate(u)
            setShowSettings(false)
            setShowUpdate(true)
          }}
          theme={theme}
          setTheme={setTheme}
          inkMode={mode === 'ink'}
          messy={messy}
          setMessy={setMessy}
          weight={weight}
          setWeight={setWeight}
          englishWeight={englishWeight}
          setEnglishWeight={setEnglishWeight}
          countSamples={async () => gather(await allFixes()).length}
          exportSamples={exportFixes}
        />
      )}
      {showModels && <ModelPanel status={modelStatus} speechOnly={showModels === 'speech'} onBeforeRemove={saveBeforeMaintenance} onClose={() => setShowModels(false)} />}
      {renamingWho && <SpeakersDialog initial={renamingWho}
        names={[...new Set([...lines.flatMap((line) => line.speaker ? [line.speaker] : []), ...voicesRef.current.map((voice) => voice.name), ...(speaker.trim() ? [speaker.trim()] : []), renamingWho])]}
        lines={lines} boards={pagesRef.current.map((p, i) => i === pageRef.current ? session.graph : p.graph)} readOnly={!!replay || transitionBusy}
        onRename={renameSpeaker} onChoose={(name) => { speakerRef.current = name; setSpeaker(name) }} onClose={() => setRenamingWho(null)} />}
      {showMeetingTitle && <MeetingTitleDialog title={meeting.title} onSave={(title) => renameMeeting(meeting.id, title)} onClose={() => setShowMeetingTitle(false)} />}
      {showMeetings && (
        <MeetingsPanel
          currentId={meeting.id}
          current={libraryCurrent}
          getFailure={() => transitionError.current}
          onClose={() => setShowMeetings(false)}
          onOpen={openOther}
          onNew={(keep) => reset(keep)}
          onDeleteCurrent={deleteCurrentMeeting}
          onRename={renameMeeting}
          onReplay={(id) => openFromLibrary(id, 'replay')}
          onExport={(id) => openFromLibrary(id, 'export')}
          boardsOf={boardsOf}
          onContinue={continueOn}
        />
      )}
      {importTask && !showSettings && !showModels && <Suspense fallback={null}><ImportTaskPanel
        task={importTask} busy={transitionBusy} saveError={importSaveError}
        onCancel={cancelImport}
        onRetry={() => {
          const source = importWork.current?.source
          if (typeof source === 'string') void importArxiv(source)
          else if (source) void importFile(source)
        }}
        onConfigure={() => isDesktop ? setShowModels(true) : setShowSettings('models')}
        onCommit={(fresh) => { void placeImport(fresh) }}
      /></Suspense>}
      {showKeys && (
        <Dialog label={t.app.keys} onClose={() => setShowKeys(false)}>
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
                      <kbd>{shortcutLabel(k, navigator.platform)}</kbd>
                    </td>
                    <td>{what}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="hint">{t.app.keysHint}</p>
          </div>
        </Dialog>
      )}
      {showDiag && (
        <DiagnosticsPanel
          voice={{ listening, device: mic ?? null, choice: micChoice ?? null, source: voiceSource, diarize, error: voiceErr }}
          timing={timingReport(lines, llm.label)}
          onClose={() => setShowDiag(false)}
          onSaved={(ok, text) => setToast({ ok, text })}
        />
      )}
      {confirmSwitch && <Dialog label={t.workbench.switchTitle} role="alertdialog" onClose={() => pendingSwitch.current?.(false)}>
        <section className="panel review-panel">
          <h2>{t.workbench.switchTitle}</h2>
          <p>{t.workbench.switchNotice(meeting.title)}</p>
          <div className="review-actions">
            <button type="button" className="secondary" autoFocus onClick={() => pendingSwitch.current?.(false)}>{t.workbench.stayHere}</button>
            <button type="button" onClick={() => pendingSwitch.current?.(true)}>{t.workbench.stopAndSwitch}</button>
          </div>
        </section>
      </Dialog>}
      {toast && (
        <div className={`toast${toast.ok ? '' : ' err'}`} role="status">
          <span>{toast.text}</span>
          {(toast.path || toast.retryExport) && <div className="toast-actions">
            {toast.path && <>
            <button type="button" className="secondary small" disabled={openingExport} onClick={() => void openExport(false)}>{t.app.openExport}</button>
            <button type="button" className="secondary small" disabled={openingExport} onClick={() => void openExport(true)}>{t.app.revealExport}</button>
            </>}
            {toast.retryExport && <button type="button" className="secondary small" disabled={exporting} onClick={() => void exportAs(toast.retryExport!)}>{t.app.exportAgain}</button>}
            <button type="button" className="ghost small" onClick={() => setToast(null)}>{t.app.close}</button>
            {openingExport && <span>{t.app.exportOpening}</span>}
          </div>}
          {!toast.ok && (
            <button
              type="button"
              className="ghost small"
              onClick={() => {
                void navigator.clipboard
                  ?.writeText(`${toast.text}\n\n${recentErrors().slice(-5).join('\n')}\n${navigator.userAgent}`)
                  .then(() => setToast({ ...toast, ok: true, text: t.app.copied }))
                  .catch((error) => setToast({ ...toast, ok: false, text: errText(error) }))
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
