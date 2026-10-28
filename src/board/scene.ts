// 坐标 → Excalidraw 元素。手绘感来自 Excalidraw 自己的 roughness。
// 铅笔稿：灰色、虚线、半透明；墨线：深色实线。被提到的节点会短暂涂上高亮。

import { convertToExcalidrawElements } from '@excalidraw/excalidraw'
import type { ExcalidrawElementSkeleton } from '@excalidraw/excalidraw/data/transform'
import type { Graph } from '../engine/graph.ts'
import { FONT, type Layout } from './layout.ts'

export interface Look {
  ink: string
  pencil: string
  highlight: string
  group: string
}

export function scene(g: Graph, l: Layout, hot: Set<string>, look: Look) {
  const sk: ExcalidrawElementSkeleton[] = []
  for (const grp of l.groups) {
    sk.push({
      type: 'rectangle',
      id: grp.id,
      x: grp.x,
      y: grp.y,
      width: grp.w,
      height: grp.h,
      strokeColor: look.group,
      strokeStyle: 'dashed',
      roughness: 2,
      roundness: { type: 3 },
    })
    sk.push({
      type: 'text',
      x: grp.x + 14,
      y: grp.y + 10,
      text: grp.label,
      fontSize: FONT - 2,
      strokeColor: look.group,
    })
  }
  for (const b of l.nodes) {
    const n = g.nodes.find((x) => x.id === b.id)
    if (!n) continue
    const draft = n.ink === 'draft'
    sk.push({
      type: 'rectangle',
      id: n.id,
      x: b.x,
      y: b.y,
      width: b.w,
      height: b.h,
      strokeColor: draft ? look.pencil : look.ink,
      strokeStyle: draft ? 'dashed' : 'solid',
      opacity: draft ? 55 : 100,
      backgroundColor: hot.has(n.id) ? look.highlight : 'transparent',
      fillStyle: 'hachure',
      roughness: 1,
      roundness: { type: 3 },
      label: { text: n.label, fontSize: FONT, strokeColor: draft ? look.pencil : look.ink },
    })
  }
  for (const r of l.edges) {
    const e = g.edges.find((x) => x.from === r.from && x.to === r.to)
    if (!e) continue
    const draft = e.ink === 'draft'
    const [x0, y0] = r.points[0]
    sk.push({
      type: 'arrow',
      x: x0,
      y: y0,
      points: r.points.map(([x, y]) => [x - x0, y - y0] as const) as never,
      strokeColor: draft ? look.pencil : look.ink,
      strokeStyle: draft ? 'dashed' : 'solid',
      opacity: draft ? 55 : 100,
      roughness: 1,
      start: { id: e.from },
      end: { id: e.to },
      label: e.label ? { text: e.label, fontSize: FONT - 4 } : undefined,
    })
  }
  return convertToExcalidrawElements(sk, { regenerateIds: false })
}
