import assert from 'node:assert/strict'
import { test } from 'node:test'
import { info, normalize, type Meeting } from '../meetings.ts'
import { emptyGraph } from './graph.ts'

test('library counts omit manual edits and interrupted speech remains retryable on reopen', () => {
  const graph = emptyGraph()
  const meeting: Meeting = { version: 2, id: 'counts', title: 'Meeting', createdAt: 1, updatedAt: 2,
    graph, history: [], pages: [{ graph }, { graph }], lines: [
      { text: 'Pending', id: 'p' }, { text: 'Failed', ms: 0, failed: true }, { text: 'Done', ms: 8 }, { text: 'Edit', ms: 0, manual: true },
    ] }
  assert.deepEqual([info(meeting).lines, info(meeting).pages, info(meeting).unfinished], [3, 2, 2])
  const reopened = normalize(meeting)
  assert.equal(reopened.lines[0].id, 'p')
  assert.equal(reopened.lines[0].failed, true)
  assert.equal(reopened.lines[0].boardFailure, 'cancelled')
  assert.equal(info(reopened).unfinished, 2)
  assert.equal(meeting.lines[0].ms, undefined)
})
