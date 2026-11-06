import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type AssetStore, type Meeting, loadAssets as load, stashAssets as stash, sourceHash, meetings, nextStorageRevision } from '../meetings.ts'
import { emptyGraph } from './graph.ts'
import { assetKey, verifyAsset } from './assetIntegrity.ts'

// These storage tests use synthetic payloads and an explicit decoder. Actual PNG/SVG
// decoding and the timeout are exercised by check-asset-integrity.cjs in a browser.
const decode = async () => {}
const loadAssets = (m: Meeting, store: AssetStore, stored?: Set<string>) => load(m, store, stored, decode)
const stashAssets = (m: Meeting, store: AssetStore, stored: Set<string>) => stash(m, store, stored, decode)

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

test('references are turned back into images on load; a missing image retains its recoverable reference', async () => {
  const { store, files } = memoryStore()
  const saved = await stashAssets(meeting([img(1), img(2)]), store, new Set())
  const back = await loadAssets(saved, store)
  assert.deepEqual(
    back.pages!.map((p) => p.underlay),
    [img(1), img(2)],
  )
  files.delete(`${saved.id}/${saved.pages![0].underlay!.slice('asset:'.length)}`)
  const partial = await loadAssets(saved, store)
  assert.equal(partial.pages![0].underlay, saved.pages![0].underlay)
  assert.equal(partial.missingAssets?.length, 1)
  await assert.rejects(stashAssets(partial, store, new Set()), /missing/)
})

test('meetings saved before images were split out still load, and are split on the next save', async () => {
  const { store, files } = memoryStore()
  const old = meeting([img(3)])
  assert.equal(await loadAssets(old, store), old)
  const saved = await stashAssets(old, store, new Set())
  assert.equal(files.size, 1)
  assert.match(saved.pages![0].underlay!, /^asset:/)
})

test('restoring a missing resource allows retrying the same loaded meeting and clears stale diagnostics', async () => {
  const { store, files } = memoryStore()
  const saved = await stashAssets(meeting([img(1)]), store, new Set())
  const [key, data] = [...files][0]
  files.delete(key)
  const partial = await loadAssets(saved, store)
  const before = JSON.stringify(partial)
  await assert.rejects(stashAssets(partial, store, new Set()), /missing/)
  files.set(key, data)
  const retried = await stashAssets(partial, store, new Set())
  assert.equal(retried.missingAssets, undefined)
  assert.equal(retried.pages![0].underlay, saved.pages![0].underlay)
  assert.equal(JSON.stringify(partial), before, 'retry does not mutate the displayed meeting')
  const reopened = await loadAssets({ ...retried, missingAssets: partial.missingAssets }, store)
  assert.equal(reopened.missingAssets, undefined)
  assert.equal(reopened.pages![0].underlay, img(1))
})

test('an image removed after loading is restored from the retained in-memory bytes before saving', async () => {
  const { store, files } = memoryStore()
  const stored = new Set<string>()
  const saved = await stashAssets(meeting([img(1)]), store, stored)
  const loaded = await loadAssets(saved, store, stored)
  files.clear()
  await stashAssets(loaded, store, stored)
  assert.equal(files.size, 1)
  assert.equal((await loadAssets(saved, store)).pages![0].underlay, img(1))
})


test('scene images and historical projections migrate to the new owner without touching originals', async () => {
  const legacy = memoryStore()
  const current = memoryStore()
  const m = meeting([img(1)])
  const scene = { elements: [], generated: [], files: { image1: { id: 'image1', mimeType: 'image/png', dataURL: 'data:image/png;base64,YWJj', created: 1 } }, overlay: { url: 'data:image/svg+xml;base64,PHN2Zy8+', x: 1, y: 2, width: 3, height: 4, hidden: [] } }
  m.scene = scene as unknown as NonNullable<Meeting['scene']>
  m.pages![0].scene = m.scene
  m.pages![0].steps = [{ t: 1, line: 0, d: {}, scene: m.scene }]
  const original = await stashAssets(m, legacy.store, new Set())
  const originalBytes = JSON.stringify(original)
  const restored = await loadAssets(JSON.parse(originalBytes), legacy.store)
  const migrated = await stashAssets({ ...restored, version: 2 }, current.store, new Set())
  assert.equal(JSON.stringify(original), originalBytes)
  assert.equal(current.files.size, 3)
  assert.equal(legacy.files.size, 3)
  assert.deepEqual((await loadAssets(migrated, current.store)).scene, m.scene)
  assert.equal(current.writes(), 3, 'shared page, scene and historical images are written once')
  assert.notEqual(await sourceHash(original), await sourceHash({ ...original, updatedAt: 5 }))
  await assert.rejects(meetings.save({ ...m, version: 99 }), /newer/)
})


test('storage compare-and-swap rejects stale windows and future formats', () => {
  const m = { ...meeting([]), version: 2, storageRevision: 3 }
  assert.equal(nextStorageRevision(m, 3), 4)
  assert.throws(() => nextStorageRevision(m, 2), /another window/)
  assert.throws(() => nextStorageRevision({ ...m, version: 99 }, 3), /newer/)
  assert.equal(nextStorageRevision(null, 0), 1)
})

test('a valid image under another content hash retains its reference and cannot poison later keys', async () => {
  const { store, files, writes } = memoryStore()
  const saved = await stashAssets(meeting([img(10)]), store, new Set())
  const [path] = files.keys()
  files.set(path, img(11))
  const original = JSON.stringify(saved), beforeWrites = writes(), known = new Set<string>()
  const loaded = await loadAssets(saved, store, known)
  assert.equal(loaded.pages![0].underlay, saved.pages![0].underlay)
  assert.deepEqual(loaded.assetIssues, [{ key: path.slice(3), reason: 'hash-mismatch' }])
  assert.equal(known.size, 0)
  await assert.rejects(stashAssets(loaded, store, known), /content hash/)
  assert.equal(writes(), beforeWrites)
  assert.equal(files.get(path), img(11), 'bad original is never silently replaced')
  assert.equal(JSON.stringify(saved), original)
  const other = memoryStore()
  const newMeeting = await stashAssets(meeting([img(11)]), other.store, new Set())
  assert.equal(newMeeting.pages![0].underlay, `asset:${await assetKey(img(11))}`, 'a rejected filename cannot authenticate a new image')
})

test('correct hashes do not bypass image decoding, including historical-only resources', async () => {
  const { store, files } = memoryStore()
  const bad = 'data:image/png;base64,bm90IGEgUE5H'
  const key = (await assetKey(bad))!
  files.set(`m1/${key}`, bad)
  const scene = { elements: [], generated: [], files: { f: { id: 'f', mimeType: 'image/png', dataURL: `asset:${key}`, created: 1 } } } as unknown as NonNullable<Meeting['scene']>
  const m = { ...meeting([]), steps: [{ t: 1, line: 0, d: {}, scene }] }
  const before = JSON.stringify(m)
  let decoded = 0
  const invalid = async () => { decoded++; throw new Error('decoder rejected truncated image') }
  const loaded = await load(m, store, undefined, invalid)
  assert.equal(decoded, 1)
  assert.deepEqual(loaded.assetIssues, [{ key, reason: 'decode' }])
  assert.equal(loaded.steps![0].scene!.files.f.dataURL, `asset:${key}`)
  await assert.rejects(stash(loaded, store, new Set(), invalid), /damaged/)
  assert.equal(JSON.stringify(m), before)
  assert.equal(files.get(`m1/${key}`), bad)
})

test('restoring corrupt content allows the same retained reference to retry and clears diagnostics', async () => {
  const { store, files } = memoryStore()
  const saved = await stashAssets(meeting([img(20)]), store, new Set())
  const [path, original] = [...files][0]
  files.set(path, img(21))
  const loaded = await loadAssets(saved, store)
  await assert.rejects(stashAssets(loaded, store, new Set()), /content hash/)
  files.set(path, original)
  const retried = await stashAssets(loaded, store, new Set())
  assert.equal(retried.assetIssues, undefined)
  assert.equal(retried.missingAssets, undefined)
  assert.equal(retried.pages![0].underlay, saved.pages![0].underlay)
  const reopened = await loadAssets({ ...retried, assetIssues: loaded.assetIssues }, store)
  assert.equal(reopened.assetIssues, undefined)
  assert.equal(reopened.pages![0].underlay, original)
})

test('post-load disk corruption blocks saving even when valid bytes remain in memory', async () => {
  const { store, files, writes } = memoryStore(), known = new Set<string>()
  const saved = await stashAssets(meeting([img(30)]), store, known)
  const loaded = await loadAssets(saved, store, known)
  const [path] = files.keys()
  files.set(path, img(31))
  const before = writes()
  await assert.rejects(stashAssets(loaded, store, known), /content hash/)
  assert.equal(writes(), before)
  assert.equal(files.get(path), img(31))
  assert.equal(loaded.pages![0].underlay, img(30))
})

test('native hash rejections and IO errors remain distinguishable without losing the reference', async () => {
  const key = (await assetKey(img(40)))!
  for (const [error, reason] of [['ASSET_HASH_MISMATCH: bad file', 'hash-mismatch'], ['Permission denied', 'unreadable']] as const) {
    const store: AssetStore = { put: async () => assert.fail('must not replace the original'), get: async () => { throw error } }
    const m = meeting([`asset:${key}`])
    const loaded = await loadAssets(m, store)
    assert.deepEqual(loaded.assetIssues, [{ key, reason }])
    assert.equal(loaded.pages![0].underlay, `asset:${key}`)
    await assert.rejects(stashAssets(loaded, store, new Set()), reason === 'hash-mismatch' ? /content hash/ : /could not be read/)
  }
})

test('hash verification uses the v1 data URL algorithm and runs before the decoder', async () => {
  const data = 'data:image/png;base64,bm90IGEgUE5H'
  assert.equal(await assetKey(data), '4c353aab651f847ff789986e238a6f2f.png')
  let decodes = 0
  await assert.rejects(verifyAsset(data, '0123456789abcdef0123456789abcdef.png', async () => { decodes++ }), /content hash/)
  assert.equal(decodes, 0)
})
