const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const vm = require('node:vm')

const html = fs.readFileSync(path.join(__dirname, '../src/replay_page.html'), 'utf8')
const fn = html.slice(html.indexOf('  function transcriptLines('), html.indexOf('  function showBar('))
assert.ok(fn.includes('function transcriptLines(count)'))
// The export is in audio order; i still refers to the append-only stored transcript.
const DATA = { lines: [
  { i: 2, k: 'first' }, { i: 1 }, { i: 4, k: 'first' },
  { i: 6, k: 'second' }, { i: 0, k: 'first' }, { i: 5 },
  { i: 3, k: 'second' }, { i: 7 }, { i: 8 },
] }
const read = vm.runInNewContext(fn + '\ntranscriptLines', { DATA })
const expected = [
  [], [0], [0, 1], [2, 1, 0], [2, 1, 0, 3], [2, 1, 4, 3, 0],
  [2, 1, 4, 3, 0, 5], [2, 1, 4, 6, 0, 5, 3],
  [2, 1, 4, 6, 0, 5, 3, 7], [2, 1, 4, 6, 0, 5, 3, 7, 8],
]
const original = JSON.stringify(DATA)
for (let count = 0; count <= DATA.lines.length; count++) {
  const result = Array.from(read(count), line => line.i)
  assert.deepEqual(result, expected[count], `replay receipt count ${count}`)
  assert.ok(result.every(index => index < count), 'future captions leaked into replay')
}
assert.equal(JSON.stringify(DATA), original)
console.log('Replay caption order: all 10 prefixes passed; stored data unchanged')
