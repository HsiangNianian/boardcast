import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseArxiv } from '../board/arxiv.ts'

test('arXiv ids are recognized in links, prefixed ids and bare ids', () => {
  const cases: [string, string | null][] = [
    ['https://arxiv.org/abs/1512.03385', '1512.03385'],
    ['https://arxiv.org/pdf/1706.03762v7', '1706.03762v7'],
    ['arxiv.org/pdf/2401.12345.pdf', '2401.12345'],
    ['https://arxiv.org/html/2401.12345v2', '2401.12345v2'],
    ['arXiv:1512.03385', '1512.03385'],
    ['  2401.01234  ', '2401.01234'],
    ['https://arxiv.org/abs/cs/0112017', 'cs/0112017'],
    ['math.GT/0309136v2', 'math.GT/0309136v2'],
    ['https://example.com/1512.03385.pdf', null],
    ['1512.033', null],
    ['不是链接', null],
    ['', null],
  ]
  for (const [input, want] of cases) assert.equal(parseArxiv(input), want, input)
})
