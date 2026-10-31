// 把模型画出的板书和人工标注对比。
// 名字比较宽松（别名、互相包含都算），因为「登录接口」和「登录服务」画在白板上没区别。

import { type Graph, type Node, norm } from '../src/engine/graph.ts'

type Names = string[]
type Pair = [string, string]

export interface Gold {
  /** 流程图节点 */
  nodes?: Names[]
  edges?: Pair[]
  /** 每组里的成员应该被放进同一个分组 */
  groups?: Names[]
  /** 这些边反过来画也算对 */
  either_direction?: Pair[]
  /** 每组第一条是标准答案，后面几条画成任意一条都算对 */
  alt_edges?: Pair[][]
  /** 画了不算错，不画也不扣分 */
  optional_nodes?: Names[]
  optional_edges?: Pair[]
  /** 出现了（而且没被划掉）就是错 */
  forbidden_nodes?: string[]
  forbidden_edges?: Pair[]
  /** 要点板：话题和挂在它下面（任意深度）的条目 */
  topics?: { title: Names; items: Names[] }[]
  /** 应该被划掉的 */
  struck?: Names[]
  /** 应该被强调的（星标、圈出、下划线都算） */
  emphasized?: Names[]
  /** 不该被强调的（比如强调落到了话题上，而不是被强调的那一条）：强调了算一次标记没命中 */
  not_emphasized?: Names[]
  /** 应该记成问题 / 公式的，写几个关键词，命中任意一个就算 */
  questions?: Names[]
  formulas?: Names[]
}

export interface Score {
  nodeP: number
  nodeR: number
  edgeP: number
  edgeR: number
  /** 话题找到了几个 */
  topicR: number | null
  itemP: number | null
  itemR: number | null
  /** 划掉、强调、问题、公式的命中率 */
  markAcc: number | null
  groupAcc: number | null
  violations: string[]
  missing: string[]
  extra: string[]
}

/** Lower-case words of a label with no Han in it; hyphens, underscores and punctuation split words */
const words = (s: string) => s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean)

/**
 * English labels match word by word: "Auth-service" equals "Auth service" (and "ClickHouse" equals
 * "Click House"), and a partial match
 * has to be whole consecutive words ("Auth" in "Auth service", but not "API" in "Capital").
 */
function sameWords(a: string, b: string): 0 | 1 | 2 {
  const x = words(a)
  const y = words(b)
  if (!x.length || !y.length) return 0
  // "ClickHouse" and "Click House" are the same name
  if (x.join('') === y.join('')) return 2
  const [s, l] = x.length <= y.length ? [x, y] : [y, x]
  for (let i = 0; i + s.length <= l.length; i++) if (s.every((w, j) => l[i + j] === w)) return 1
  return 0
}

function same(a: string, b: string): 0 | 1 | 2 {
  if (!/\p{Script=Han}/u.test(a + b)) return sameWords(a, b)
  const x = norm(a)
  const y = norm(b)
  if (!x || !y) return 0
  if (x === y) return 2
  const [s, l] = x.length <= y.length ? [x, y] : [y, x]
  return s.length >= 2 && l.includes(s) ? 1 : 0
}

const names = (n: Node) => [n.label, ...n.aliases]

function best(pred: string[], gold: Names): number {
  let b = 0
  for (const p of pred) for (const g of gold) b = Math.max(b, same(p, g))
  return b
}

const ratio = (a: number, b: number) => (b === 0 ? 1 : a / b)
const isAttach = (n: Node) => n.kind === 'question' || n.kind === 'formula' || n.kind === 'note'

type Entity = { key: string; names: Names; type: 'node' | 'topic' | 'item' | 'optional'; topic?: string }

export function score(g: Graph, gold: Gold): Score {
  const entities: Entity[] = []
  for (const n of gold.nodes ?? []) entities.push({ key: n[0], names: n, type: 'node' })
  for (const t of gold.topics ?? []) {
    entities.push({ key: t.title[0], names: t.title, type: 'topic' })
    for (const it of t.items) entities.push({ key: it[0], names: it, type: 'item', topic: t.title[0] })
  }
  for (const o of gold.optional_nodes ?? []) entities.push({ key: o[0], names: o, type: 'optional' })

  // 先配严格相等，再配包含关系；问题、公式、备注不参与实体匹配
  const map = new Map<string, Entity>() // 预测 id -> 标注实体
  const taken = new Set<string>()
  const pool = g.nodes.filter((n) => !isAttach(n))
  for (const level of [2, 1]) {
    for (const ent of entities) {
      if (taken.has(ent.key)) continue
      const hit = pool.find((n) => !map.has(n.id) && best(names(n), ent.names) >= level)
      if (hit) {
        map.set(hit.id, ent)
        taken.add(ent.key)
      }
    }
  }
  const struckIds = new Set(g.nodes.filter((n) => n.marks.includes('划掉')).map((n) => n.id))

  const violations: string[] = []
  for (const n of g.nodes)
    for (const fb of gold.forbidden_nodes ?? [])
      // 改名后旧名字留作别名是对的，所以只看当前名字；划掉了也算处理过
      if (same(n.label, fb) === 2 && !struckIds.has(n.id)) violations.push(`不该出现：${n.label}`)

  // 流程图节点
  const goldNodes = entities.filter((e) => e.type === 'node')
  const nodeTP = goldNodes.filter((e) => taken.has(e.key)).length
  const predNodes = g.nodes.filter((n) => n.kind === 'node' && !struckIds.has(n.id))
  const predNodeExtra = predNodes.filter((n) => !map.has(n.id))
  const predNodeHit = predNodes.filter((n) => map.get(n.id)?.type === 'node').length

  // 话题和条目：条目要挂在对的话题下面（任意深度）才算
  const topics = entities.filter((e) => e.type === 'topic')
  const items = entities.filter((e) => e.type === 'item')
  const idOf = (key: string) => [...map].find(([, e]) => e.key === key)?.[0]
  const ancestors = (id: string) => {
    const out = new Set<string>()
    let cur = g.nodes.find((n) => n.id === id)
    while (cur?.parent && !out.has(cur.parent)) {
      out.add(cur.parent)
      cur = g.nodes.find((n) => n.id === cur!.parent)
    }
    return out
  }
  let itemTP = 0
  const misplaced: string[] = []
  for (const it of items) {
    const id = idOf(it.key)
    const tid = idOf(it.topic!)
    if (id && tid && ancestors(id).has(tid)) itemTP++
    else if (id) misplaced.push(it.key)
  }
  const predItems = g.nodes.filter((n) => n.kind === 'item')

  // 标记
  let markHit = 0
  let markAll = 0
  const markMiss: string[] = []
  const has = (want: Names, ok: (n: Node) => boolean, label: string) => {
    markAll++
    if (g.nodes.some((n) => ok(n) && best(names(n), want) >= 1)) markHit++
    else markMiss.push(`${label}：${want[0]}`)
  }
  for (const s of gold.struck ?? []) has(s, (n) => n.marks.includes('划掉'), '没划掉')
  for (const s of gold.emphasized ?? [])
    has(s, (n) => n.marks.some((m) => m === '星标' || m === '圈出' || m === '下划线'), '没强调')
  for (const s of gold.not_emphasized ?? []) {
    markAll++
    const wrong = g.nodes.some((n) => n.marks.some((m) => m === '星标' || m === '圈出' || m === '下划线') && best(names(n), s) >= 1)
    if (wrong) markMiss.push(`强调错了地方：${s[0]}`)
    else markHit++
  }
  for (const q of gold.questions ?? []) has(q, (n) => n.kind === 'question', '没记成问题')
  for (const f of gold.formulas ?? []) has(f, (n) => n.kind === 'formula', '没记成公式')

  // 连线：映射到标注名后比较；碰到被划掉东西的线不计
  const canon = (p: Pair): string => {
    for (const grp of gold.alt_edges ?? []) {
      if (grp.some(([a, b]) => a === p[0] && b === p[1])) return `${grp[0][0]}>${grp[0][1]}`
    }
    const ed = gold.either_direction?.find(([a, b]) => (a === p[0] && b === p[1]) || (a === p[1] && b === p[0]))
    if (ed) return `${ed[0]}>${ed[1]}`
    return `${p[0]}>${p[1]}`
  }
  const goldEdges = new Set((gold.edges ?? []).map((e) => canon(e)))
  const optionalEdges = new Set((gold.optional_edges ?? []).map((e) => `${e[0]}>${e[1]}`))
  const forbidden = new Set((gold.forbidden_edges ?? []).map((e) => `${e[0]}>${e[1]}`))
  const labelOf = (id: string) => map.get(id)?.key ?? g.nodes.find((n) => n.id === id)?.label ?? id
  const predEdges = new Set<string>()
  for (const e of g.edges) {
    if (struckIds.has(e.from) || struckIds.has(e.to)) continue
    const raw = `${labelOf(e.from)}>${labelOf(e.to)}`
    if (forbidden.has(raw)) violations.push(`不该出现的连线：${raw.replace('>', ' → ')}`)
    if (optionalEdges.has(raw)) continue
    predEdges.add(canon([labelOf(e.from), labelOf(e.to)]))
  }
  let edgeTP = 0
  for (const e of predEdges) if (goldEdges.has(e)) edgeTP++

  // 分组：同一组的标准成员是否落在同一个分组里
  let groupAcc: number | null = null
  if (gold.groups?.length) {
    let ok = 0
    for (const members of gold.groups) {
      const gs = members.map((m) => g.nodes.find((n) => map.get(n.id)?.key === m)?.group)
      if (gs.every((x) => x !== undefined && x === gs[0])) ok++
    }
    groupAcc = ok / gold.groups.length
  }

  const arrow = (s: string) => s.replace('>', ' → ')
  const extraItems = predItems.filter((n) => !map.has(n.id)).map((n) => n.label)
  return {
    nodeP: ratio(predNodeHit, predNodes.length),
    nodeR: ratio(nodeTP, goldNodes.length),
    edgeP: ratio(edgeTP, predEdges.size),
    edgeR: ratio(edgeTP, goldEdges.size),
    topicR: topics.length ? topics.filter((t) => taken.has(t.key)).length / topics.length : null,
    itemP: items.length ? ratio(itemTP, predItems.length) : null,
    itemR: items.length ? itemTP / items.length : null,
    markAcc: markAll ? markHit / markAll : null,
    groupAcc,
    violations,
    missing: [
      ...goldNodes.filter((e) => !taken.has(e.key)).map((e) => `节点 ${e.key}`),
      ...topics.filter((e) => !taken.has(e.key)).map((e) => `话题 ${e.key}`),
      ...items.filter((e) => !taken.has(e.key)).map((e) => `条目 ${e.key}`),
      ...misplaced.map((k) => `条目挂错位置 ${k}`),
      ...[...goldEdges].filter((e) => !predEdges.has(e)).map((e) => `连线 ${arrow(e)}`),
      ...markMiss,
    ],
    extra: [
      ...predNodeExtra.map((n) => `节点 ${n.label}`),
      ...extraItems.map((l) => `条目 ${l}`),
      ...[...predEdges].filter((e) => !goldEdges.has(e)).map((e) => `连线 ${arrow(e)}`),
    ],
  }
}

export function f1(p: number, r: number): number {
  return p + r === 0 ? 0 : (2 * p * r) / (p + r)
}
