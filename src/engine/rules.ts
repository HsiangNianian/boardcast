// 快车道：不用模型，几毫秒内给出反馈。
// 1. 提到白板上已有的东西，立刻高亮；
// 2. 明确的口令（「画一下 A 到 B」「A 指向 B」「A -> B」）直接解析成铅笔稿。
// 模型那一遍跑完后，没被确认的铅笔稿会被擦掉（见 eraseDrafts）。

import { type Graph, norm } from './graph.ts'
import type { Op } from './ops.ts'

/** 这句话提到了哪些已有节点。 */
export function mentions(g: Graph, text: string): string[] {
  const t = norm(text)
  return g.nodes
    .filter((n) => [n.label, ...n.aliases].some((s) => norm(s).length >= 2 && t.includes(norm(s))))
    .map((n) => n.id)
}

const CUE = /(?:画一下|画个图|画出来|记一下|白板上)[:：，,\s]*/
const LINK = /(.{1,12}?)\s*(?:->|→|指向|连到|连向|调用|传给|发给|转给|到)\s*(.{1,12}?)(?=[，,。；;、]|$|然后|再)/g

function clean(s: string): string {
  return s.replace(/^(?:从|先|把|让|由)/, '').replace(/[，,。；;\s]+$/g, '').trim()
}

/** 口令解析。只认带口令或箭头的句子，宁可漏，不可乱画。 */
export function quickOps(text: string): Op[] {
  const arrow = /->|→/.test(text)
  const cue = CUE.exec(text)
  if (!cue && !arrow) return []
  const body = cue ? text.slice(cue.index + cue[0].length) : text
  const ops: Op[] = []
  // 「A 到 B 再到 C」这种链式说法：把每一段的终点当成下一段的起点
  const chain = body.split(/(?:然后|再)(?:到|转给|传给|发给)?/)
  let prev: string | undefined
  for (const seg of chain) {
    LINK.lastIndex = 0
    const m = LINK.exec(seg)
    if (m) {
      const a = clean(m[1])
      const b = clean(m[2])
      if (a && b && a !== b) ops.push({ op: 'E', from: { name: a }, to: { name: b } })
      prev = b
    } else if (prev) {
      const b = clean(seg)
      if (b && b.length <= 12 && b !== prev) ops.push({ op: 'E', from: { name: prev }, to: { name: b } })
      prev = b
    }
  }
  return ops
}
