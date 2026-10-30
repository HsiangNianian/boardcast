import assert from 'node:assert/strict'
import { test } from 'node:test'
import { remove, rename, toggleMark } from './edit.ts'
import { type Graph, emptyGraph } from './graph.ts'
import { applyOps, eraseDrafts, parseOps } from './ops.ts'
import { type Step, diffGraph, inkOnly, patchGraph, replayTo, replays } from './steps.ts'

test('recording every change and patching them back gives the same board at each step', () => {
  const states: Graph[] = []
  let g = emptyGraph()
  const steps: Step[] = []
  let last = emptyGraph()
  const record = (next: Graph) => {
    const ink = inkOnly(next)
    const d = diffGraph(last, ink)
    if (d) {
      steps.push({ t: steps.length, line: steps.length, d })
      states.push(ink)
      last = ink
    }
  }
  // 铅笔稿先占掉编号，然后被擦掉：只重放操作的话后面的编号会对不上
  g = applyOps(g, parseOps('N 「草稿」'), 'draft')
  record(g)
  g = eraseDrafts(applyOps(g, parseOps('E 「用户」 -> 「网关」 : 「请求」'), 'ink'))
  record(g)
  g = applyOps(g, parseOps('T 「爬虫」\nL 「爬虫」 「监控任务」\nG 「入口」 n2'), 'ink')
  record(g)
  // 手动修改：改名、加标记、删除
  g = rename(g, { kind: 'node', id: 'n3' }, 'API 网关')
  record(g)
  g = toggleMark(g, 'n5', '星标')
  record(g)
  g = remove(g, { kind: 'edge', from: 'n2', to: 'n3' })
  record(g)
  // 没变化不记
  record(g)
  assert.equal(steps.length, states.length)
  for (let i = 0; i < steps.length; i++) assert.deepEqual(replayTo(steps, i + 1), states[i], `第 ${i + 1} 步对不上`)
  assert.ok(replays(steps, g))
  assert.deepEqual(replayTo(steps, 0), emptyGraph())
})

test('a reorder is recorded, and an unchanged board gives no diff', () => {
  const a = applyOps(emptyGraph(), parseOps('N 「甲」\nN 「乙」\nN 「丙」'))
  assert.equal(diffGraph(a, a), null)
  const b = structuredClone(a)
  b.nodes.reverse()
  const d = diffGraph(a, b)!
  assert.deepEqual(d.nodes?.order, ['n3', 'n2', 'n1'])
  assert.deepEqual(patchGraph(a, d), b)
})

test('diffs only carry what changed', () => {
  const a = applyOps(emptyGraph(), parseOps('N 「甲」\nN 「乙」\nE 「甲」 -> 「乙」'))
  const b = applyOps(a, parseOps('N 「丙」'))
  const d = diffGraph(a, b)!
  assert.deepEqual(
    d.nodes?.set?.map((n) => n.label),
    ['丙'],
  )
  assert.equal(d.edges, undefined)
  assert.equal(d.nextNode, 4)
})
