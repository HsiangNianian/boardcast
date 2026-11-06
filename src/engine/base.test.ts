import assert from 'node:assert/strict'
import { test } from 'node:test'
import { inherited, oldTextOpacity } from './base.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'

test('old text keeps contrast in both board themes without fading unreadable custom colors further', () => {
  const rgb = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
  const luminance = (channels: number[]) => channels.reduce((sum, c, i) => sum + (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4) * [0.2126, 0.7152, 0.0722][i], 0)
  for (const [colors, paper] of [[['#1d1f24', '#d02e2e', '#1f56c4'], '#fbfbf8'], [['#eeeee6', '#ff9c8a', '#9cc9f5'], '#22302a']] as const) {
    for (const color of colors) {
      const opacity = oldTextOpacity(color, paper), bg = rgb(paper)
      const a = luminance(rgb(color).map((c, i) => c * opacity + bg[i] * (1 - opacity))), b = luminance(bg)
      assert.ok((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) >= 4.5, `${color} on ${paper}`)
    }
  }
  assert.equal(oldTextOpacity('#cccccc', '#ffffff'), 1)
  assert.equal(oldTextOpacity('#000', '#ffffff', 0.4), 0.4)
  assert.equal(oldTextOpacity('transparent', '#ffffff'), 1)
})

test('a new mark on a repeated word at different coordinates is not inherited', () => {
  const base = board('N 「Repeated words」')
  const span = { text: 'word', mark: '下划线' as const, boxes: [{ x: 10, y: 20, w: 30, h: 15 }] }
  base.nodes[0].spans = [span]
  assert.equal(inherited(base).span(base.nodes[0], span), true)
  assert.equal(inherited(base).span(base.nodes[0], { ...span, boxes: [{ ...span.boxes[0], y: 50 }] }), false)
})

const board = (text: string, g = emptyGraph()) => applyOps(g, parseOps(text))

test('what stayed as it was on the base board is old; what this meeting added or changed is not', () => {
  const base = board('E 「订单服务」 -> 「库存服务」 : 「扣库存」\nE n1 -> 「消息队列」\nT 「待办」\nL n4 「压测」\nG 「后端」 n1 n2\nM n5 下划线')
  // 这场会：加风控、划掉消息队列、库存服务改名、压测加星标、扣库存的标签改了
  const now = board('E n1 -> 「风控服务」\nM n3 划掉\nR n2 「库存中心」\nM n5 星标\nE n1 -> n2 : 「锁库存」', base)
  const old = inherited(base)
  const byName = (name: string) => now.nodes.find((n) => n.label === name)!

  assert.equal(old.node(byName('订单服务')), true)
  assert.equal(old.label(byName('订单服务')), true)
  // 改了名字：框是旧的，名字是新的
  assert.equal(old.node(byName('库存中心')), true)
  assert.equal(old.label(byName('库存中心')), false)
  assert.equal(old.node(byName('风控服务')), false)
  // 今天划掉的旧节点：字是旧的，那一道是新的
  assert.equal(old.label(byName('消息队列')), true)
  assert.equal(old.mark(byName('消息队列'), '划掉'), false)
  assert.equal(old.mark(byName('压测'), '下划线'), true)
  assert.equal(old.mark(byName('压测'), '星标'), false)

  assert.equal(old.edge({ from: 'n1', to: 'n3' }), true)
  assert.equal(old.edge({ from: 'n1', to: 'n6' }), false)
  // 线是旧的，标签改过
  const relabeled = now.edges.find((e) => e.from === 'n1' && e.to === 'n2')!
  assert.equal(old.edge(relabeled), true)
  assert.equal(old.edge(relabeled, true), false)
  assert.equal(old.group(now.groups[0]), true)
  assert.equal(old.group({ ...now.groups[0], label: '服务端' }), false)
})

test('without a base board nothing is old', () => {
  const g = board('E 「A」 -> 「B」')
  const old = inherited(undefined)
  assert.equal(old.node(g.nodes[0]), false)
  assert.equal(old.label(g.nodes[0]), false)
  assert.equal(old.edge(g.edges[0]), false)
})
