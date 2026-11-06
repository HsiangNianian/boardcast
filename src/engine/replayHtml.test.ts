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
  messy: 0,
  weight: 0.65,
  englishWeight: 1.25,
  summary: '### 结论\n- 好',
  lines: [{ text: `危险的 </script><script>alert(1)</script> 和${String.fromCharCode(0x2028)}换行` }],
  pages: [{ graph: g, steps: [{ t: 0, line: 1, d: diffGraph(emptyGraph(), g)! }] }],
}

test('replay bundles board text and generated headings, not transcript-only text', () => {
  assert.deepEqual(usedHan(data).sort(), [...new Set('用户网关请求待办补充与问题待定')].sort())
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
  assert.deepEqual(JSON.parse(json), JSON.parse(JSON.stringify({ ...data, lang: 'zh' })))
})

test('the exported language controls the HTML declaration, title and player data', (t) => {
  t.after(() => setLangChoice('zh'))
  for (const lang of ['en', 'zh'] as const) {
    setLangChoice(lang === 'en' ? 'zh' : 'en')
    const html = replayHtml({ ...data, lang }, '', {})
    assert.ok(html.includes(`<html lang="${lang === 'zh' ? 'zh-CN' : 'en'}">`))
    assert.ok(html.includes(lang === 'en' ? '· boardcast replay</title>' : '· boardcast 回放</title>'))
    assert.equal(JSON.parse(/window\.__boardcastData = (.*);\n/.exec(html)![1]).lang, lang)
  }
  setLangChoice('en')
  const html = replayHtml(data, '', {})
  assert.match(html, /<html lang="en">/)
  assert.equal(data.lang, undefined, 'serializing must not mutate the source meeting')
})
