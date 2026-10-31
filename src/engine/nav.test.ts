import assert from 'node:assert/strict'
import { test } from 'node:test'
import { navTarget, parseNav } from './nav.ts'

test('a whole sentence that only asks to turn the page is a command', () => {
  assert.deepEqual(parseNav('翻到第三页'), { to: 'page', n: 3 })
  assert.deepEqual(parseNav('我们看第十二页吧。'), { to: 'page', n: 12 })
  assert.deepEqual(parseNav('好，去第 5 张板') ?? parseNav('好，去第5张板'), { to: 'board', n: 5 })
  assert.deepEqual(parseNav('下一页'), { by: 1 })
  assert.deepEqual(parseNav('回到上一页'), { by: -1 })
})

test('talk that merely mentions a page is left to the model', () => {
  assert.equal(parseNav('workflow-based automation，这里其实是第三页的内容了'), null)
  assert.equal(parseNav('第三页那个表有问题'), null)
  assert.equal(parseNav('下一页要讲的是部署'), null)
})

test('page numbers point into the imported file, nearest copy first', () => {
  const boards = [{}, { no: 1 }, { no: 2 }, { no: 5 }, { no: 1 }, { no: 2 }]
  assert.equal(navTarget({ to: 'page', n: 2 }, boards, 1), 2)
  assert.equal(navTarget({ to: 'page', n: 2 }, boards, 4), 5)
  assert.equal(navTarget({ to: 'page', n: 3 }, boards, 1), -1, '这一页没建板，不要翻到别的板上')
  assert.equal(navTarget({ to: 'page', n: 2 }, [{}, {}, {}], 0), 1, '没有导入文件时页就是板')
  assert.equal(navTarget({ to: 'board', n: 1 }, boards, 3), 0)
  assert.equal(navTarget({ by: -1 }, boards, 0), -1)
})
