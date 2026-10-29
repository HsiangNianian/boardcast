// 手写模式：自己画 SVG，不走 Excalidraw。
// 新出现的东西按笔顺一笔一笔写出来（骨架线做遮罩，stroke-dashoffset 从头描到尾），
// 写过的东西位置变了就直接挪过去，不重写。
// 浅色是白板马克笔，深色是黑板粉笔。

import { type PointerEvent, type WheelEvent, useEffect, useMemo, useRef, useState } from 'react'
import { board } from '../board/layout.ts'
import type { Graph } from '../engine/graph.ts'
import { type InkLook, type Piece, compose } from './compose.ts'
import { localPoint } from '../ui/landscape.ts'

const WHITEBOARD: InkLook = {
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
}: {
  graph: Graph
  hot: Set<string>
  dark: boolean
  messy: number
  /** 画面底部被悬浮控件挡住的高度，自动适配时让开 */
  bottomInset?: number
  /** 每点一次「整理排版」加一：排完以后重新适配画面 */
  tidy?: number
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
  /** 现在画面上的视角和画布尺寸，排完新内容时判断要不要重新适配 */
  const shown = useRef<{ x: number; y: number; k: number } | null>(null)
  const size = useRef<[number, number]>([1000, 700])
  const tidied = useRef(tidy)
  /** 上一次板上有没有东西：从空白板开始画第一笔时总是适配一次 */
  const had = useRef(false)

  useEffect(() => {
    let stale = false
    void (async () => {
      const l = await board.layout(graph)
      const ps = await compose(graph, l, hot, look, messy)
      if (stale) return
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
      // 写完以后去掉遮罩，换成静态路径
      const wait = cursor.current - now + 100
      setTimeout(() => !stale && setTick((t) => t + 1), wait)
    })()
    return () => {
      stale = true
    }
  }, [graph, hot, look, messy, tidy, bottomInset])

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
    const [x, y] = at(e)
    drag.current = { x, y }
    ;(e.target as Element).setPointerCapture(e.pointerId)
  }
  const onMove = (e: PointerEvent) => {
    if (!drag.current) return
    const [x, y] = at(e)
    const dx = (x - drag.current.x) / v.k
    const dy = (y - drag.current.y) / v.k
    drag.current = { x, y }
    setView({ ...v, x: v.x - dx, y: v.y - dy })
  }

  const now = performance.now()
  const render = (p: Piece, i: number) => (
    <g key={p.key} opacity={p.opacity} fill={p.color} style={p.under ? { mixBlendMode: dark ? 'screen' : 'multiply' } : undefined}>
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
      onPointerUp={() => (drag.current = null)}
      onDoubleClick={() => setView(null)}
    >
      <svg width="100%" height="100%" viewBox={`${v.x} ${v.y} ${W / v.k} ${H / v.k}`} data-messy={drawn}>
        <defs>
          {/* 粉笔：边缘抖一点，再挖掉一些颗粒 */}
          <filter id="chalk" x="-5%" y="-5%" width="110%" height="110%">
            <feTurbulence type="fractalNoise" baseFrequency="1.2" numOctaves="2" seed="7" result="n" />
            <feDisplacementMap in="SourceGraphic" in2="n" scale="2.2" result="d" />
            <feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -2.2 1.7" result="grain" />
            <feComposite in="d" in2="grain" operator="in" />
          </filter>
          {/* 马克笔：墨水边缘稍微洇开 */}
          <filter id="marker" x="-5%" y="-5%" width="110%" height="110%">
            <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="1" seed="3" result="n" />
            <feDisplacementMap in="SourceGraphic" in2="n" scale="0.9" />
          </filter>
        </defs>
        <g>{pieces.filter((p) => p.under).map(render)}</g>
        <g filter={`url(#${look.chalk ? 'chalk' : 'marker'})`}>{pieces.filter((p) => !p.under).map(render)}</g>
      </svg>
    </div>
  )
}
