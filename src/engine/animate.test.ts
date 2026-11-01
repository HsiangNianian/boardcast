import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Layout } from '../board/layout.ts'
import { animatedSvg, ownerOf, ranks, timeline } from '../ink/animate.ts'
import type { Piece } from '../ink/compose.ts'
import { type Graph, emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { type Step, diffGraph } from './steps.ts'

const noRegions = { regions: [] } as unknown as Layout
const piece = (key: string, strokes = 1, under = false): Piece => ({
  key,
  color: '#111',
  opacity: 1,
  under,
  strokes: Array.from({ length: strokes }, () => ({ d: 'M0 0Z', center: 'M 0 0 L 90 0', length: 90 })),
})

/** 按轮记下回放记录，和 App 一样 */
function record(...rounds: string[]): { g: Graph; steps: Step[] } {
  let g = emptyGraph()
  const steps: Step[] = []
  for (const r of rounds) {
    const next = applyOps(g, parseOps(r))
    steps.push({ t: steps.length, line: steps.length, d: diffGraph(g, next)! })
    g = next
  }
  return { g, steps }
}

test('ownerOf reads which node, edge or group a piece of ink belongs to', () => {
  assert.equal(ownerOf('n3|消息队列'), 'n3')
  assert.equal(ownerOf('n3|box'), 'n3')
  assert.equal(ownerOf('n3|消息队列|s'), 'n3')
  assert.equal(ownerOf('n1>n2'), 'n1>n2')
  assert.equal(ownerOf('n1~n2'), 'n1>n2')
  assert.equal(ownerOf('n1>n2l|发消息'), 'n1>n2')
  assert.equal(ownerOf('n1>n2l|bg'), 'n1>n2')
  assert.equal(ownerOf('g2|后端'), 'g2')
  assert.equal(ownerOf('g2t|后端'), 'g2')
  assert.equal(ownerOf('__loose|待定|ul'), '__loose')
})

test('pieces are ordered by the round they appeared in; a mark waits for the round it was added', () => {
  const { g, steps } = record('N 「网关」', 'N 「鉴权」\nE 「网关」 -> 「鉴权」', 'M 「网关」 星标')
  const keys = ['n1|网关', 'n1|网关|s', 'n2|鉴权', 'n1>n2', 'n1|box']
  assert.deepEqual(ranks(g, noRegions, steps, keys), [0, 2, 1, 1, 0])
  // 没有回放记录（老会议）：按编号先后，连线等两头都在
  assert.deepEqual(ranks(g, noRegions, [], ['n2|鉴权', 'n1|网关', 'n1>n2']), [2, 1, 2])
})

test('the timeline writes stroke after stroke, pauses between rounds, and squeezes long boards into the budget', () => {
  const pieces = [piece('a', 2), piece('b', 1), piece('hl', 1, true)]
  const { timed, end } = timeline(pieces, [0, 1, 1])
  // 每笔 100ms（90px ÷ 0.9），笔间 35ms；换一轮停 300ms
  assert.deepEqual(timed[0].delays, [0, 135])
  assert.deepEqual(timed[1].delays, [270 + 300])
  assert.deepEqual(timed[2].durs, [0], '荧光笔垫底不写，直接出现')
  assert.equal(end, 270 + 300 + 135)
  const many = Array.from({ length: 500 }, (_, i) => piece(`n${i}`, 4))
  const fast = timeline(many, many.map((_, i) => i), 12000)
  assert.ok(fast.end <= 12000 + 1, `${fast.end}`)
  assert.ok(fast.end > 11000)
})

test('the svg animates every stroke with CSS only and falls back to the full board for reduced motion', () => {
  const { timed } = timeline([piece('n1|网关', 2)], [0])
  const svg = animatedSvg(timed, { x: -48, y: -48, w: 300, h: 200 }, '#fbfbf8', { href: 'data:image/png;base64,AA"<', w: 100, h: 50 })
  assert.match(svg, /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg" viewBox="-48 -48 300 200" width="300" height="200">/)
  assert.equal(svg.match(/<mask /g)?.length, 2)
  assert.match(svg, /animation-delay:135ms/)
  assert.match(svg, /prefers-reduced-motion/)
  assert.ok(!/<script/.test(svg))
  assert.match(svg, /href="data:image\/png;base64,AA&quot;&lt;"/, '底图地址转义')
})
