// 把模型画出的图和人工标注的图对比。
// 名字比较宽松（别名、互相包含都算），因为「登录接口」和「登录服务」画在白板上没区别。

import { type Graph, norm } from '../src/engine/graph.ts'

type Names = string[]
type Pair = [string, string]

export interface Gold {
  nodes: Names[]
  edges: Pair[]
  /** 每组里的成员应该被放进同一个分组 */
  groups?: Names[]
  /** 这些边反过来画也算对 */
  either_direction?: Pair[]
  /** 每组第一条是标准答案，后面几条画成任意一条都算对 */
  alt_edges?: Pair[][]
  /** 画了不算错，不画也不扣分 */
  optional_nodes?: Names[]
  optional_edges?: Pair[]
  /** 出现了就是错（被纠正掉、被删掉的东西） */
  forbidden_nodes?: string[]
  forbidden_edges?: Pair[]
}

export interface Score {
  nodeP: number
  nodeR: number
  edgeP: number
  edgeR: number
  groupAcc: number | null
  violations: string[]
  missingNodes: string[]
  extraNodes: string[]
  missingEdges: string[]
  extraEdges: string[]
}

function same(a: string, b: string): 0 | 1 | 2 {
  const x = norm(a)
  const y = norm(b)
  if (!x || !y) return 0
  if (x === y) return 2
  const [s, l] = x.length <= y.length ? [x, y] : [y, x]
  return s.length >= 2 && l.includes(s) ? 1 : 0
}

function names(n: Graph['nodes'][number]): string[] {
  return [n.label, ...n.aliases]
}

function best(pred: string[], gold: Names): number {
  let b = 0
  for (const p of pred) for (const g of gold) b = Math.max(b, same(p, g))
  return b
}

const f = (a: number, b: number) => (b === 0 ? 1 : a / b)

export function score(g: Graph, gold: Gold): Score {
  // 节点：先配严格相等，再配包含关系
  const map = new Map<string, string>() // 预测 id -> 标准名（每组第一个）
  const taken = new Set<string>()
  for (const level of [2, 1]) {
    for (const gn of gold.nodes) {
      if (taken.has(gn[0])) continue
      const hit = g.nodes.find((n) => !map.has(n.id) && best(names(n), gn) >= level)
      if (hit) {
        map.set(hit.id, gn[0])
        taken.add(gn[0])
      }
    }
  }
  const optional = new Set<string>()
  for (const n of g.nodes) {
    if (map.has(n.id)) continue
    const o = gold.optional_nodes?.find((on) => best(names(n), on) >= 1)
    if (o) {
      map.set(n.id, o[0])
      optional.add(n.id)
    }
  }
  const violations: string[] = []
  for (const n of g.nodes) {
    for (const fb of gold.forbidden_nodes ?? []) {
      // 改名后旧名字留作别名是对的，所以只看当前名字
      if (same(n.label, fb) === 2) violations.push(`不该出现的节点：${n.label}`)
    }
  }
  const realPred = g.nodes.filter((n) => !optional.has(n.id))
  const nodeTP = taken.size

  // 边：映射到标准名后比较
  const canon = (p: Pair): string => {
    for (const grp of gold.alt_edges ?? []) {
      if (grp.some(([a, b]) => a === p[0] && b === p[1])) return `${grp[0][0]}>${grp[0][1]}`
    }
    if (gold.either_direction?.some(([a, b]) => (a === p[0] && b === p[1]) || (a === p[1] && b === p[0]))) {
      const e = gold.either_direction.find(([a, b]) => (a === p[0] && b === p[1]) || (a === p[1] && b === p[0]))!
      return `${e[0]}>${e[1]}`
    }
    return `${p[0]}>${p[1]}`
  }
  const goldEdges = new Set(gold.edges.map((e) => canon(e)))
  const optionalEdges = new Set((gold.optional_edges ?? []).map((e) => `${e[0]}>${e[1]}`))
  const forbidden = new Set((gold.forbidden_edges ?? []).map((e) => `${e[0]}>${e[1]}`))
  const labelOf = (id: string) => map.get(id) ?? g.nodes.find((n) => n.id === id)?.label ?? id
  const predEdges = new Set<string>()
  let optionalHit = 0
  for (const e of g.edges) {
    const raw = `${labelOf(e.from)}>${labelOf(e.to)}`
    if (forbidden.has(raw)) violations.push(`不该出现的连线：${raw.replace('>', ' → ')}`)
    if (optionalEdges.has(raw)) {
      optionalHit++
      continue
    }
    predEdges.add(canon([labelOf(e.from), labelOf(e.to)]))
  }
  let edgeTP = 0
  for (const e of predEdges) if (goldEdges.has(e)) edgeTP++

  // 分组：同一组的标准成员是否落在同一个分组里
  let groupAcc: number | null = null
  if (gold.groups?.length) {
    let ok = 0
    for (const members of gold.groups) {
      const gs = members.map((m) => g.nodes.find((n) => map.get(n.id) === m)?.group)
      if (gs.every((x) => x !== undefined && x === gs[0])) ok++
    }
    groupAcc = ok / gold.groups.length
  }

  const arrow = (s: string) => s.replace('>', ' → ')
  return {
    nodeP: f(nodeTP, realPred.length),
    nodeR: f(nodeTP, gold.nodes.length),
    edgeP: f(edgeTP, predEdges.size),
    edgeR: f(edgeTP, goldEdges.size),
    groupAcc,
    violations,
    missingNodes: gold.nodes.filter((gn) => !taken.has(gn[0])).map((gn) => gn[0]),
    extraNodes: realPred.filter((n) => !map.has(n.id)).map((n) => n.label),
    missingEdges: [...goldEdges].filter((e) => !predEdges.has(e)).map(arrow),
    extraEdges: [...predEdges].filter((e) => !goldEdges.has(e)).map(arrow),
  }
}

export function f1(p: number, r: number): number {
  return p + r === 0 ? 0 : (2 * p * r) / (p + r)
}
