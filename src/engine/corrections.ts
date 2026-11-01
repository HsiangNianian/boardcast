// 手动修板记成纠错样本（#52）：模型刚画完一轮，用户紧接着在手写板上改了它画的东西
// （删掉一条线、改个名字、划掉、加星标、撤销整轮），就把这一轮打包成一条：
// 模型看到了什么（板书、前文、这一句）、它画了什么、用户觉得应该是什么样。
// 攒下来可以导出成评测用例（eval/cases.json 的格式，从当时的板书接着跑）
// 或者偏好数据（DPO 的 prompt / chosen / rejected，chosen 也能直接当 SFT 的答案）。
//
// 只收和这一轮有关的修改：改的东西得是模型这一轮刚画、刚改的，不然算不到它头上；
// 先改了别的，或者模型画的时候有人动了板，这一轮就不收，免得样本里混进和这句话无关的东西。

import type { Term } from './glossary.ts'
import { type Graph, type Mark, type Node, norm } from './graph.ts'
import { type Op, type Ref, eraseDrafts, formatOps, parseOps } from './ops.ts'
import { type Utterance, messages } from './prompt.ts'
import { type Diff, diffGraph, patchGraph } from './steps.ts'

export interface Correction {
  /** 这一轮画完的时间（毫秒时间戳） */
  at: number
  /** 画在第几张板上 */
  page?: number
  /** 模型看到的板书（连同当时的铅笔稿，和发给它的一模一样） */
  seen: Graph
  /** 它看到的前文 */
  context: Utterance[]
  /** 这一轮的话（几句攒成一批时是合起来的那句） */
  said: Utterance
  terms?: Term[]
  /** 模型的原输出 */
  ops: string
  /** 模型这一轮画出来的：和它看到的板书（只看墨线）比。存差异，会议文件不至于越存越大 */
  drew: Diff
  /** 用户改的：和模型画完的比 */
  fix: Diff
  /** 用户做了哪些修改（会议记录里的写法），给人看 */
  edits: string[]
}

/** 模型画完的一轮 */
export interface Round {
  at: number
  page?: number
  seen: Graph
  context: Utterance[]
  said: Utterance
  terms?: Term[]
  ops: Op[]
  /** 这一轮画完以后的板书 */
  drawn: Graph
}

/** 只留墨线（铅笔稿和挂在它下面的都去掉） */
const ink = (g: Graph) => eraseDrafts(g)

const nodeKey = (n: Node) => JSON.stringify([n.label, n.aliases, n.kind, n.parent, n.group, n.marks, n.icon, n.spans?.map((s) => [s.mark, s.text])])

/** 两版板书之间变了的东西：节点 id、连线「from>to」、分组 id。不看编号计数和先后顺序 */
export function changed(a: Graph, b: Graph): Set<string> {
  const out = new Set<string>()
  const diff = <T>(xs: T[], ys: T[], id: (x: T) => string, key: (x: T) => string) => {
    const before = new Map(xs.map((x) => [id(x), key(x)]))
    const after = new Map(ys.map((y) => [id(y), key(y)]))
    for (const [k, v] of before) if (after.get(k) !== v) out.add(k)
    for (const k of after.keys()) if (!before.has(k)) out.add(k)
  }
  diff(a.nodes, b.nodes, (n) => n.id, nodeKey)
  diff(a.edges, b.edges, (e) => `${e.from}>${e.to}`, (e) => e.label ?? '')
  diff(a.groups, b.groups, (x) => x.id, (x) => x.label)
  return out
}

/** 一场会最多留多少条，免得会议文件越存越大 */
const LIMIT = 200
/** 已经在改这一轮了：这么久之内接着改的都算（划掉了错的那条，再去划对的那条） */
const FOLLOW_MS = 60_000

/**
 * 跟着会议走：模型每画完一轮调 `round`，每次手动修改（含撤销、重做）调 `edit`，
 * 换板时调 `reset`，换会议时调 `load`。收下的样本在 `list` 里，跟着会议一起存。
 */
export class Corrections {
  list: Correction[] = []
  /** 最近画完的一轮：改的是它画的东西，就算纠正它 */
  private last?: { round: Round; seen: Graph; drawn: Graph; touched: Set<string> }
  /** `list` 的最后一条还在改，上一次改是什么时候 */
  private open = false
  private editedAt = 0
  /** 最近一版墨线板书：模型画完、手动改完都更新 */
  private known?: Graph

  constructor(list: Correction[] = []) {
    this.list = [...list]
  }

  /** 换了一场会：换成那场会的样本 */
  load(list: Correction[] = []) {
    this.reset()
    this.list = [...list]
  }

  /** 换了一张板（或者板书整个换了）：不再跟之前那一轮 */
  reset() {
    this.close()
    this.known = undefined
  }

  /** 模型画完一轮：上一轮的样本到此为止 */
  round(r: Round) {
    const seen = ink(r.seen)
    const drawn = ink(r.drawn)
    // 模型画的这段时间里有人手动改过板：它看到的不是它落笔的那张，这一轮不收
    const stale = this.known !== undefined && changed(this.known, seen).size > 0
    const touched = changed(seen, drawn)
    this.known = drawn
    // 这一轮什么都没画（「嗯，好的」）：接着跟上一轮，用户这会儿改的还是上一轮画的
    if (!stale && !touched.size) return
    this.close()
    if (!stale) this.last = { round: r, seen, drawn, touched }
  }

  /**
   * 手动改了板书。算是在纠正刚画的那一轮就收下（或者接着改那一条），返回 true。
   * 头一下得改到这一轮画的东西；已经在改了，一分钟之内接着改别处也算（划掉错的，再划对的）
   */
  edit(g: Graph, note: string, at = Date.now()): boolean {
    const before = this.known
    const now = ink(g)
    this.known = now
    const l = this.last
    if (!l || !before) return false
    const hit = [...changed(before, now)]
    // 板书没变（撤销的是一次拖动）
    if (!hit.length) return this.open
    // 改的不是这一轮画的：不算它头上，这一轮也不再收（以后的板书里混着这次无关的修改）
    if (!hit.some((k) => l.touched.has(k)) && !(this.open && at - this.editedAt <= FOLLOW_MS)) {
      this.close()
      return false
    }
    for (const k of hit) l.touched.add(k)
    this.editedAt = at
    const fix = diffGraph(l.drawn, now) ?? {}
    if (this.open) {
      const c = this.list[this.list.length - 1]
      c.fix = fix
      c.edits.push(note)
      // 改回了模型画的样子：这条眼下不算数
      return useful(c)
    }
    const r = l.round
    this.list.push({
      at: r.at,
      ...(r.page !== undefined ? { page: r.page } : {}),
      seen: r.seen,
      context: r.context,
      said: r.said,
      ...(r.terms?.length ? { terms: r.terms } : {}),
      ops: formatOps(r.ops) || '-',
      drew: diffGraph(l.seen, l.drawn) ?? {},
      fix,
      edits: [note],
    })
    if (this.list.length > LIMIT) this.list.shift()
    this.open = true
    return true
  }

  /** 这一轮不再跟了。改来改去又改回模型画的样子，这条就不要 */
  close() {
    if (this.open && !useful(this.list[this.list.length - 1])) this.list.pop()
    this.open = false
    this.last = undefined
  }
}

// —— 导出 ——

/** 模型这一轮画完的样子 */
export function drawnOf(c: Correction): Graph {
  return patchGraph(ink(c.seen), c.drew)
}

/** 用户改完的样子（只有墨线） */
export function fixedOf(c: Correction): Graph {
  return patchGraph(drawnOf(c), c.fix)
}

/** 真改过东西的才算：改完又撤销回去的不算 */
export function useful(c: Correction): boolean {
  return changed(drawnOf(c), fixedOf(c)).size > 0
}

/**
 * 从 `a` 改到 `b` 的操作，写法和模型一样：已有的东西用编号，新画的用名字。
 * 操作表达不了的（去掉标记、移出分组、分组改名）略过。
 */
export function opsBetween(
  a: Graph,
  b: Graph,
  /** 不要写成别名的名字（用户改掉的错名字，改名时自动留成了别名） */
  wrong: string[] = [],
): Op[] {
  const bad = new Set(wrong.map(norm))
  const A = new Map(a.nodes.map((n) => [n.id, n]))
  const B = new Map(b.nodes.map((n) => [n.id, n]))
  const ref = (id: string): Ref => (A.has(id) ? { id } : { name: B.get(id)?.label ?? id })
  const make: Op[] = []
  const edit: Op[] = []
  const edges: Op[] = []
  const marks: Op[] = []
  const drop: Op[] = []

  // 新画的：先画上面的，再画挂在它下面的
  // 有连线的普通节点由连线带出来，省几个字；下面还挂着东西的得先建出来
  const linked = new Set(b.edges.flatMap((e) => [e.from, e.to]))
  for (const n of b.nodes) if (n.parent) linked.delete(n.parent)
  const done = new Set<string>()
  const create = (n: Node) => {
    if (done.has(n.id)) return
    done.add(n.id)
    const parent = n.parent && B.has(n.parent) ? n.parent : undefined
    if (parent && !A.has(parent)) create(B.get(parent)!)
    if (n.kind === 'topic') make.push({ op: 'T', name: n.label })
    else if (n.kind === 'question' || n.kind === 'formula')
      make.push({ op: n.kind === 'question' ? 'Q' : 'F', ...(parent ? { parent: ref(parent) } : {}), text: n.label })
    else if (n.kind === 'note' && parent) make.push({ op: 'K', target: ref(parent), text: n.label })
    else if (parent) make.push({ op: 'L', parent: ref(parent), name: n.label })
    else if (n.kind !== 'region' && !linked.has(n.id)) make.push({ op: 'N', name: n.label })
    for (const al of n.aliases) if (!bad.has(norm(al))) edit.push({ op: 'A', target: { name: n.label }, alias: al })
  }
  for (const n of b.nodes) if (!A.has(n.id)) create(n)

  for (const n of b.nodes) {
    const was = A.get(n.id)
    if (!was) {
      for (const m of n.marks) marks.push({ op: 'M', target: ref(n.id), mark: m })
      for (const s of n.spans ?? []) marks.push({ op: 'M', target: ref(n.id), mark: s.mark, quote: s.text })
      if (n.icon) marks.push({ op: 'I', target: ref(n.id), icon: n.icon })
      continue
    }
    if (norm(was.label) !== norm(n.label)) edit.push({ op: 'R', target: { id: n.id }, name: n.label })
    for (const al of n.aliases)
      if (norm(al) !== norm(was.label) && !bad.has(norm(al)) && !was.aliases.some((x) => norm(x) === norm(al))) edit.push({ op: 'A', target: { id: n.id }, alias: al })
    if (n.kind === 'topic' && was.kind === 'node') edit.push({ op: 'T', name: n.label })
    if (n.parent && n.parent !== was.parent) edit.push({ op: 'L', parent: ref(n.parent), name: n.label })
    for (const m of n.marks) if (!was.marks.includes(m)) marks.push({ op: 'M', target: { id: n.id }, mark: m })
    for (const s of n.spans ?? [])
      if (!was.spans?.some((x) => x.mark === s.mark && x.text === s.text)) marks.push({ op: 'M', target: { id: n.id }, mark: s.mark, quote: s.text })
    if (n.icon && n.icon !== was.icon) marks.push({ op: 'I', target: { id: n.id }, icon: n.icon })
  }

  const key = (e: { from: string; to: string }) => `${e.from}>${e.to}`
  const before = new Map(a.edges.map((e) => [key(e), e]))
  const after = new Set(b.edges.map(key))
  for (const e of b.edges) {
    const was = before.get(key(e))
    if (!was || (e.label && e.label !== was.label)) edges.push({ op: 'E', from: ref(e.from), to: ref(e.to), ...(e.label ? { label: e.label } : {}) })
  }
  for (const e of a.edges) if (!after.has(key(e)) && B.has(e.from) && B.has(e.to)) drop.push({ op: 'X', from: { id: e.from }, to: { id: e.to } })

  // 分组：新进组的成员（按组名，没有就新建）
  const groupOf = (g: Graph, id: string) => g.groups.find((x) => x.id === id)
  const joined = new Map<string, Ref[]>()
  for (const n of b.nodes) {
    if (!n.group) continue
    const grp = groupOf(b, n.group)
    const was = A.get(n.id)
    if (!grp || (was?.group && norm(groupOf(a, was.group)?.label ?? '') === norm(grp.label))) continue
    joined.set(grp.label, [...(joined.get(grp.label) ?? []), ref(n.id)])
  }
  const groups: Op[] = [...joined].map(([group, members]) => ({ op: 'G', group, members }))

  // 删掉的：只删最上面那个，挂在下面的跟着走
  for (const n of a.nodes) if (!B.has(n.id) && !(n.parent && A.has(n.parent) && !B.has(n.parent))) drop.push({ op: 'D', target: { id: n.id } })

  return [...make, ...edit, ...edges, ...groups, ...marks, ...drop]
}

/** 用户改掉的名字：模型画的时候叫这个，用户改成了别的 */
function renamed(drawn: Graph, fixed: Graph): Map<string, string> {
  const D = new Map(drawn.nodes.map((n) => [n.id, n]))
  const out = new Map<string, string>()
  for (const n of fixed.nodes) {
    const d = D.get(n.id)
    if (d && norm(d.label) !== norm(n.label)) out.set(n.id, d.label)
  }
  return out
}

/** 用户想要的这一轮：从模型看到的板书，改到用户改完的样子 */
export function wanted(c: Correction): string {
  return formatOps(opsBetween(ink(c.seen), fixedOf(c), [...renamed(drawnOf(c), fixedOf(c)).values()])) || '-'
}

// 评测用例：和 eval/score.ts 的 Gold 是同一个格式（这里只用到其中一部分）
type Names = string[]
type Pair = [string, string]
export interface CaseGold {
  nodes?: Names[]
  edges?: Pair[]
  groups?: Names[]
  optional_edges?: Pair[]
  forbidden_nodes?: string[]
  forbidden_edges?: Pair[]
  topics?: { title: Names; items: Names[] }[]
  struck?: Names[]
  not_struck?: Names[]
  emphasized?: Names[]
  not_emphasized?: Names[]
  questions?: Names[]
  formulas?: Names[]
}

export interface EvalCase {
  id: string
  name: string
  /** 从这张板开始（不给就是空板）、模型已经看过的前文、术语 */
  board?: Graph
  history?: Utterance[]
  terms?: Term[]
  turns: Utterance[]
  gold: CaseGold
}

const EMPHASIS: Mark[] = ['星标', '圈出', '下划线']
const emphasized = (n: Node | undefined) => !!n?.marks.some((m) => EMPHASIS.includes(m))

/**
 * 一条样本 → 一个回归用例：从模型当时看到的板书接着说这一句，最后的板书应该是用户改完的样子。
 * 节点全都要有（optional_nodes 在评分里会算进精确率的分母，用了反而冤枉对的答案）；
 * 连线里原来就在、没人动的只是「可以有」，不拉高召回；用户删掉的线、改掉的名字算「不该出现」。
 */
export function toCase(c: Correction, id: string): EvalCase {
  const seen = ink(c.seen)
  const drawn = drawnOf(c)
  const fixed = fixedOf(c)
  const S = new Map(seen.nodes.map((n) => [n.id, n]))
  const D = new Map(drawn.nodes.map((n) => [n.id, n]))
  const F = new Map(fixed.nodes.map((n) => [n.id, n]))
  // 这一轮或者用户动过的
  const hot = new Set([...changed(seen, fixed), ...changed(drawn, fixed)])
  // 用户改掉的名字：模型起的不对
  const wrong = renamed(drawn, fixed)
  const names = (n: Node): Names => [n.label, ...n.aliases.filter((x) => norm(x) !== norm(wrong.get(n.id) ?? ''))]
  const label = (id: string) => F.get(id)!.label
  const gold: CaseGold = {}
  const add = <K extends keyof CaseGold>(k: K, v: NonNullable<CaseGold[K]>[number]) => {
    ;((gold[k] ??= []) as unknown[]).push(v)
  }

  for (const n of fixed.nodes) {
    if (n.kind === 'node') add('nodes', names(n))
    if (!hot.has(n.id)) continue
    if (n.kind === 'question') add('questions', [n.label])
    if (n.kind === 'formula') add('formulas', [n.label])
    if (n.marks.includes('划掉') && !S.get(n.id)?.marks.includes('划掉')) add('struck', names(n))
    if (!n.marks.includes('划掉') && D.get(n.id)?.marks.includes('划掉')) add('not_struck', names(n))
    if (emphasized(n) && !emphasized(S.get(n.id))) add('emphasized', names(n))
    if (!emphasized(n) && emphasized(D.get(n.id))) add('not_emphasized', names(n))
  }
  // 话题和条目：每条挂在离它最近的那个话题下
  const topicOf = (n: Node): string | undefined => {
    const seenIds = new Set<string>()
    let p = n.parent ? F.get(n.parent) : undefined
    while (p && !seenIds.has(p.id)) {
      if (p.kind === 'topic') return p.id
      seenIds.add(p.id)
      p = p.parent ? F.get(p.parent) : undefined
    }
    return undefined
  }
  for (const tp of fixed.nodes.filter((n) => n.kind === 'topic'))
    add('topics', { title: names(tp), items: fixed.nodes.filter((n) => n.kind === 'item' && topicOf(n) === tp.id).map(names) })

  const ek = (e: { from: string; to: string }) => `${e.from}>${e.to}`
  const kept = new Set(fixed.edges.map(ek))
  for (const e of fixed.edges) add(hot.has(ek(e)) ? 'edges' : 'optional_edges', [label(e.from), label(e.to)])
  const gone = new Set<string>()
  for (const e of [...drawn.edges, ...seen.edges]) {
    if (kept.has(ek(e)) || !F.has(e.from) || !F.has(e.to) || gone.has(ek(e))) continue
    gone.add(ek(e))
    add('forbidden_edges', [label(e.from), label(e.to)])
  }
  // 用户删掉的节点、改掉的名字：板上不该再有叫这个的
  const taken = (s: string) => fixed.nodes.some((n) => norm(n.label) === norm(s))
  const banned = new Set<string>()
  for (const n of [...drawn.nodes, ...seen.nodes]) if (!F.has(n.id) && n.kind !== 'region') banned.add(n.label)
  for (const w of wrong.values()) banned.add(w)
  for (const s of banned) if (!taken(s)) add('forbidden_nodes', s)

  // 分组：动过的分组里的成员应该还在同一组
  for (const grp of fixed.groups) {
    const members = fixed.nodes.filter((n) => n.group === grp.id && n.kind === 'node')
    if (members.length && members.some((n) => hot.has(n.id) || hot.has(grp.id))) add('groups', members.map((n) => n.label))
  }

  const said = c.said.text.length > 40 ? `${c.said.text.slice(0, 40)}…` : c.said.text
  return {
    id,
    name: `${said} → ${c.edits.join('；')}`,
    board: seen,
    ...(c.context.length ? { history: c.context } : {}),
    ...(c.terms?.length ? { terms: c.terms } : {}),
    turns: [c.said],
    gold,
  }
}

/** 一条样本 → 一条偏好数据：发给模型的完整对话，用户要的（chosen）和模型原来给的（rejected） */
export function toPair(c: Correction, id: string) {
  return {
    id,
    prompt: messages(c.seen, c.context, c.said, c.terms ?? []),
    chosen: [{ role: 'assistant', content: wanted(c) }],
    rejected: [{ role: 'assistant', content: c.ops }],
  }
}

/** 能导出的：真改过、而且用户要的和模型给的不一样 */
export function exportable(c: Correction): boolean {
  return useful(c) && wanted(c) !== (formatOps(parseOps(c.ops)) || '-')
}

/** 几场会攒下的样本，每条带上编号（会议编号-第几条） */
export function gather(all: { id: string; corrections?: Correction[] }[]): { id: string; c: Correction }[] {
  return all.flatMap((m) => (m.corrections ?? []).map((c, i) => ({ id: `${m.id}-${i + 1}`, c }))).filter((x) => exportable(x.c))
}

/** 导出成评测用例文件：和 eval/cases.json 一样的格式，`node eval/run.ts --file <它>` 就能跑 */
export function casesFile(all: { id: string; corrections?: Correction[] }[]): string {
  return `${JSON.stringify(
    gather(all).map((x) => toCase(x.c, x.id)),
    null,
    2,
  )}\n`
}

/** 导出成偏好数据：一行一条（JSONL），DPO 直接用，SFT 取 prompt 和 chosen */
export function pairsFile(all: { id: string; corrections?: Correction[] }[]): string {
  return gather(all)
    .map((x) => `${JSON.stringify(toPair(x.c, x.id))}\n`)
    .join('')
}
