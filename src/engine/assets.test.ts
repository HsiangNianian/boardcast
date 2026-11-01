import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type AssetStore, type Meeting, loadAssets, stashAssets } from '../meetings.ts'
import { emptyGraph } from './graph.ts'

function memoryStore() {
  const files = new Map<string, string>()
  let writes = 0
  const store: AssetStore = {
    put: async (m, k, d) => {
      writes++
      files.set(`${m}/${k}`, d)
    },
    get: async (m, k) => files.get(`${m}/${k}`) ?? null,
  }
  return { store, files, writes: () => writes }
}

const img = (n: number) => `data:image/jpeg;base64,${Buffer.from(`page-${n}-`.repeat(500)).toString('base64')}`
const meeting = (underlays: (string | undefined)[]): Meeting => ({
  version: 1,
  id: 'm1',
  title: 't',
  createdAt: 0,
  updatedAt: 0,
  graph: emptyGraph(),
  history: [],
  lines: [],
  pages: underlays.map((u) => ({ graph: emptyGraph(), underlay: u })),
  page: 0,
})

test('page images are saved once under their content hash and the meeting keeps only references', async () => {
  const { store, files, writes } = memoryStore()
  const stored = new Set<string>()
  const m = meeting([img(1), undefined, img(2), img(1)])
  const out = await stashAssets(m, store, stored)
  const refs = out.pages!.map((p) => p.underlay)
  assert.match(refs[0]!, /^asset:[0-9a-f]{32}\.jpg$/)
  assert.equal(refs[1], undefined)
  assert.equal(refs[3], refs[0], '同一张图只存一份')
  assert.equal(files.size, 2)
  assert.ok(JSON.stringify(out).length < 1000, '会议本身只剩引用')
  // 下次自动保存：图都存过了，不再写
  await stashAssets(m, store, stored)
  assert.equal(writes(), 2)
  // 原来的会议对象没被改动（界面上还在用）
  assert.equal(m.pages![0].underlay, img(1))
})

test('references are turned back into images on load; a missing image just drops the underlay', async () => {
  const { store, files } = memoryStore()
  const saved = await stashAssets(meeting([img(1), img(2)]), store, new Set())
  const back = await loadAssets(saved, store)
  assert.deepEqual(
    back.pages!.map((p) => p.underlay),
    [img(1), img(2)],
  )
  files.delete([...files.keys()][0])
  const partial = await loadAssets(saved, store)
  assert.equal(partial.pages!.filter((p) => p.underlay).length, 1)
})

test('meetings saved before images were split out still load, and are split on the next save', async () => {
  const { store, files } = memoryStore()
  const old = meeting([img(3)])
  assert.equal(await loadAssets(old, store), old)
  const saved = await stashAssets(old, store, new Set())
  assert.equal(files.size, 1)
  assert.match(saved.pages![0].underlay!, /^asset:/)
})
