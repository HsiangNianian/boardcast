// 动态 SVG（#53）：最终那张板书，按会上画出来的先后一笔一笔写出来，写完停在全貌。
// 和白板上写字是同一个办法：每一笔一个遮罩，骨架线的 stroke-dashoffset 从头描到尾，露出底下的墨迹。
// 只用 CSS 动画、不带脚本，所以贴进 GitHub README、文档、群聊里的 <img> 也会自己播。

import type { Layout } from '../board/layout.ts'
import { type Graph, MARKS, type Mark, type Node } from '../engine/graph.ts'
import type { Step } from '../engine/steps.ts'
import type { Piece } from './compose.ts'

/** 和白板上一样的笔速：像素每毫秒；每笔最短多久、笔与笔之间停多久 */
const SPEED = 0.9
const MIN_STROKE = 45
const GAP = 35
/** 会上每画一轮之间多停一下，看得出节奏 */
const PAUSE = 300
/** 整张板最多写多久：会开了十分钟还是一小时，都压到这么长 */
export const TOTAL = 12000

/** 一块墨迹属于哪个节点、连线（`from>to`）、分组或区域：看 compose 给的 key 的开头 */
export function ownerOf(key: string): string {
  const head = key.split('|')[0]
  const edge = /^(n\d+)[>~](n\d+)l?$/.exec(head)
  if (edge) return `${edge[1]}>${edge[2]}`
  // 分组名的 key 是「g3t|名字」
  if (/^g\d+t$/.test(head)) return head.slice(0, -1)
  return head
}

/** 标记的笔画（划掉、下划线、圈出、星标）在 key 末尾带一个字母；原文上圈画的词句，key 的第二段是标记本身 */
const MARK_SUFFIX: Record<string, Mark> = { x: '划掉', u: '下划线', o: '圈出', s: '星标' }

function markOf(key: string): Mark | undefined {
  const parts = key.split('|')
  if (parts.length >= 2 && (MARKS as readonly string[]).includes(parts[1])) return parts[1] as Mark
  const last = parts[parts.length - 1]
  return parts.length >= 2 ? MARK_SUFFIX[last] : undefined
}

/**
 * 每块墨迹在会上第几轮出现（越小越早）。按回放记录：节点、连线、分组第一次出现的那一轮；
 * 标记按它被加上的那一轮。没有记录的（老会议）按编号先后排在最后。
 */
export function ranks(g: Graph, l: Layout, steps: Step[], keys: string[]): number[] {
  const first = new Map<string, number>()
  const marked = new Map<string, number>()
  steps.forEach((s, i) => {
    for (const n of s.d.nodes?.set ?? []) {
      if (!first.has(n.id)) first.set(n.id, i)
      for (const m of n.marks) if (!marked.has(`${n.id}|${m}`)) marked.set(`${n.id}|${m}`, i)
      for (const sp of n.spans ?? []) if (!marked.has(`${n.id}|${sp.mark}`)) marked.set(`${n.id}|${sp.mark}`, i)
    }
    for (const e of s.d.edges?.set ?? []) if (!first.has(`${e.from}>${e.to}`)) first.set(`${e.from}>${e.to}`, i)
    for (const x of s.d.groups?.set ?? []) if (!first.has(x.id)) first.set(x.id, i)
  })
  const late = steps.length
  const num = (id: string) => Number(/\d+/.exec(id)?.[0] ?? 0)
  const node = (id: string) => first.get(id) ?? late + num(id)
  const byId = new Map<string, Node>(g.nodes.map((n) => [n.id, n]))
  const rankOf = (owner: string): number => {
    if (first.has(owner)) return first.get(owner)!
    if (byId.has(owner)) return node(owner)
    const edge = /^(n\d+)>(n\d+)$/.exec(owner)
    // 连线最早也得等两头都在
    if (edge) return Math.max(node(edge[1]), node(edge[2]))
    // 分组跟着最早进组的那个；没有名字的区域（「待定」）跟着它里面最早的一条
    const members = g.nodes.filter((n) => n.group === owner).map((n) => node(n.id))
    if (members.length) return Math.min(...members)
    const rows = l.regions.find((r) => r.id === owner)?.rows.map((r) => node(r.node.id)) ?? []
    return rows.length ? Math.min(...rows) : late + 1e6
  }
  return keys.map((key) => {
    const owner = ownerOf(key)
    const base = rankOf(owner)
    const m = byId.has(owner) ? markOf(key) : undefined
    return m ? Math.max(base, marked.get(`${owner}|${m}`) ?? base) : base
  })
}

export interface Timed {
  piece: Piece
  /** 每一笔从第几毫秒开始写、写多久 */
  delays: number[]
  durs: number[]
}

/** 排好写字的时间：按先后一笔接一笔，整张板写不完 `total` 毫秒就整体加速 */
export function timeline(pieces: Piece[], rank: number[], total = TOTAL): { timed: Timed[]; end: number } {
  const order = pieces.map((p, i) => ({ p, r: rank[i], i })).sort((a, b) => a.r - b.r || a.i - b.i)
  const pen = order.filter((x) => !x.p.under)
  const turns = new Set(pen.map((x) => x.r)).size
  const natural =
    pen.reduce((t, x) => t + x.p.strokes.reduce((u, s) => u + Math.max(MIN_STROKE, s.length / SPEED) + GAP, 0), 0) + Math.max(0, turns - 1) * PAUSE
  const k = natural > total ? total / natural : 1
  let at = 0
  let last: number | undefined
  const timed: Timed[] = []
  for (const { p, r } of order) {
    // 荧光笔垫底不写，跟着它那一轮直接出现
    if (p.under) {
      timed.push({ piece: p, delays: p.strokes.map(() => at), durs: p.strokes.map(() => 0) })
      continue
    }
    if (last !== undefined && r !== last) at += PAUSE * k
    last = r
    const delays: number[] = []
    const durs: number[] = []
    for (const s of p.strokes) {
      const dur = Math.max(MIN_STROKE, s.length / SPEED) * k
      delays.push(Math.round(at))
      durs.push(Math.round(dur))
      at += dur + GAP * k
    }
    timed.push({ piece: p, delays, durs })
  }
  return { timed, end: Math.round(at) }
}

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;')

/**
 * 拼成一个 SVG 文件。`box` 是裁出来的范围（板书坐标），`paper` 是纸色；
 * `underlay` 是导入页的图（data URL），`size` 是它的大小。
 */
export function animatedSvg(
  timed: Timed[],
  box: { x: number; y: number; w: number; h: number },
  paper: string,
  underlay?: { href: string; w: number; h: number },
): string {
  const { x, y, w, h } = box
  // 遮罩的范围要写明：不写时默认按画布大小从原点算，板书坐标是负数的那部分（上边、左边）会被裁掉
  let n = 0
  const body = timed
    .map(({ piece: p, delays, durs }) => {
      const strokes = p.strokes
        .map((s, j) => {
          if (!durs[j]) return delays[j] ? `<path d="${s.d}" class="pop" style="animation-delay:${delays[j]}ms"/>` : `<path d="${s.d}"/>`
          const id = `m${n++}`
          const len = (s.length + 1).toFixed(1)
          return `<mask id="${id}" maskUnits="userSpaceOnUse" x="${x}" y="${y}" width="${w}" height="${h}"><path class="w" d="${s.center}" stroke-dasharray="${len}" stroke-dashoffset="${len}" style="animation-duration:${durs[j]}ms;animation-delay:${delays[j]}ms"/></mask><path d="${s.d}" mask="url(#${id})"/>`
        })
        .join('')
      return `<g fill="${p.color}" opacity="${p.opacity}"${p.under ? ' style="mix-blend-mode:multiply"' : ''}>${strokes}</g>`
    })
    .join('\n')
  const style = [
    '@keyframes w{to{stroke-dashoffset:0}}',
    '@keyframes pop{from{opacity:0}to{opacity:1}}',
    '.w{fill:none;stroke:#fff;stroke-width:26;stroke-linecap:round;stroke-linejoin:round;animation-name:w;animation-timing-function:linear;animation-fill-mode:both}',
    '.pop{animation:pop 1ms both}',
    // 不想看动画的人直接看全貌
    '@media (prefers-reduced-motion:reduce){.w{animation:none;stroke-dashoffset:0}.pop{animation:none}}',
  ].join('\n')
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${w} ${h}" width="${Math.round(w)}" height="${Math.round(h)}">`,
    `<style>\n${style}\n</style>`,
    `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${paper}"/>`,
    underlay ? `<image href="${esc(underlay.href)}" x="0" y="0" width="${underlay.w}" height="${underlay.h}" preserveAspectRatio="none"/>` : '',
    body,
    '</svg>',
  ].join('\n')
}
