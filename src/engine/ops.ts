// 模型输出的操作：一行一条，越短越好（输出 token 数直接决定延迟）。
//
//   N 「名字」                 新增节点（已存在就忽略）
//   E 引用 -> 引用 [: 「标签」] 连线；引用里出现的新名字会自动建节点
//   X 引用 引用                删除连线（「不对」「不是 A 到 B」）
//   R 引用 「新名字」          改名，旧名字留作别名
//   A 引用 「别名」            记一个别名（「它」「那个服务」以后要能对上）
//   G 「组名」 引用 ...        把节点放进分组（没有就新建）
//   D 引用                     删除节点和它的连线
//   -                          这句话不需要改板书
//
// 引用 = 实体表里的 id（n3）或者 「名字」。

import { type Graph, type Ink, cloneGraph, findNode, norm } from './graph.ts'

export type Ref = { id: string } | { name: string }

export type Op =
  | { op: 'N'; name: string }
  | { op: 'E'; from: Ref; to: Ref; label?: string }
  | { op: 'X'; from: Ref; to: Ref }
  | { op: 'R'; target: Ref; name: string }
  | { op: 'A'; target: Ref; alias: string }
  | { op: 'G'; group: string; members: Ref[] }
  | { op: 'D'; target: Ref }

const TOKEN = /\s*(?:「([^」\n]*)」|(\S+))/y

function tokens(line: string): { quoted: boolean; text: string }[] {
  const out: { quoted: boolean; text: string }[] = []
  TOKEN.lastIndex = 0
  let m: RegExpExecArray | null
  while (TOKEN.lastIndex < line.length && (m = TOKEN.exec(line))) {
    if (m[1] !== undefined) out.push({ quoted: true, text: m[1].trim() })
    // 箭头和冒号只是写给模型看的分隔符
    else if (m[2] !== undefined && m[2] !== '->' && m[2] !== ':') out.push({ quoted: false, text: m[2] })
  }
  return out
}

function ref(t: { quoted: boolean; text: string } | undefined): Ref | undefined {
  if (!t || !t.text) return undefined
  return t.quoted ? { name: t.text } : { id: t.text }
}

/** 解析模型输出。坏行直接跳过，不让一行错误毁掉整批。 */
export function parseOps(text: string): Op[] {
  const ops: Op[] = []
  for (const raw of text.split('\n')) {
    const line = raw.trim()
    if (!line || line === '-' || line === 'END') continue
    const [head, ...rest] = tokens(line)
    if (!head || head.quoted) continue
    const a = ref(rest[0])
    const b = ref(rest[1])
    switch (head.text) {
      case 'N':
        if (rest[0]?.text) ops.push({ op: 'N', name: rest[0].text })
        break
      case 'E':
        if (a && b) ops.push({ op: 'E', from: a, to: b, label: rest[2]?.text || undefined })
        break
      case 'X':
        if (a && b) ops.push({ op: 'X', from: a, to: b })
        break
      case 'R':
        if (a && rest[1]?.text) ops.push({ op: 'R', target: a, name: rest[1].text })
        break
      case 'A':
        if (a && rest[1]?.text) ops.push({ op: 'A', target: a, alias: rest[1].text })
        break
      case 'G':
        if (rest[0]?.text && rest.length > 1)
          ops.push({ op: 'G', group: rest[0].text, members: rest.slice(1).map(ref).filter((r) => r !== undefined) })
        break
      case 'D':
        if (a) ops.push({ op: 'D', target: a })
        break
    }
  }
  return ops
}

export function formatOps(ops: Op[]): string {
  const r = (x: Ref) => ('id' in x ? x.id : `「${x.name}」`)
  return ops
    .map((o) => {
      switch (o.op) {
        case 'N':
          return `N 「${o.name}」`
        case 'E':
          return `E ${r(o.from)} -> ${r(o.to)}${o.label ? ` : 「${o.label}」` : ''}`
        case 'X':
          return `X ${r(o.from)} -> ${r(o.to)}`
        case 'R':
          return `R ${r(o.target)} 「${o.name}」`
        case 'A':
          return `A ${r(o.target)} 「${o.alias}」`
        case 'G':
          return `G 「${o.group}」 ${o.members.map(r).join(' ')}`
        case 'D':
          return `D ${r(o.target)}`
      }
    })
    .join('\n')
}

function resolve(g: Graph, r: Ref, create: boolean, ink: Ink): string | undefined {
  let name: string
  if ('id' in r) {
    const id = r.id
    if (g.nodes.some((n) => n.id === id)) return id
    // 模型偶尔会把名字写成裸词，当名字再找一次
    name = id
  } else {
    name = r.name
  }
  const hit = findNode(g, name)
  if (hit) return hit.id
  if (!create || !norm(name)) return undefined
  const id = `n${g.nextNode++}`
  g.nodes.push({ id, label: name, aliases: [], ink })
  return id
}

/** 把操作落到图上，返回新图（不改原图）。`ink` 决定新内容是铅笔稿还是墨线。 */
export function applyOps(graph: Graph, ops: Op[], ink: Ink = 'ink'): Graph {
  const g = cloneGraph(graph)
  const touch = (id: string | undefined) => {
    const n = id ? g.nodes.find((x) => x.id === id) : undefined
    if (n && ink === 'ink') n.ink = 'ink'
  }
  for (const o of ops) {
    switch (o.op) {
      case 'N':
        touch(resolve(g, { name: o.name }, true, ink))
        break
      case 'E': {
        const from = resolve(g, o.from, true, ink)
        const to = resolve(g, o.to, true, ink)
        if (!from || !to || from === to) break
        touch(from)
        touch(to)
        const old = g.edges.find((e) => e.from === from && e.to === to)
        if (old) {
          if (o.label) old.label = o.label
          if (ink === 'ink') old.ink = 'ink'
        } else {
          g.edges.push({ from, to, label: o.label, ink })
        }
        break
      }
      case 'X': {
        const from = resolve(g, o.from, false, ink)
        const to = resolve(g, o.to, false, ink)
        g.edges = g.edges.filter((e) => !(e.from === from && e.to === to))
        break
      }
      case 'R': {
        const id = resolve(g, o.target, false, ink)
        const n = g.nodes.find((x) => x.id === id)
        if (!n || norm(n.label) === norm(o.name)) break
        if (!n.aliases.some((a) => norm(a) === norm(n.label))) n.aliases.push(n.label)
        n.label = o.name
        n.aliases = n.aliases.filter((a) => norm(a) !== norm(o.name))
        touch(n.id)
        break
      }
      case 'A': {
        const id = resolve(g, o.target, false, ink)
        const n = g.nodes.find((x) => x.id === id)
        if (!n || findNode(g, o.alias)) break
        n.aliases.push(o.alias)
        break
      }
      case 'G': {
        let grp = g.groups.find((x) => norm(x.label) === norm(o.group))
        if (!grp) {
          grp = { id: `g${g.nextGroup++}`, label: o.group }
          g.groups.push(grp)
        }
        for (const m of o.members) {
          const id = resolve(g, m, true, ink)
          const n = g.nodes.find((x) => x.id === id)
          if (n) n.group = grp.id
          touch(id)
        }
        break
      }
      case 'D': {
        const id = resolve(g, o.target, false, ink)
        if (!id) break
        g.nodes = g.nodes.filter((n) => n.id !== id)
        g.edges = g.edges.filter((e) => e.from !== id && e.to !== id)
        break
      }
    }
  }
  // 空分组不留
  g.groups = g.groups.filter((x) => g.nodes.some((n) => n.group === x.id))
  return g
}

export interface DraftMark {
  nodes: Set<string>
  edges: Set<string>
}

/** 记下此刻有哪些铅笔稿，模型这一轮只负责裁决它们。 */
export function markDrafts(g: Graph): DraftMark {
  return {
    nodes: new Set(g.nodes.filter((n) => n.ink === 'draft').map((n) => n.id)),
    edges: new Set(g.edges.filter((e) => e.ink === 'draft').map((e) => `${e.from}>${e.to}`)),
  }
}

/**
 * 模型过完一遍以后：它确认过的铅笔稿已经被描成墨线，剩下没被确认的铅笔稿擦掉。
 * 传了 `only` 时只擦这一轮开始前就存在的铅笔稿，模型跑的这段时间里新画的留着。
 */
export function eraseDrafts(g: Graph, only?: DraftMark): Graph {
  const out = cloneGraph(g)
  out.nodes = out.nodes.filter((n) => n.ink === 'ink' || (only !== undefined && !only.nodes.has(n.id)))
  const alive = new Set(out.nodes.map((n) => n.id))
  out.edges = out.edges.filter(
    (e) =>
      (e.ink === 'ink' || (only !== undefined && !only.edges.has(`${e.from}>${e.to}`))) &&
      alive.has(e.from) &&
      alive.has(e.to),
  )
  out.groups = out.groups.filter((x) => out.nodes.some((n) => n.group === x.id))
  return out
}
