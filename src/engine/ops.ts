// 模型输出的操作：一行一条，越短越好（输出 token 数直接决定延迟）。
//
// 流程图部分：
//   E 引用 -> 引用 [: 「标签」] 连线；引用里出现的新名字会自动建节点
//   N 「名字」                 新增节点（已存在就忽略）
//   X 引用 -> 引用             删除连线
//   G 「组名」 引用 ...        把节点放进分组（没有就新建）
// 要点和随手记：
//   T 「话题」                 新起一块话题（要点、分工、方案都从这里挂）
//   L 引用 「条目」            在话题或条目下面加一条（挂在条目下就是子条目）
//   Q [引用] 「问题」          记一个待定的问题，可以挂在某个东西旁边
//   F [引用] 「公式」          记一个公式或短表达式
//   K 引用 「备注」            在某个东西旁边写一句备注
//   M 引用 划掉|圈出|下划线|星标   做标记；纠正时优先划掉而不是删掉
//   I 引用 图标名              给东西配一个小图标
//   H 引用                     指给大家看：高亮一下，板书不变
// 通用：
//   R 引用 「新名字」          改名，旧名字留作别名
//   A 引用 「别名」            记一个别名
//   D 引用                     删除（连同挂在它下面的东西）
//   -                          这句话不需要改板书
//
// 引用 = 实体表里的 id（n3）或者 「名字」。

import {
  type Graph,
  ICONS,
  type Icon,
  type Ink,
  type Kind,
  MARKS,
  type Mark,
  cloneGraph,
  findNode,
  norm,
  subtree,
} from './graph.ts'

export type Ref = { id: string } | { name: string }

export type Op =
  | { op: 'N'; name: string }
  | { op: 'E'; from: Ref; to: Ref; label?: string }
  | { op: 'X'; from: Ref; to: Ref }
  | { op: 'R'; target: Ref; name: string }
  | { op: 'A'; target: Ref; alias: string }
  | { op: 'G'; group: string; members: Ref[] }
  | { op: 'D'; target: Ref }
  | { op: 'T'; name: string }
  | { op: 'L'; parent: Ref; name: string }
  | { op: 'Q' | 'F'; parent?: Ref; text: string }
  | { op: 'K'; target: Ref; text: string }
  | { op: 'M'; target: Ref; mark: Mark }
  | { op: 'I'; target: Ref; icon: Icon }
  | { op: 'H'; target: Ref }

type Tok = { quoted: boolean; text: string }

const TOKEN = /\s*(?:「([^」\n]*)」|(\S+))/y

function tokens(line: string): Tok[] {
  const out: Tok[] = []
  TOKEN.lastIndex = 0
  let m: RegExpExecArray | null
  while (TOKEN.lastIndex < line.length && (m = TOKEN.exec(line))) {
    if (m[1] !== undefined) out.push({ quoted: true, text: m[1].trim() })
    // 箭头和冒号只是写给模型看的分隔符
    else if (m[2] !== undefined && m[2] !== '->' && m[2] !== ':') out.push({ quoted: false, text: m[2] })
  }
  return out
}

function ref(t: Tok | undefined): Ref | undefined {
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
    const second = rest[1]?.text
    switch (head.text) {
      case 'N':
        if (rest[0]?.text) ops.push({ op: 'N', name: rest[0].text })
        break
      case 'H':
        if (a) ops.push({ op: 'H', target: a })
        break
      case 'T':
        if (rest[0]?.text) ops.push({ op: 'T', name: rest[0].text })
        break
      case 'E':
        if (a && b) ops.push({ op: 'E', from: a, to: b, label: rest[2]?.text || undefined })
        break
      case 'X':
        if (a && b) ops.push({ op: 'X', from: a, to: b })
        break
      case 'R':
        if (a && second) ops.push({ op: 'R', target: a, name: second })
        break
      case 'A':
        if (a && second) ops.push({ op: 'A', target: a, alias: second })
        break
      case 'L':
        if (a && second) ops.push({ op: 'L', parent: a, name: second })
        break
      case 'K':
        if (a && second) ops.push({ op: 'K', target: a, text: second })
        break
      case 'Q':
      case 'F':
        // 一个参数是独立的问题/公式，两个参数时第一个是它挂靠的东西
        if (rest.length >= 2 && second) ops.push({ op: head.text, parent: a, text: second })
        else if (rest[0]?.text) ops.push({ op: head.text, text: rest[0].text })
        break
      case 'M':
        if (a && (MARKS as readonly string[]).includes(second ?? ''))
          ops.push({ op: 'M', target: a, mark: second as Mark })
        break
      case 'I':
        if (a && (ICONS as readonly string[]).includes(second ?? ''))
          ops.push({ op: 'I', target: a, icon: second as Icon })
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
        case 'T':
          return `${o.op} 「${o.name}」`
        case 'E':
          return `E ${r(o.from)} -> ${r(o.to)}${o.label ? ` : 「${o.label}」` : ''}`
        case 'X':
          return `X ${r(o.from)} -> ${r(o.to)}`
        case 'R':
          return `R ${r(o.target)} 「${o.name}」`
        case 'A':
          return `A ${r(o.target)} 「${o.alias}」`
        case 'L':
          return `L ${r(o.parent)} 「${o.name}」`
        case 'Q':
        case 'F':
          return `${o.op} ${o.parent ? `${r(o.parent)} ` : ''}「${o.text}」`
        case 'K':
          return `K ${r(o.target)} 「${o.text}」`
        case 'M':
          return `M ${r(o.target)} ${o.mark}`
        case 'I':
          return `I ${r(o.target)} ${o.icon}`
        case 'H':
          return `H ${r(o.target)}`
        case 'G':
          return `G 「${o.group}」 ${o.members.map(r).join(' ')}`
        case 'D':
          return `D ${r(o.target)}`
      }
    })
    .join('\n')
}

function add(g: Graph, label: string, kind: Kind, ink: Ink, parent?: string): string {
  const id = `n${g.nextNode++}`
  g.nodes.push({ id, label, aliases: [], kind, parent, marks: [], ink })
  return id
}

/** 引用 → 节点 id。`create` 给出新名字时建什么类型的节点。 */
function resolve(g: Graph, r: Ref, create: Kind | false, ink: Ink): string | undefined {
  let name: string
  if ('id' in r) {
    const id = r.id
    if (g.nodes.some((n) => n.id === id)) return id
    // 长得像编号却不存在（远程模型没有语法约束，会编出 n12 这种编号）：不能当名字建一个叫「n12」的节点
    if (/^[ng]\d+$/.test(id)) return undefined
    // 模型偶尔会把名字写成裸词，当名字再找一次
    name = id
  } else {
    name = r.name
  }
  const hit = findNode(g, name)
  if (hit) return hit.id
  if (!create || !norm(name)) return undefined
  return add(g, name, create, ink)
}

/** 把操作落到图上，返回新图（不改原图）。`ink` 决定新内容是铅笔稿还是墨线。 */
export function applyOps(graph: Graph, ops: Op[], ink: Ink = 'ink'): Graph {
  const g = cloneGraph(graph)
  // 导入文件上的原文区域是钉死的：模型只能标记、连线、加备注，不能改名、删掉、挪进话题或分组
  const pinned = new Map(graph.nodes.filter((n) => n.kind === 'region').map((n) => [n.id, n]))
  const get = (id: string | undefined) => (id ? g.nodes.find((x) => x.id === id) : undefined)
  const touch = (id: string | undefined) => {
    const n = get(id)
    if (n && ink === 'ink') n.ink = 'ink'
  }
  for (const o of ops) {
    switch (o.op) {
      case 'N':
        touch(resolve(g, { name: o.name }, 'node', ink))
        break
      case 'T': {
        const id = resolve(g, { name: o.name }, 'topic', ink)
        const n = get(id)
        // 已经存在的普通节点被「升格」成话题
        if (n && n.kind === 'node') n.kind = 'topic'
        touch(id)
        break
      }
      case 'L': {
        const parent = resolve(g, o.parent, 'topic', ink)
        if (!parent) break
        const hit = findNode(g, o.name)
        if (hit && hit.id === parent) break
        if (hit && !subtree(g, hit.id).has(parent)) {
          // 已经有了：挪到新的位置下面（「这个归到 xx 下面」）
          hit.parent = parent
          if (hit.kind === 'node' || hit.kind === 'topic') hit.kind = 'item'
          touch(hit.id)
        } else if (!hit) {
          add(g, o.name, 'item', ink, parent)
        }
        touch(parent)
        break
      }
      case 'Q':
      case 'F': {
        const kind = o.op === 'Q' ? 'question' : 'formula'
        const parent = o.parent ? resolve(g, o.parent, false, ink) : undefined
        const hit = findNode(g, o.text)
        if (hit) touch(hit.id)
        else add(g, o.text, kind, ink, parent)
        break
      }
      case 'K': {
        const target = resolve(g, o.target, false, ink)
        if (target && !findNode(g, o.text)) add(g, o.text, 'note', ink, target)
        break
      }
      case 'M': {
        const n = get(resolve(g, o.target, false, ink))
        if (n && !n.marks.includes(o.mark)) n.marks.push(o.mark)
        break
      }
      // 只是指一下，由 Session 负责高亮
      case 'H':
        break
      case 'I': {
        const n = get(resolve(g, o.target, false, ink))
        if (n) n.icon = o.icon
        break
      }
      case 'E': {
        const from = resolve(g, o.from, 'node', ink)
        const to = resolve(g, o.to, 'node', ink)
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
        const n = get(resolve(g, o.target, false, ink))
        if (!n || norm(n.label) === norm(o.name)) break
        if (!n.aliases.some((a) => norm(a) === norm(n.label))) n.aliases.push(n.label)
        n.label = o.name
        n.aliases = n.aliases.filter((a) => norm(a) !== norm(o.name))
        touch(n.id)
        break
      }
      case 'A': {
        const n = get(resolve(g, o.target, false, ink))
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
          const id = resolve(g, m, 'node', ink)
          const n = get(id)
          if (n) n.group = grp.id
          touch(id)
        }
        break
      }
      case 'D': {
        const id = resolve(g, o.target, false, ink)
        if (!id) break
        const gone = subtree(g, id)
        g.nodes = g.nodes.filter((n) => !gone.has(n.id))
        g.edges = g.edges.filter((e) => !gone.has(e.from) && !gone.has(e.to))
        break
      }
    }
  }
  if (pinned.size) {
    const now = new Map(g.nodes.map((n) => [n.id, n]))
    for (const [id, was] of pinned) {
      const n = now.get(id)
      // 被删了就放回去（标记留着），改了的名字、位置、归属都还原
      if (!n) g.nodes.push({ ...was, aliases: [...was.aliases], marks: [...was.marks], at: was.at && { ...was.at } })
      else Object.assign(n, { label: was.label, aliases: [...was.aliases], kind: 'region', at: was.at && { ...was.at }, parent: undefined, group: undefined })
    }
    // 挂在原文上的只能是备注、问题之类，不能是条目
    for (const n of g.nodes) if (n.parent && pinned.has(n.parent) && n.kind === 'item') n.kind = 'note'
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
  out.nodes = out.nodes.filter((n) => !n.parent || alive.has(n.parent))
  out.edges = out.edges.filter(
    (e) =>
      (e.ink === 'ink' || (only !== undefined && !only.edges.has(`${e.from}>${e.to}`))) &&
      alive.has(e.from) &&
      alive.has(e.to),
  )
  out.groups = out.groups.filter((x) => out.nodes.some((n) => n.group === x.id))
  return out
}
