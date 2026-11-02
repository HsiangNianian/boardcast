import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appleAssetIssue } from './speech.ts'

test('Apple asset failures lead to setup and preserve the selected language and actual state', () => {
  for (const state of ['supported', 'downloading', 'unsupported', 'unknown']) {
    assert.deepEqual(appleAssetIssue(`APPLE_SPEECH_ASSETS_NOT_READY:zh-CN:${state}`), { locale: 'zh-CN', state })
  }
  for (const message of ['Microphone access denied', 'Apple speech helper exited unexpectedly',
    'APPLE_SPEECH_ASSETS_NOT_READY::supported', 'APPLE_SPEECH_ASSETS_NOT_READY:en-US:installed']) {
    assert.equal(appleAssetIssue(message), null)
  }
})
