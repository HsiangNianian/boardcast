// 板书溯源（#51）：墨线上的节点和连线记下是谁提的（`by`，说话人的名字）。平时不显示，
// 悬停会议记录里的说话人时，白板上标出这个人提的东西；选中一条时能看到是谁提的、跳回那句话。
//
// 模型一轮处理的一批话里只有一个人说话，就是他；几个人的话攒成一批时，看谁的话和名字最像。

import { type Edge, type Graph, cloneGraph, norm } from './graph.ts'
import type { Utterance } from './prompt.ts'
import type { Step } from './steps.ts'

/** 按两个字一段切开（只有一个字就是它自己） */
function grams(s: string): Set<string> {
  const t = norm(s)
  const out = new Set<string>()
  if (t.length === 1) out.add(t)
  for (let i = 0; i + 1 < t.length; i++) out.add(t.slice(i, i + 2))
  return out
}

/** 一句话和这些名字有多像：名字里有几段两个字的片段在这句话里出现过 */
export function overlap(text: string, names: string[]): number {
  const t = norm(text)
  let n = 0
  for (const name of names) for (const g of grams(name)) if (t.includes(g)) n++
  return n
}

/** 这一批话里，最可能是谁说出了这些名字。只有一个人说话就是他；一样像的算先说的；都对不上、没标说话人就不知道 */
export function whoSaid(batch: Utterance[], names: string[]): string | undefined {
  const who = new Set(batch.flatMap((u) => (u.speaker ? [u.speaker] : [])))
  if (who.size <= 1) return [...who][0]
  let best: string | undefined
  let top = 0
  for (const u of batch) {
    if (!u.speaker) continue
    const s = overlap(u.text, names)
    if (s > top) {
      top = s
      best = u.speaker
    }
  }
  return best
}

const edgeKey = (e: Edge) => `${e.from}>${e.to}`

/**
 * 模型这一轮新写成墨线的节点和连线（之前没有，或者还是铅笔稿）记下是谁说的。
 * 以前就有的不动：后来有人改名、打标记、挪位置，提的人还是原来那个。
 */
export function attribute(before: Graph, after: Graph, batch: Utterance[]): Graph {
  const inked = new Set(before.nodes.filter((n) => n.ink === 'ink').map((n) => n.id))
  const inkedEdges = new Set(before.edges.filter((e) => e.ink === 'ink').map(edgeKey))
  const freshNode = (n: Graph['nodes'][number]) => n.ink === 'ink' && !n.by && !inked.has(n.id)
  const freshEdge = (e: Edge) => e.ink === 'ink' && !e.by && !inkedEdges.has(edgeKey(e))
  if (!after.nodes.some(freshNode) && !after.edges.some(freshEdge)) return after
  const g = cloneGraph(after)
  const label = (id: string) => g.nodes.find((n) => n.id === id)?.label ?? ''
  for (const n of g.nodes) {
    if (!freshNode(n)) continue
    const by = whoSaid(batch, [n.label, ...n.aliases])
    if (by) n.by = by
  }
  for (const e of g.edges) {
    if (!freshEdge(e)) continue
    const by = whoSaid(batch, [label(e.from), label(e.to), e.label ?? ''])
    if (by) e.by = by
  }
  return g
}

const has = (xs: { by?: string }[] | undefined, who: string) => !!xs?.some((x) => x.by === who)

/** 说话人改名（或者两个人合成一个）：板书上记的名字跟着改；没有要改的原样返回 */
export function renameBy(g: Graph, from: string, to: string): Graph {
  if (!has(g.nodes, from) && !has(g.edges, from)) return g
  const out = cloneGraph(g)
  for (const x of [...out.nodes, ...out.edges]) if (x.by === from) x.by = to
  return out
}

/** 同上，改回放记录里的 */
export function renameByInSteps(steps: Step[], from: string, to: string): Step[] {
  const fix = <T extends { by?: string }>(xs: T[]) => xs.map((x) => (x.by === from ? { ...x, by: to } : x))
  return steps.map((s) => {
    const { nodes, edges } = s.d
    if (!has(nodes?.set, from) && !has(edges?.set, from)) return s
    const d = { ...s.d }
    if (nodes?.set) d.nodes = { ...nodes, set: fix(nodes.set) }
    if (edges?.set) d.edges = { ...edges, set: fix(edges.set) }
    return { ...s, d }
  })
}

/** 一个节点或一条连线第一次出现在回放记录里时，会议记录有几句；记录里没有就不知道 */
export function firstSeen(steps: Step[], sel: { kind: 'node'; id: string } | { kind: 'edge'; from: string; to: string }): number | undefined {
  for (const s of steps) {
    const hit =
      sel.kind === 'node'
        ? s.d.nodes?.set?.some((n) => n.id === sel.id)
        : s.d.edges?.set?.some((e) => e.from === sel.from && e.to === sel.to)
    if (hit) return s.line
  }
  return undefined
}

/**
 * 这条板书是会议记录里的哪一句：`who` 在这张板上说的话里，和名字最像的那句。
 * 知道它是会议记录有几句时上的板（`before`），就只在那之前找；一样像的取最靠后的，也就是促成它上板的那句。
 */
export function saidAt(
  lines: { text: string; speaker?: string; manual?: boolean; page?: number }[],
  who: string,
  names: string[],
  page: number,
  before = lines.length,
): number | undefined {
  let best: number | undefined
  let top = -1
  for (let i = 0; i < Math.min(before, lines.length); i++) {
    const l = lines[i]
    if (l.manual || l.speaker !== who || (l.page ?? 0) !== page) continue
    const s = overlap(l.text, names)
    if (s >= top) {
      top = s
      best = i
    }
  }
  return best
}
