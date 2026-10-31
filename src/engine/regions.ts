// 导入页（PDF 的一页、一张图片）变成一块板：页面大小记在 underlay 上，
// 页面上一块块内容（一段字、一个标题、一张表）变成钉在原位的「原文」节点，模型开会时只能在上面圈画、连线、写批注。

import { type Graph, emptyGraph } from './graph.ts'

/** 页面上的一块内容，坐标是板书坐标（页面左上角是原点） */
export interface Area {
  text: string
  /** 起好的名字（图 3：……、公式 (5)、§3 方法）；没有就用文字开头 */
  name?: string
  x: number
  y: number
  w: number
  h: number
}

/** 名字太长时截一截：模型看到的实体表越短越好，指认一块原文看开头就够了 */
const MAX = 36

export function pageGraph(size: { w: number; h: number }, areas: Area[]): Graph {
  const g = emptyGraph()
  g.underlay = { w: Math.round(size.w), h: Math.round(size.h) }
  const used = new Map<string, number>()
  for (const a of areas) {
    const flat = (a.name ?? a.text).replace(/[「」]/g, '').replace(/\s+/g, ' ').trim()
    if (!flat) continue
    let label = flat.length > MAX ? `${flat.slice(0, MAX - 1)}…` : flat
    // 同样开头的两块（比如表格里的两行）不能合成一个
    const n = (used.get(label) ?? 0) + 1
    used.set(label, n)
    if (n > 1) label = `${label}（${n}）`
    g.nodes.push({
      id: `n${g.nextNode++}`,
      label,
      aliases: [],
      kind: 'region',
      marks: [],
      ink: 'ink',
      at: { x: Math.round(a.x), y: Math.round(a.y), w: Math.round(a.w), h: Math.round(a.h) },
    })
  }
  return g
}
