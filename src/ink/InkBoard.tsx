// 手写模式：自己画 SVG，不走 Excalidraw。
// 新出现的东西按笔顺一笔一笔写出来（骨架线做遮罩，stroke-dashoffset 从头描到尾），
// 写过的东西位置变了就直接挪过去，不重写。
// 浅色是白板马克笔，深色是黑板粉笔。

import { type PointerEvent, type ReactNode, type WheelEvent, useEffect, useMemo, useRef, useState } from 'react'
import { type Layout, board } from '../board/layout.ts'
import type { Selection } from '../engine/edit.ts'
import type { Graph } from '../engine/graph.ts'
import { type InkLook, type Piece, type Trace, compose } from './compose.ts'
import { localPoint } from '../ui/landscape.ts'
import { hitTest, selectionBox, selectionLine } from './hit.ts'

export const WHITEBOARD: InkLook = {
  paper: '#fbfbf8',
  ink: '#1d1f24',
  pencil: '#9aa0a6',
  red: '#d02e2e',
  blue: '#1f56c4',
  highlight: '#ffe45c',
  chalk: false,
}

const BLACKBOARD: InkLook = {
  paper: '#22302a',
  ink: '#eeeee6',
  pencil: '#8f9a94',
  red: '#ff9c8a',
  blue: '#9cc9f5',
  highlight: '#f2d75c',
  chalk: true,
}

/** 笔速，像素每毫秒；每笔之间停一下 */
const SPEED = 0.9
const GAP = 35
const MIN_STROKE = 45
/** 一轮新内容最多写多久 */
const BUDGET = 2400
const MAX_BACKLOG = 600

interface Timing {
  delays: number[]
  durs: number[]
  end: number
}

export default function InkBoard({
  graph,
  hot,
  dark,
  messy,
  bottomInset = 0,
  tidy = 0,
  weight = 1,
  selection = null,
  onSelect,
  toolbar,
  onMoveSel,
  layoutRev = 0,
  underlay,
  trace = null,
}: {
  graph: Graph
  hot: Set<string>
  dark: boolean
  messy: number
  /** 画面底部被悬浮控件挡住的高度，自动适配时让开 */
  bottomInset?: number
  /** 每点一次「整理排版」加一：排完以后重新适配画面 */
  tidy?: number
  /** 笔画粗细的倍数 */
  weight?: number
  /** 手动修改：选中的东西、点选回调、浮在选中物上方的按钮 */
  selection?: Selection | null
  onSelect?: (sel: Selection | null, at?: { x: number; y: number }) => void
  toolbar?: ReactNode
  /** 把选中的框或话题拖到了新位置（画板坐标的位移） */
  onMoveSel?: (sel: Selection, dx: number, dy: number) => void
  /** 排版记忆变了（拖动、撤销）就加一：重新排版、重画 */
  layoutRev?: number
  /** 垫在底下的导入页（PDF 的一页、一张图片），左上角对齐板书原点，大小是 graph.underlay */
  underlay?: string
  /** 溯源：标出这个说话人提的东西，别的淡下去 */
  trace?: Trace | null
}) {
  const look = dark ? BLACKBOARD : WHITEBOARD
  const [pieces, setPieces] = useState<Piece[]>([])
  /** 当前画面是按哪个潦草度算出来的（测试和调试用） */
  const [drawn, setDrawn] = useState(messy)
  const [bounds, setBounds] = useState({ x0: 0, y0: 0, x1: 800, y1: 600 })
  const [view, setView] = useState<{ x: number; y: number; k: number } | null>(null)
  const [, setTick] = useState(0)
  const timing = useRef(new Map<string, Timing>())
  const cursor = useRef(0)
  const wrap = useRef<HTMLDivElement>(null)
  const drag = useRef<{ x: number; y: number } | null>(null)
  /** 这次按下以后有没有拖动：没拖动就算点了一下，用来点选 */
  const press = useRef<{ x: number; y: number; moved: boolean } | null>(null)
  /** 正在拖动选中的东西：按下时的画板坐标，和目前的位移 */
  const moveFrom = useRef<{ x: number; y: number } | null>(null)
  const [moving, setMoving] = useState<{ dx: number; dy: number } | null>(null)
  /** 最近一次的排版：点选判定、画高亮框要用 */
  const [lay, setLay] = useState<Layout | null>(null)
  /** 现在画面上的视角和画布尺寸，排完新内容时判断要不要重新适配 */
  const shown = useRef<{ x: number; y: number; k: number } | null>(null)
  const size = useRef<[number, number]>([1000, 700])
  const tidied = useRef(tidy)
  /** 上一次板上有没有东西：从空白板开始画第一笔时总是适配一次 */
  const had = useRef(false)
  /** 上一次是照着什么画的：只是溯源换了人，板书没变，就不动视角 */
  const drewFor = useRef<unknown[]>([])

  useEffect(() => {
    let stale = false
    void (async () => {
      const l = await board.layout(graph)
      const ps = await compose(graph, l, hot, look, messy, weight, trace)
      if (stale) return
      const from = [graph, hot, look, messy, weight, tidy, bottomInset, layoutRev]
      const restyled = from.every((x, i) => x === drewFor.current[i])
      drewFor.current = from
      setLay(l)
      // 给第一次出现的笔画排上写字的时间：接在正在写的东西后面。
      // 板书不能落后于讨论：一轮新内容最多写 BUDGET 毫秒，写不完就整体加速；
      // 上一轮还没写完时，最多再等 MAX_BACKLOG 就开始写这一轮。
      const now = performance.now()
      cursor.current = Math.min(Math.max(cursor.current, now), now + MAX_BACKLOG)
      const fresh = ps.filter((p) => !timing.current.has(p.key))
      const natural = fresh
        .filter((p) => !p.under)
        .reduce((t, p) => t + p.strokes.reduce((u, s) => u + Math.max(MIN_STROKE, s.length / SPEED) + GAP, 0), 0)
      const k = natural > BUDGET ? BUDGET / natural : 1
      for (const p of fresh) {
        if (p.under) {
          timing.current.set(p.key, { delays: p.strokes.map(() => 0), durs: p.strokes.map(() => 0), end: now })
          continue
        }
        const delays: number[] = []
        const durs: number[] = []
        for (const s of p.strokes) {
          const dur = Math.max(MIN_STROKE, s.length / SPEED) * k
          delays.push(cursor.current - now)
          durs.push(dur)
          cursor.current += dur + GAP * k
        }
        timing.current.set(p.key, { delays, durs, end: cursor.current })
      }
      setPieces(ps)
      setDrawn(messy)
      if (!restyled) {
        setBounds(l.bounds)
        // 新内容还在画面里就不动视角，免得每说一句整块板缩放一次；跑出画面或者刚整理过才重新适配
        const cur = shown.current
        const b = l.bounds
        const [w, h] = size.current
        const inside =
          cur &&
          b.x0 >= cur.x &&
          b.y0 >= cur.y &&
          b.x1 <= cur.x + w / cur.k &&
          b.y1 <= cur.y + Math.max(120, h - bottomInset) / cur.k
        setView(inside && had.current && tidied.current === tidy ? cur : null)
        tidied.current = tidy
        had.current = graph.nodes.length > 0
      }
      // 写完以后去掉遮罩，换成静态路径
      const wait = cursor.current - now + 100
      setTimeout(() => !stale && setTick((t) => t + 1), wait)
    })()
    return () => {
      stale = true
    }
    // trace 按内容比：每次渲染传进来的都是新对象也不会重画
  }, [graph, hot, look, messy, weight, tidy, bottomInset, layoutRev, trace?.who, trace?.color])

  // 画布尺寸放进 state：第一次渲染时 ref 还没挂上，窗口缩放时也要重新适配
  const [[W, H], setSize] = useState<[number, number]>([1000, 700])
  useEffect(() => {
    const el = wrap.current
    if (!el) return
    const ro = new ResizeObserver(() => setSize([el.clientWidth || 1000, el.clientHeight || 700]))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // 自动适配画面：内容少时不放大到 1 倍以上；底下留出悬浮输入框的位置
  const fit = useMemo(() => {
    const free = Math.max(120, H - bottomInset)
    const w = Math.max(1, bounds.x1 - bounds.x0 + 120)
    const h = Math.max(1, bounds.y1 - bounds.y0 + 120)
    const k = Math.min(1, W / w, free / h)
    return { k, x: (bounds.x0 + bounds.x1) / 2 - W / (2 * k), y: (bounds.y0 + bounds.y1) / 2 - free / (2 * k) }
  }, [bounds, W, H, bottomInset])
  const v = view ?? fit
  shown.current = v
  size.current = [W, H]

  // 手机上强制横屏时整个应用是转过的，指针位置都先换回画布自己的坐标
  const at = (e: { clientX: number; clientY: number }) => localPoint(wrap.current!, e.clientX, e.clientY)
  const onWheel = (e: WheelEvent) => {
    const [px, py] = at(e)
    const mx = v.x + px / v.k
    const my = v.y + py / v.k
    const k = Math.min(4, Math.max(0.2, v.k * Math.exp(-e.deltaY * 0.0015)))
    setView({ k, x: mx - px / k, y: my - py / k })
  }
  const onDown = (e: PointerEvent) => {
    // 浮动按钮上的点击不算点白板
    if ((e.target as Element).closest('.ink-toolbar')) return
    const [x, y] = at(e)
    ;(e.target as Element).setPointerCapture(e.pointerId)
    // 在已经选中的框或话题上按下去：拖它，而不是平移画面
    const wx = v.x + x / v.k
    const wy = v.y + y / v.k
    if (selection?.kind === 'node' && onMoveSel && lay && board.movable(selection.id)) {
      const hit = hitTest(lay, graph, wx, wy, 8 / v.k)
      if (hit?.kind === 'node' && hit.id === selection.id) {
        moveFrom.current = { x: wx, y: wy }
        setMoving({ dx: 0, dy: 0 })
        return
      }
    }
    drag.current = { x, y }
    press.current = { x, y, moved: false }
  }
  const onUp = (e: PointerEvent) => {
    if (moveFrom.current) {
      const m = moving
      moveFrom.current = null
      setMoving(null)
      if (m && selection && Math.hypot(m.dx, m.dy) * v.k > 3) onMoveSel?.(selection, m.dx, m.dy)
      return
    }
    drag.current = null
    const p = press.current
    press.current = null
    if (!p || p.moved || !onSelect || !lay) return
    // 点了一下（没拖动）：看点中了什么；点空白就取消选中
    const [x, y] = at(e)
    onSelect(hitTest(lay, graph, v.x + x / v.k, v.y + y / v.k, 8 / v.k), { x: v.x + x / v.k, y: v.y + y / v.k })
  }
  const onMove = (e: PointerEvent) => {
    if (moveFrom.current) {
      const [x, y] = at(e)
      setMoving({ dx: v.x + x / v.k - moveFrom.current.x, dy: v.y + y / v.k - moveFrom.current.y })
      return
    }
    if (!drag.current) return
    const [x, y] = at(e)
    if (press.current && Math.hypot(x - press.current.x, y - press.current.y) > 4) press.current.moved = true
    const dx = (x - drag.current.x) / v.k
    const dy = (y - drag.current.y) / v.k
    drag.current = { x, y }
    setView({ ...v, x: v.x - dx, y: v.y - dy })
  }

  const moved = useMemo(() => {
    if (!moving || selection?.kind !== 'node') return null
    const ids = new Set([selection.id])
    // 话题：下面挂的条目、问题、备注都跟着走
    for (let grew = true; grew; ) {
      grew = false
      for (const n of graph.nodes)
        if (n.parent && ids.has(n.parent) && !ids.has(n.id)) {
          ids.add(n.id)
          grew = true
        }
    }
    return ids
  }, [moving, selection, graph])
  const follows = (key: string) => !!moved && [...moved].some((id) => key === id || key.startsWith(`${id}|`) || key.startsWith(`${id}pre`))
  const offset = moving ? `translate(${moving.dx} ${moving.dy})` : undefined

  const selBox0 = selection && lay ? selectionBox(lay, selection) : undefined
  const selBox = selBox0 && moving ? { ...selBox0, x: selBox0.x + moving.dx, y: selBox0.y + moving.dy } : selBox0
  const selLine = selection && lay ? selectionLine(lay, selection) : undefined

  const now = performance.now()
  const render = (p: Piece, i: number) => (
    <g key={p.key} data-key={p.key} transform={follows(p.key) ? offset : undefined} opacity={p.opacity} fill={p.color} style={p.under ? { mixBlendMode: dark ? 'screen' : 'multiply' } : undefined}>
      {p.strokes.map((s, j) => {
        const t = timing.current.get(p.key)
        const live = t && t.end > now && t.durs[j] > 0
        if (!live) return <path key={j} d={s.d} />
        const id = `w${i}-${j}-${p.key.length}`
        return (
          <g key={j}>
            <mask id={id} maskUnits="userSpaceOnUse">
              <path
                d={s.center}
                fill="none"
                stroke="#fff"
                strokeWidth={26}
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeDasharray={s.length + 1}
                strokeDashoffset={s.length + 1}
                style={{ animation: `bc-write ${t.durs[j]}ms linear ${t.delays[j]}ms forwards` }}
              />
            </mask>
            <path d={s.d} mask={`url(#${id})`} />
          </g>
        )
      })}
    </g>
  )

  return (
    <div
      ref={wrap}
      className="ink"
      style={{ background: look.paper }}
      onWheel={onWheel}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onDoubleClick={() => setView(null)}
    >
      <svg width="100%" height="100%" viewBox={`${v.x} ${v.y} ${W / v.k} ${H / v.k}`} data-messy={drawn}>
        <defs>
          {/* 滤镜的参数都是画板坐标，画面缩放后要换算回屏幕像素：
              颗粒比一个像素还细时会变成满屏噪点，抖动超过笔画宽度时笔画会散掉，两者叠在一起字就糊了。
              所以按当前缩放 v.k 算：颗粒固定在屏幕上约 2.5 像素一颗，抖动不超过笔画宽度的四成。 */}
          {/* 粉笔：边缘抖一点，再挖掉一些颗粒 */}
          <filter id="chalk" x="-5%" y="-5%" width="110%" height="110%" colorInterpolationFilters="sRGB">
            <feTurbulence type="fractalNoise" baseFrequency={(0.4 * v.k).toFixed(3)} numOctaves="2" seed="7" result="n" />
            <feDisplacementMap in="SourceGraphic" in2="n" scale={Math.min(0.8, 0.9 / v.k).toFixed(2)} result="d" />
            <feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -1.5 1.55" result="grain" />
            <feComposite in="d" in2="grain" operator="in" />
          </filter>
          {/* 马克笔：墨水边缘稍微洇开 */}
          <filter id="marker" x="-5%" y="-5%" width="110%" height="110%" colorInterpolationFilters="sRGB">
            <feTurbulence type="fractalNoise" baseFrequency={(0.45 * v.k).toFixed(3)} numOctaves="1" seed="3" result="n" />
            <feDisplacementMap in="SourceGraphic" in2="n" scale={Math.min(0.7, 0.7 / v.k).toFixed(2)} />
          </filter>
        </defs>
        {underlay && graph.underlay && (
          <image className="underlay" href={underlay} x={0} y={0} width={graph.underlay.w} height={graph.underlay.h} preserveAspectRatio="none" />
        )}
        <g>{pieces.filter((p) => p.under).map(render)}</g>
        <g filter={`url(#${look.chalk ? 'chalk' : 'marker'})`}>{pieces.filter((p) => !p.under).map(render)}</g>
        {selBox && (
          <g className="ink-selection">
            {selLine ? (
              <polyline points={selLine.map((p) => p.join(',')).join(' ')} />
            ) : (
              <rect x={selBox.x - 8} y={selBox.y - 6} width={selBox.w + 16} height={selBox.h + 12} rx={8} />
            )}
          </g>
        )}
      </svg>
      {selBox && toolbar && !moving && (
        <div
          className="ink-toolbar"
          style={{
            // 按钮条以选中物为中心；靠近边缘时整条往里收，别被裁掉（按钮条大约 300 宽）
            left: Math.max(160, Math.min(W - 160, (selBox.x + selBox.w / 2 - v.x) * v.k)),
            top: Math.max(8, (selBox.y - v.y) * v.k - 12),
          }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          {toolbar}
        </div>
      )}
    </div>
  )
}
