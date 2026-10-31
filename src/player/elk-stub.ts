// 回放播放器里替代 ELK 的小排版（ELK 有 1.5 MB）：播放器一开始就放回了每张板最终的排版，
// 几乎用不上它；万一用上（回放中途流程图被清空过），就按连线分层、从左往右排，连线画直线。
// 只实现 layout.ts 用到的那一点接口。

interface N {
  id: string
  width?: number
  height?: number
  x?: number
  y?: number
  children?: N[]
  layoutOptions?: Record<string, string>
  edges?: E[]
}
interface E {
  id: string
  sources: string[]
  targets: string[]
  sections?: { startPoint: { x: number; y: number }; endPoint: { x: number; y: number }; bendPoints?: { x: number; y: number }[] }[]
}

export default class ElkStub {
  async layout(root: N & { edges?: E[] }): Promise<N> {
    const leaves: N[] = []
    const parentOf = new Map<string, N>()
    for (const c of root.children ?? []) {
      if (c.children?.length) for (const k of c.children) (leaves.push(k), parentOf.set(k.id, c))
      else leaves.push(c)
    }
    // 分层：最长路径
    const layer = new Map(leaves.map((n) => [n.id, 0]))
    for (let pass = 0; pass < leaves.length; pass++)
      for (const e of root.edges ?? []) {
        const a = layer.get(e.sources[0])
        const b = layer.get(e.targets[0])
        if (a !== undefined && b !== undefined && b <= a && a + 1 < leaves.length) layer.set(e.targets[0], a + 1)
      }
    const rows = new Map<number, number>()
    const abs = new Map<string, { x: number; y: number; w: number; h: number }>()
    for (const n of leaves) {
      const l = layer.get(n.id)!
      const r = rows.get(l) ?? 0
      rows.set(l, r + 1)
      abs.set(n.id, { x: 24 + l * 260, y: 44 + r * 110, w: n.width ?? 120, h: n.height ?? 56 })
    }
    // 分组：成员的外框加边距；成员坐标改成相对分组
    for (const c of root.children ?? []) {
      if (!c.children?.length) continue
      const bs = c.children.map((k) => abs.get(k.id)!)
      c.x = Math.min(...bs.map((b) => b.x)) - 24
      c.y = Math.min(...bs.map((b) => b.y)) - 44
      c.width = Math.max(...bs.map((b) => b.x + b.w)) + 24 - c.x
      c.height = Math.max(...bs.map((b) => b.y + b.h)) + 24 - c.y
      for (const k of c.children) {
        const b = abs.get(k.id)!
        k.x = b.x - c.x
        k.y = b.y - c.y
      }
    }
    for (const n of leaves) if (!parentOf.has(n.id)) ((n.x = abs.get(n.id)!.x), (n.y = abs.get(n.id)!.y))
    for (const e of root.edges ?? []) {
      const a = abs.get(e.sources[0])
      const b = abs.get(e.targets[0])
      if (!a || !b) continue
      e.sections = [{ startPoint: { x: a.x + a.w, y: a.y + a.h / 2 }, endPoint: { x: b.x, y: b.y + b.h / 2 } }]
    }
    return root
  }
}
