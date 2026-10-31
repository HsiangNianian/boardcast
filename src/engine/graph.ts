// 板书的状态。不只是流程图：一块板上可以同时有
// - 话题（topic）和挂在它下面的条目（item，可以再挂子条目），用来记要点、分工、方案；
// - 普通节点（node）和连线，用来画流程、调用、因果，这部分走自动排版；
// - 问题（question）、公式（formula）、备注（note），可以挂在别的东西上，也可以单独存在；
// - 标记：划掉、圈出、下划线、星标。纠正时更像真实板书的做法是划掉，而不是擦掉。
// 模型从不直接画东西，只发出操作（见 ops.ts），由这里落到状态上。

export type Ink = 'draft' | 'ink'

/** region：导入的 PDF / 图片上的一块内容，钉在底图上的固定位置，只能被标记、连线、加备注 */
export type Kind = 'node' | 'topic' | 'item' | 'question' | 'formula' | 'note' | 'region'

export const MARKS = ['划掉', '圈出', '下划线', '星标'] as const
export type Mark = (typeof MARKS)[number]

/** 图标库：模型只能从这里挑，画法见 board/icons.ts */
export const ICONS = ['数据库', '表格', '手机', '人', '服务器', '文档', '云', '图表', '消息'] as const
export type Icon = (typeof ICONS)[number]

export interface Node {
  id: string
  label: string
  aliases: string[]
  kind: Kind
  /** 条目挂在话题或条目下；问题、公式、备注可以挂在任何东西上 */
  parent?: string
  group?: string
  marks: Mark[]
  icon?: Icon
  ink: Ink
  /** region 在底图上的位置（板书坐标） */
  at?: { x: number; y: number; w: number; h: number }
  /** region 的全文：label 只是开头，模型要看全文才知道一个词在哪一段 */
  text?: string
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
  /** 垫在底下的导入页（PDF 的一页、一张图片）的大小；图片本身存在会议的这张板里 */
  underlay?: { w: number; h: number }
}

export function emptyGraph(): Graph {
  return { nodes: [], edges: [], groups: [], nextNode: 1, nextGroup: 1 }
}

export function cloneGraph(g: Graph): Graph {
  return {
    nodes: g.nodes.map((n) => ({ ...n, aliases: [...n.aliases], marks: [...n.marks], ...(n.at ? { at: { ...n.at } } : {}) })),
    edges: g.edges.map((e) => ({ ...e })),
    groups: g.groups.map((x) => ({ ...x })),
    nextNode: g.nextNode,
    nextGroup: g.nextGroup,
    ...(g.underlay ? { underlay: { ...g.underlay } } : {}),
  }
}

/** 名字比较时忽略大小写、空白和常见标点。 */
export function norm(s: string): string {
  return s.toLowerCase().replace(/[\s·・,，。.、"“”'‘’「」『』()（）?？!！]/g, '')
}

/** 按名字或别名找节点。实体表的指代消解先靠这个，再交给模型。 */
export function findNode(g: Graph, name: string): Node | undefined {
  const k = norm(name)
  if (!k) return undefined
  return g.nodes.find((n) => norm(n.label) === k || n.aliases.some((a) => norm(a) === k))
}

export function children(g: Graph, id: string): Node[] {
  return g.nodes.filter((n) => n.parent === id)
}

/** 自己和所有后代，删除时一起删 */
export function subtree(g: Graph, id: string): Set<string> {
  const out = new Set([id])
  let grew = true
  while (grew) {
    grew = false
    for (const n of g.nodes) {
      if (n.parent && out.has(n.parent) && !out.has(n.id)) {
        out.add(n.id)
        grew = true
      }
    }
  }
  return out
}

const TAG: Record<Kind, string> = {
  node: '',
  topic: '[话题] ',
  item: '',
  question: '[问题] ',
  formula: '[公式] ',
  note: '[备注] ',
  region: '[原文] ',
}

/** 实体表里一块原文最多给多少字 */
const REGION_TEXT = 3000

/** 给模型看的紧凑实体表。越短越好：它决定了每次要重新读入多少 token。 */
export function describe(g: Graph): string {
  if (g.nodes.length === 0 && g.groups.length === 0) return '（空）'
  const lines: string[] = []
  for (const x of g.groups) lines.push(`${x.id} [组] ${x.label}`)
  const line = (n: Node, depth: number) => {
    const extra: string[] = []
    if (n.aliases.length) extra.push(`又称 ${n.aliases.join('/')}`)
    if (n.group) extra.push(`在 ${n.group}`)
    if (n.marks.length) extra.push(n.marks.join('、'))
    if (n.icon) extra.push(`图标 ${n.icon}`)
    const shown = n.text ? (n.text.length > REGION_TEXT ? `${n.text.slice(0, REGION_TEXT)}…` : n.text) : n.label
    lines.push(`${'  '.repeat(depth)}${n.id} ${TAG[n.kind]}${shown}${extra.length ? `（${extra.join('，')}）` : ''}`)
    for (const c of children(g, n.id)) line(c, depth + 1)
  }
  for (const n of g.nodes) if (!n.parent || !g.nodes.some((p) => p.id === n.parent)) line(n, 0)
  // 连线带上名字：只给编号时，小模型经常删错线
  const name = (id: string) => g.nodes.find((n) => n.id === id)?.label ?? ''
  for (const e of g.edges)
    lines.push(`${e.from}(${name(e.from)}) -> ${e.to}(${name(e.to)})${e.label ? ` ${e.label}` : ''}`)
  return lines.join('\n')
}
