import type { Graph } from '../engine/graph.ts'
import { type Box, type LayoutMemory, StableLayout, routeSignature } from './layout.ts'

const GAP = 96

/** Explicit whole-board composition; live recording continues to use StableLayout. */
export async function beautify(graph: Graph, memory: LayoutMemory): Promise<LayoutMemory> {
  const draft = new StableLayout()
  draft.restore(memory)
  let layout = await draft.layout(graph)
  let next = draft.snapshot()
  if (layout.diagram) {
    // The compiler stays out of startup and exported replay HTML. Never send meeting text to a server.
    const { D2 } = await import('@d2lang/d2')
    const d2 = new D2()
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      const nodes = layout.diagram.nodes
      const names = new Map<string, string>()
      const groups = new Map(graph.groups.map((g, i) => [g.id, `g${i}`]))
      const declarations = nodes.map((b, i) => {
        const group = groups.get(graph.nodes.find((n) => n.id === b.id)?.group ?? '')
        const name = `${group ? `${group}.` : ''}n${i}`
        names.set(b.id, name)
        return `${name}: "" {width: ${Math.ceil(b.w)}; height: ${b.h}}`
      })
      const edges = graph.edges.filter((e) => names.has(e.from) && names.has(e.to))
      const input = [
        'vars: {d2-config: {data: {tala-seeds: [1]}}}',
        ...(nodes.length <= 6 ? ['direction: right'] : []),
        ...[...groups.values()].filter((id) => [...names.values()].some((n) => n.startsWith(`${id}.`))).map((id) => `${id}: ""`),
        ...declarations,
        ...edges.map((e) => `${names.get(e.from)} -> ${names.get(e.to)}: ${JSON.stringify(e.label ?? '')}`),
      ].join('\n')
      const result = await Promise.race([
        d2.compile(input, { layout: 'tala' }),
        new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error('Board layout timed out')), 20_000) }),
      ])
      const shapes = new Map(result.diagram.shapes.map((s) => [s.id, s]))
      const positions = new Map<string, Box>()
      for (const b of nodes) {
        const shape = shapes.get(names.get(b.id)!)
        if (!shape || !Number.isFinite(shape.pos.x) || !Number.isFinite(shape.pos.y)) throw new Error('Incomplete board layout')
        positions.set(b.id, { ...b, x: shape.pos.x, y: shape.pos.y })
      }
      next.nodeAt = [...positions]
      next.routeAt = edges.flatMap((e) => {
        const c = result.diagram.connections.find((c) => c.src === names.get(e.from) && c.dst === names.get(e.to))
        const points = c?.route?.map((p) => [p.x, p.y] as [number, number]) ?? []
        if (points.length < 2 || points.some((p) => !p.every(Number.isFinite))) return []
        return [[`${e.from}>${e.to}`, { sig: routeSignature(e, positions), points }]]
      })
      next.regionAt = []
      next.boxAt = []
      draft.restore(next)
      layout = await draft.layout(graph)
      next = draft.snapshot()
    } finally {
      clearTimeout(timer)
      await d2.dispose()
    }
  }

  const order = new Map(graph.nodes.map((n, i) => [n.id, i]))
  const blocks: Box[] = [...layout.regions.filter((r) => r.id !== '__margin'), ...(layout.diagram ? [layout.diagram.box] : [])]
  const at = (b: Box) => b.id === '__diagram'
    ? Math.min(...layout.diagram!.nodes.map((n) => order.get(n.id)!))
    : b.id === '__loose' ? Infinity : (order.get(b.id) ?? Infinity)
  blocks.sort((a, b) => at(a) - at(b))
  // A short opening topic reads as the board heading; the rest share aligned rows and generous gutters.
  const first = layout.regions.find((r) => r.id === blocks[0]?.id)
  const heading = first?.head && first.rows.length <= 2 ? blocks.shift() : undefined
  const minWidth = Math.max(1, ...blocks.map((b) => b.w))
  const target = Math.max(minWidth, Math.sqrt(blocks.reduce((s, b) => s + (b.w + GAP) * (b.h + GAP), 0) * 1.6))
  const pack = (width: number) => {
    let x = 0, y = heading ? heading.h + GAP : 0, rowHeight = 0
    const packed: Box[] = heading ? [{ ...heading, x: 0, y: 0 }] : []
    for (const b of blocks) {
      if (x && x + b.w > width) { x = 0; y += rowHeight + GAP; rowHeight = 0 }
      packed.push({ ...b, x, y })
      x += b.w + GAP
      rowHeight = Math.max(rowHeight, b.h)
    }
    return packed
  }
  const cost = (boxes: Box[]) => {
    const w = Math.max(1, ...boxes.map((b) => b.x + b.w))
    const h = Math.max(1, ...boxes.map((b) => b.y + b.h))
    return Math.max(w / 1.6, h) + w * h / 100_000
  }
  const packed = [minWidth, target, target * 1.25, target * 1.6].map(pack).sort((a, b) => cost(a) - cost(b))[0]
  const origins = new Map(next.regionAt)
  const before = new Map([...layout.regions, ...(layout.diagram ? [layout.diagram.box] : [])].map((b) => [b.id, b]))
  for (const b of packed) {
    const old = before.get(b.id)!
    const origin = origins.get(b.id) ?? { x: old.x, y: old.y }
    origins.set(b.id, { x: origin.x + b.x - old.x + (layout.page ? layout.page.w + GAP : 0), y: origin.y + b.y - old.y })
  }
  draft.restore({ ...next, regionAt: [...origins], boxAt: [] })
  await draft.layout(graph)
  return draft.snapshot()
}
