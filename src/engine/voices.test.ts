import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type Voice, identify, nextName, renameVoice } from './voices.ts'

// 造几个「人」：每人一个随机方向（维数高，彼此几乎正交，和真的声纹一样），每句在这个方向上加一点噪声
const DIM = 192
function random(seed: number): () => number {
  let s = seed * 2654435761
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648
    return s / 1073741824 - 1
  }
}
function person(seed: number): number[] {
  const r = random(seed)
  return Array.from({ length: DIM }, r)
}
function say(p: number[], noise: number, k: number): number[] {
  const r = random(1000 + k)
  const v = p.map((x) => x + noise * r())
  const len = Math.hypot(...v)
  return v.map((x) => x / len)
}

test('the same voice keeps its name and a new voice gets the next letter', () => {
  const [p, q] = [person(1), person(2)]
  let voices: Voice[] = []
  const names: string[] = []
  for (const [i, who] of [p, p, q, p, q, q].entries()) {
    const r = identify(voices, say(who, 0.3, i))
    voices = r.voices
    names.push(r.name)
  }
  assert.deepEqual(names, ['A', 'A', 'B', 'A', 'B', 'B'])
  assert.equal(voices.length, 2)
  assert.equal(voices.find((v) => v.name === 'A')!.n, 3)
})

test('new speakers skip names that are already used in the meeting', () => {
  assert.equal(nextName(['A', 'B', '张三']), 'C')
  assert.equal(nextName(Array.from({ length: 26 }, (_, i) => String.fromCharCode(65 + i))), 'A2')
  const r = identify([], say(person(3), 0, 0), ['A'])
  assert.equal(r.name, 'B')
})

test('renaming a voice to an existing name merges the two', () => {
  const p = person(4)
  let voices: Voice[] = []
  voices = identify(voices, say(p, 0.2, 1)).voices
  voices = identify(voices, say(person(5), 0, 0)).voices
  assert.deepEqual(
    voices.map((v) => v.name),
    ['A', 'B'],
  )
  const renamed = renameVoice(voices, 'A', '张三')
  assert.deepEqual(renamed.map((v) => v.name).sort(), ['B', '张三'].sort())
  // 「B」其实也是张三：合并成一个，之后张三的声音认成张三
  const merged = renameVoice(renamed, 'B', '张三')
  assert.equal(merged.length, 1)
  assert.equal(merged[0].n, 2)
  assert.equal(identify(merged, say(p, 0.2, 9)).name, '张三')
})
