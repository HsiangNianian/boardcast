// 板书的状态：一张由节点、连线、分组组成的图。
// 模型从不直接画东西，只发出操作（见 ops.ts），由这里落到状态上。

export type Ink = 'draft' | 'ink'

export interface Node {
  id: string
  label: string
  aliases: string[]
  group?: string
  ink: Ink
}

export interface Edge {
  from: string
  to: string
  label?: string
  ink: Ink
}

export interface Group {
  id: string
  label: string
}

export interface Graph {
  nodes: Node[]
  edges: Edge[]
  groups: Group[]
  nextNode: number
  nextGroup: number
}

export function emptyGraph(): Graph {
  return { nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1 }
}

export function cloneGraph(g: Graph): Graph {
  return {
    nodes: g.nodes.map((n) => ({ ...n, aliases: [...n.aliases] })),
    edges: g.edges.map((e) => ({ ...e })),
    groups: g.groups.map((x) => ({ ...x })),
    nextNode: g.nextNode,
    nextGroup: g.nextGroup,
  }
}

/** 名字比较时忽略大小写、空白和常见标点。 */
export function norm(s: string): string {
  return s.toLowerCase().replace(/[\s·・,，。.、"“”'‘’「」『』()（）]/g, '')
}

/** 按名字或别名找节点。实体表的指代消解先靠这个，再交给模型。 */
export function findNode(g: Graph, name: string): Node | undefined {
  const k = norm(name)
  if (!k) return undefined
  return g.nodes.find((n) => norm(n.label) === k || n.aliases.some((a) => norm(a) === k))
}

/** 给模型看的紧凑实体表。越短越好：它决定了每次要重新读入多少 token。 */
export function describe(g: Graph): string {
  if (g.nodes.length === 0 && g.groups.length === 0) return '（空）'
  const lines: string[] = []
  for (const x of g.groups) lines.push(`${x.id} [组] ${x.label}`)
  for (const n of g.nodes) {
    const extra: string[] = []
    if (n.aliases.length) extra.push(`又称 ${n.aliases.join('/')}`)
    if (n.group) extra.push(`在 ${n.group}`)
    lines.push(`${n.id} ${n.label}${extra.length ? `（${extra.join('，')}）` : ''}`)
  }
  // 连线带上名字：只给编号时，小模型经常删错线
  const name = (id: string) => g.nodes.find((n) => n.id === id)?.label ?? ''
  for (const e of g.edges)
    lines.push(`${e.from}(${name(e.from)}) -> ${e.to}(${name(e.to)})${e.label ? ` ${e.label}` : ''}`)
  return lines.join('\n')
}
