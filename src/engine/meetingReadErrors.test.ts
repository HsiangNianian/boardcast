import assert from 'node:assert/strict'
import { test } from 'node:test'
import '../i18n/index.ts'
import type { Meeting } from '../meetings.ts'
import { emptyGraph } from './graph.ts'

test('library preserves both versions of read failures, searches healthy records and recovers an older copy safely', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window')
  const record = (id: string, title: string): Meeting => ({ version: 2, id, title, createdAt: 1, updatedAt: 2, graph: emptyGraph(), history: [], lines: [] })
  const older = { ...record('recoverable', 'Older needle'), version: 1 as const, storageRevision: 4 }
  const changed = { ...record('changed', 'Changed older copy'), version: 1 as const }
  const healthy = record('current-with-bad-old', 'Healthy needle')
  const migrated = { ...record('changed', 'Migrated record'), migration: { sourceHash: 'previous-content' } }
  const records: Record<number, Record<string, string>> = {
    1: { recoverable: JSON.stringify(older), changed: JSON.stringify(changed), 'legacy-bad': '{truncated', 'current-with-bad-old': '{}' },
    2: { 'current-with-bad-old': JSON.stringify(healthy), changed: JSON.stringify(migrated), recoverable: '{truncated', 'native-bad': '{truncated' },
  }
  const original = structuredClone(records)
  const reads: string[] = []
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: async (command: string, args: Record<string, unknown>) => {
      const version = args.storageVersion as number
      const id = args.id as string
      if (command === 'meetings_list') {
        if (args.strict) throw new Error('Could not read meeting: bad.json')
        return Object.entries(records[version]).map(([id, text]) => {
          const saved = text.startsWith('{"version"') ? JSON.parse(text) as Meeting : null
          return { id, title: saved?.title ?? id, createdAt: 1, updatedAt: 2, lines: 0, ...(saved ? {} : { readError: `Could not read v${version}/${id}.json` }) }
        })
      }
      if (command === 'meeting_load') { reads.push(`${version}/${id}`); return records[version][id] ?? null }
      if (command === 'meeting_save') {
        assert.equal(version, 2)
        assert.equal(args.expectedRevision, 0)
        assert.equal(records[version][id], undefined, 'recovery uses a new file')
        records[version][id] = args.json as string
        return
      }
      assert.fail(`Unexpected command: ${command}`)
    } },
  } })
  try {
    const { meetings } = await import('../meetings.ts')
    const rows = await meetings.list()
    assert.equal(rows.filter(row => row.readError).length, 4, 'a healthy version never hides another version’s error')
    assert.equal(new Set(rows.map(row => row.id)).size, rows.length)
    assert.deepEqual(rows.filter(row => !row.readError).map(row => row.id).sort(), ['changed', 'current-with-bad-old', 'legacy:changed', 'legacy:recoverable'])
    assert.ok(rows.find(row => row.id === 'legacy:recoverable')?.title.includes('older version copy'))
    assert.deepEqual((await meetings.search('needle')).map(result => result.info.id).sort(), ['current-with-bad-old', 'legacy:recoverable'])
    assert.ok(!reads.includes('2/recoverable') && !reads.includes('1/current-with-bad-old'), 'unreadable files are not opened during listing or search')
    await assert.rejects(meetings.corrections(), /bad.json/, 'sample exports still fail rather than omit unreadable records')
    await assert.rejects(meetings.load('recoverable'), SyntaxError, 'normal loading never silently overwrites a damaged newer file')
    const recovered = await meetings.load('legacy:recoverable')
    assert.ok(recovered)
    assert.notEqual(recovered.id, older.id)
    assert.equal(recovered.migration, undefined)
    assert.equal(recovered.storageRevision, undefined)
    assert.equal(recovered.title, `${older.title} (older version copy)`)
    await meetings.save(recovered)
    assert.equal(JSON.parse(records[2][recovered.id]).storageRevision, 1)
    for (const version of [1, 2]) for (const [id, text] of Object.entries(original[version])) assert.equal(records[version][id], text)
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous)
    else Reflect.deleteProperty(globalThis, 'window')
  }
})
