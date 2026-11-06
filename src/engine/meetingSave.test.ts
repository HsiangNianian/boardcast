import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Meeting } from '../meetings.ts'
import { emptyGraph } from './graph.ts'
import { MeetingSave, MeetingWrites } from './meetingSave.ts'

const meeting = (id = 'a', updatedAt = 1): Meeting => ({ version: 1, id, title: id, createdAt: 0, updatedAt, graph: emptyGraph(), history: [], lines: [] })
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

test('an older write cannot acknowledge edits made while it was saving', async () => {
  const pending = deferred<void>()
  const save = new MeetingSave(() => pending.promise, () => {})
  save.changed()
  const first = save.save(meeting())
  save.changed()
  pending.resolve()
  await first
  assert.equal(save.status.state, 'dirty')
  await save.save(meeting('a', 2))
  assert.deepEqual(save.status, { state: 'saved', savedAt: 2 })
})

test('flush waits for the latest revision, and failure retains a retryable error', async () => {
  const pending = deferred<void>()
  let attempts = 0
  const saved: number[] = []
  const save = new MeetingSave(async (m) => {
    attempts++
    if (attempts === 1) await pending.promise
    if (attempts === 3) throw new Error('disk full')
    saved.push(m.updatedAt)
  }, () => {})
  let revision = 1
  save.changed()
  const flush = save.flush(() => meeting('a', revision))
  revision++
  save.changed()
  pending.resolve()
  await flush
  assert.deepEqual(saved, [1, 2])
  await assert.rejects(save.flush(() => meeting('a', 3)), /disk full/)
  assert.equal(save.status.state, 'error')
  save.changed()
  assert.equal(save.status.error, 'disk full')
  await save.flush(() => meeting('a', 4))
  assert.equal(save.status.state, 'saved')
})

test('old failure after opening another meeting does not poison its save state', async () => {
  const pending = deferred<void>()
  const save = new MeetingSave(() => pending.promise, () => {})
  save.changed()
  const old = save.save(meeting())
  save.reset(10)
  pending.reject(new Error('old failure'))
  await assert.rejects(old)
  assert.deepEqual(save.status, { state: 'saved', savedAt: 10 })
})

test('writes and deletion are ordered per meeting, and a failed write does not stall recovery', async () => {
  const writes = new MeetingWrites()
  const pending = deferred<void>()
  const events: string[] = []
  const first = writes.run('a', async () => { events.push('write start'); await pending.promise; events.push('write end') })
  const removed = writes.run('a', async () => { events.push('delete') })
  await writes.run('b', async () => { events.push('other meeting') })
  assert.deepEqual(events, ['write start', 'other meeting'])
  pending.resolve()
  await Promise.all([first, removed])
  assert.deepEqual(events, ['write start', 'other meeting', 'write end', 'delete'])
  await assert.rejects(writes.run('a', async () => { throw new Error('write failed') }))
  await writes.run('a', async () => { events.push('retry') })
  assert.equal(events.at(-1), 'retry')
})

test('unchanged missing-assets meetings need no write; actual edits still require saving', async () => {
  let writes = 0
  const save = new MeetingSave(async () => { writes++; throw new Error('missing asset') }, () => {})
  const source: Meeting = { ...meeting(), missingAssets: ['absent.png'], pages: [{ graph: emptyGraph(), underlay: 'asset:absent.png' }, { graph: emptyGraph() }], page: 0 }
  save.reset(1)
  save.remember(source)
  save.changed() // Merely browsing another page may schedule the usual autosave effect.
  assert.equal(save.unchanged({ ...source, updatedAt: 500, page: 1 }), true)
  assert.equal(writes, 0)
  const edited = { ...source, lines: [{ id: 'new-speech', text: 'Do not discard this edit' }] }
  assert.equal(save.unchanged(edited), false)
  await assert.rejects(save.save(edited), /missing asset/)
  assert.equal(writes, 1)
  assert.equal(save.status.state, 'error')
})

test('pending free-canvas gestures cannot be mistaken for an unchanged saved meeting', () => {
  const save = new MeetingSave(async () => {}, () => {})
  const free = { id: 'free', type: 'rectangle', x: 10, y: 20, width: 80, height: 40, isDeleted: false } as NonNullable<Meeting['scene']>['elements'][number]
  const source: Meeting = { ...meeting(), scene: { elements: [free], generated: [], files: {} } }
  save.reset(1); save.remember(source)
  assert.equal(save.unchanged({ ...source, scene: { ...source.scene!, elements: [{ ...free, version: 30, updated: 123 }] } }), true)
  assert.equal(save.unchanged({ ...source, scene: { ...source.scene!, elements: [{ ...free, x: 160 }] } }), false)
})

test('closing a rich missing-assets meeting does not mistake transcript context metadata for an edit', () => {
  const save = new MeetingSave(async () => { throw new Error('must not rewrite a readonly source') }, () => {})
  const utterance = { id: 'old-line', text: 'Original content', speaker: 'Alice', pointing: ['n1'] }
  const source: Meeting = { ...meeting(), missingAssets: ['absent.png'], history: [utterance],
    lines: [{ ...utterance, page: 1, ms: 42, ops: 'N original', via: 'local' }],
    pages: [{ graph: emptyGraph() }, { graph: emptyGraph(), underlay: 'asset:absent.png' }], page: 1 }
  save.reset(1); save.remember(source)
  // cancelMeetingWork retains rows and rebuilds the Session context from them.
  const checkpoint = { ...source, history: source.lines }
  assert.equal(save.unchanged(checkpoint), true)
  assert.equal(save.unchanged({ ...checkpoint, missingAssets: undefined, assetIssues: [{ key: 'absent.png', reason: 'decode' }] }), true, 'resource diagnostics are not content edits')
  assert.equal(save.unchanged({ ...checkpoint, history: [{ ...utterance, text: 'Changed context' }] }), false)
  assert.equal(save.unchanged({ ...checkpoint, lines: [{ ...source.lines[0], page: 0 }] }), false)
  assert.equal(save.unchanged({ ...checkpoint, lines: [...source.lines, { text: 'A real new line', ms: 0 }] }), false)
})

test('an older in-flight write prevents skipping a restored snapshot', async () => {
  const pending = deferred<void>()
  const save = new MeetingSave(() => pending.promise, () => {})
  const original = meeting()
  save.reset(1); save.remember(original)
  const writing = save.save({ ...original, title: 'Temporary edit' })
  assert.equal(save.unchanged(original), false, 'the old write could otherwise replace the restored content after leaving')
  pending.resolve(); await writing
  assert.equal(save.unchanged(original), false)
})
