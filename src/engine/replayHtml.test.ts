import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type ReplayData, replayHtml, usedHan } from '../player/data.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { diffGraph } from './steps.ts'
import { setLangChoice } from '../i18n/index.ts'

// These tests check the Chinese wording
setLangChoice('zh')

const g = applyOps(emptyGraph(), parseOps('E 「用户」 -> 「网关」 : 「请求」\nT 「待办」'))
const data: ReplayData = {
  title: '周会 <b>',
  createdAt: 0,
  summary: '### 结论\n- 好',
  lines: [{ text: `危险的 </script><script>alert(1)</script> 和${String.fromCharCode(0x2028)}换行` }],
  pages: [{ graph: g, steps: [{ t: 0, line: 1, d: diffGraph(emptyGraph(), g)! }] }],
}

test('the replay page only needs the characters written on the boards', () => {
  assert.deepEqual(usedHan(data).sort(), [...'用户网关请求待办'].sort())
})

test('meeting text cannot break out of the embedded script', () => {
  const html = replayHtml(data, 'console.log("player")', { 用: [[[0, 0]]] })
  // 只有我们自己的两个 <script>，会议里的「</script>」被转义了
  assert.equal(html.match(/<script>/g)?.length, 2)
  assert.equal(html.match(/<\/script>/g)?.length, 2)
  assert.ok(!html.includes(String.fromCharCode(0x2028)))
  assert.match(html, /<title>周会 &lt;b&gt; · boardcast 回放<\/title>/)
  // 数据原样还原得回来
  const json = /window\.__boardcastData = (.*);\n/.exec(html)![1]
  assert.deepEqual(JSON.parse(json), JSON.parse(JSON.stringify(data)))
})
