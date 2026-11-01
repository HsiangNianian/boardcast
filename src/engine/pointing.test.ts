import assert from 'node:assert/strict'
import { test } from 'node:test'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { latest } from './pointing.ts'
import { type Utterance, pointingLine, userMessage } from './prompt.ts'
import { Session } from './session.ts'

const board = () => applyOps(emptyGraph(), parseOps('E 「订单服务」 -> 「库存服务」\nN 「数据库」'))

test('keeps the latest things circled, oldest first, without repeats', () => {
  assert.deepEqual(latest(['n1', 'n2', 'n3'], 2), ['n2', 'n3'])
  assert.deepEqual(latest(['n1', 'n2', 'n1']), ['n2', 'n1'])
  assert.deepEqual(latest(Array.from({ length: 10 }, (_, i) => `n${i}`)), ['n2', 'n3', 'n4', 'n5', 'n6', 'n7', 'n8', 'n9'])
  assert.deepEqual(latest([]), [])
})

test('the prompt says what is pointed at, right before the new line, and only then', () => {
  const g = board()
  const said = { speaker: '小王', text: '把这俩连起来' }
  const plain = userMessage(g, [], said)
  assert.ok(!plain.includes('手指着'))
  const msg = userMessage(g, [], { ...said, pointing: ['n2', 'n3'] })
  assert.ok(msg.endsWith('当前手指着：n2（库存服务）、n3（数据库）\n新的一句：小王：把这俩连起来'), msg)
  // 除了这一行，别的都和没指时一样（前缀缓存照样能用）
  assert.equal(msg.replace('当前手指着：n2（库存服务）、n3（数据库）\n', ''), plain)
  // 指着的东西已经被删了：不提
  assert.equal(pointingLine(g, ['n9']), undefined)
  assert.equal(pointingLine(g, ['n9', 'n1']), '当前手指着：n1（订单服务）')
})

test('a long block of imported text is named by its first few characters', () => {
  const g = emptyGraph()
  g.nodes.push({ id: 'n1', label: '库存扣减在下单时同步完成，超时后由补偿任务回滚', aliases: [], kind: 'region', marks: [], ink: 'ink' })
  assert.equal(pointingLine(g, ['n1']), '当前手指着：n1（库存扣减在下单时同步完成，超时后…）')
})

test('lines batched while the model is busy carry what was pointed at', async () => {
  const sent: string[] = []
  const recorded: Utterance[] = []
  let release: () => void = () => {}
  const first = new Promise<void>((r) => (release = r))
  const s = new Session({
    baseUrl: 'http://fake',
    send: async (b) => {
      const msgs = b.messages as { content: string }[]
      sent.push(msgs[msgs.length - 1].content)
      if (sent.length === 1) await first
      return { choices: [{ message: { content: '-' } }] }
    },
  }, { llm: (r) => recorded.push(r.said) })
  s.graph = board()
  s.say({ text: '先说一句' })
  s.say({ text: '从这个', pointing: ['n1'] })
  s.say({ text: '连到这个', pointing: ['n3'] })
  release()
  await s.drain()
  assert.deepEqual(recorded, [{ text: '先说一句' }, { text: '从这个 连到这个', pointing: ['n1', 'n3'] }])
  assert.equal(sent.length, 2)
  assert.ok(!sent[0].includes('手指着'))
  assert.ok(sent[1].endsWith('当前手指着：n1（订单服务）、n3（数据库）\n新的一句：从这个 连到这个'), sent[1])
})
