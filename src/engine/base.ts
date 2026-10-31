// 带着旧板开会（#49）：拿以前某场会的一张板当底板，在上面接着画。
// 底板上原样留下来的东西用淡墨，这场会新加的、改了名字的、新做的标记用正常墨迹，
// 一眼看出这次讨论改了哪儿。这里只回答「这一块是不是底板上原样留下来的」，怎么画交给 ink/compose.ts。

import type { Edge, Graph, Group, Mark, Node, Span } from './graph.ts'

export interface Inherited {
  /** 节点在底板上就有（名字改没改都算：框还是原来那个） */
  node(n: Node | undefined): boolean
  /** 名字也没改 */
  label(n: Node | undefined): boolean
  /** 这个标记底板上就有 */
  mark(n: Node, m: Mark): boolean
  /** 圈画在原文某个词、某句话上的，底板上就有 */
  span(n: Node, s: Span): boolean
  /** 图标没换 */
  icon(n: Node): boolean
  /** 连线底板上就有；`label` 为 true 时标签也得一样 */
  edge(e: Pick<Edge, 'from' | 'to' | 'label'>, label?: boolean): boolean
  /** 分组底板上就有，名字也一样 */
  group(x: Pick<Group, 'id' | 'label'>): boolean
}

/** 没有底板时什么都不算旧的 */
export function inherited(base?: Graph): Inherited {
  const nodes = new Map((base?.nodes ?? []).map((n) => [n.id, n]))
  const edges = new Map((base?.edges ?? []).map((e) => [`${e.from}>${e.to}`, e]))
  const groups = new Map((base?.groups ?? []).map((x) => [x.id, x]))
  return {
    node: (n) => !!n && nodes.has(n.id),
    label: (n) => !!n && nodes.get(n.id)?.label === n.label,
    mark: (n, m) => !!nodes.get(n.id)?.marks.includes(m),
    span: (n, s) => !!nodes.get(n.id)?.spans?.some((x) => x.mark === s.mark && x.text === s.text),
    icon: (n) => nodes.has(n.id) && nodes.get(n.id)?.icon === n.icon,
    edge: (e, label = false) => {
      const w = edges.get(`${e.from}>${e.to}`)
      return !!w && (!label || w.label === e.label)
    },
    group: (x) => groups.get(x.id)?.label === x.label,
  }
}
