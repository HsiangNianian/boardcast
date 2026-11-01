import assert from 'node:assert/strict'
import { test } from 'node:test'
import { correct, formatGlossary, parseGlossary } from './glossary.ts'
import { userMessage } from './prompt.ts'
import { emptyGraph } from './graph.ts'

const terms = parseGlossary(`
MediaCrawler = 媒体爬虫, 美的爬虫
graphRAG
ClickHouse = 克里克豪斯
订单中心 = 订单中兴   # 常听错
API 网关
`)

test('the glossary is one term per line with optional misheard forms', () => {
  assert.deepEqual(
    terms.map((t) => t.term),
    ['MediaCrawler', 'graphRAG', 'ClickHouse', '订单中心', 'API 网关'],
  )
  assert.deepEqual(terms[0].also, ['媒体爬虫', '美的爬虫'])
  assert.equal(parseGlossary(formatGlossary(terms)).length, 5)
})

test('recognized text is corrected to the standard spelling, whole words only', () => {
  assert.equal(correct('爬虫基于 media crawler 改', terms), '爬虫基于 MediaCrawler 改')
  assert.equal(correct('基于美的爬虫改一下', terms), '基于MediaCrawler改一下')
  assert.equal(correct('结果喂给 Graph Rag 存起来', terms), '结果喂给 graphRAG 存起来')
  assert.equal(correct('结果喂给 GRAPHRAG', terms), '结果喂给 graphRAG')
  assert.equal(correct('写进克里克豪斯', terms), '写进ClickHouse')
  assert.equal(correct('订单中兴先校验', terms), '订单中心先校验')
  assert.equal(correct('请求先过 api 网关', terms), '请求先过 API 网关')
  // 不是整词的不改
  assert.equal(correct('mygraphrag2 不是术语', terms), 'mygraphrag2 不是术语')
  assert.equal(correct('没有术语的句子', []), '没有术语的句子')
})

test('the model sees the glossary before the board', () => {
  const msg = userMessage(emptyGraph(), [], { text: '先说爬虫' }, terms)
  assert.match(msg, /^术语（起名照这个写法）：MediaCrawler、graphRAG、ClickHouse、订单中心、API 网关\n白板：/)
  assert.doesNotMatch(userMessage(emptyGraph(), [], { text: '先说爬虫' }), /术语/)
})
