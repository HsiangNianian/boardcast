import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appleAssetIssue, appleLanguageChoice, appleLanguageState, startVoice } from './speech.ts'
import type { SpeechModels, VoiceSource } from '../desktop.ts'

function deferred() {
  let resolve!: () => void
  const promise = new Promise<void>((done) => { resolve = done })
  return { promise, resolve }
}

test('starting from idle preserves each selected source and honors cancellation before opening it', async () => {
  for (const source of ['mic', 'system', 'both'] as const) {
    const calls: (VoiceSource | 'stop')[] = []
    const input = {
      start: async (_device: string | undefined, selected?: VoiceSource) => { calls.push(selected!) },
      stop: async () => { calls.push('stop') },
    }
    await startVoice(input, { stop: true }, undefined, source)
    assert.deepEqual(calls, [])
    await startVoice(input, { stop: false }, undefined, source)
    assert.deepEqual(calls, [source])
  }
})

test('changing a live input waits for the previous capture to stop before opening both sources', async () => {
  const calls: string[] = []
  const stopped = deferred()
  const input = {
    stop: async () => { calls.push('stop'); await stopped.promise },
    start: async (device: string | undefined, source?: VoiceSource) => { calls.push(`start:${device}:${source}`) },
  }
  const switching = startVoice(input, { stop: false }, 'USB microphone', 'both', true)
  assert.deepEqual(calls, ['stop'])
  stopped.resolve()
  await switching
  assert.deepEqual(calls, ['stop', 'start:USB microphone:both'])
})

test('a stop during a source switch or a permission prompt does not leave recording running', async () => {
  for (const during of ['stop', 'start'] as const) {
    const calls: string[] = []
    const pending = deferred()
    const entered = deferred()
    const request = { stop: false }
    const input = {
      stop: async () => { calls.push('stop'); if (during === 'stop') { entered.resolve(); await pending.promise } },
      start: async () => { calls.push('start'); if (during === 'start') { entered.resolve(); await pending.promise } },
    }
    const switching = startVoice(input, request, undefined, 'both', true)
    await entered.promise
    request.stop = true
    pending.resolve()
    await switching
    assert.deepEqual(calls, during === 'stop' ? ['stop'] : ['stop', 'start', 'stop'])
  }
})

test('source switching surfaces failures without silently falling back to one input', async () => {
  for (const failure of ['stop', 'start'] as const) {
    const calls: string[] = []
    const input = {
      stop: async () => { calls.push('stop'); if (failure === 'stop') throw new Error('stop failed') },
      start: async () => { calls.push('start:both'); if (failure === 'start') throw new Error('permission denied') },
    }
    await assert.rejects(startVoice(input, { stop: false }, undefined, 'both', true),
      { message: failure === 'stop' ? 'stop failed' : 'permission denied' })
    assert.deepEqual(calls, failure === 'stop' ? ['stop'] : ['stop', 'start:both'])
  }
})

test('Apple asset failures lead to setup and preserve the selected language and actual state', () => {
  for (const state of ['supported', 'downloading', 'unsupported', 'unknown']) {
    assert.deepEqual(appleAssetIssue(`APPLE_SPEECH_ASSETS_NOT_READY:zh-CN:${state}`), { locale: 'zh-CN', state })
  }
  for (const message of ['Microphone access denied', 'Apple speech helper exited unexpectedly',
    'APPLE_SPEECH_ASSETS_NOT_READY::supported', 'APPLE_SPEECH_ASSETS_NOT_READY:en-US:installed']) {
    assert.equal(appleAssetIssue(message), null)
  }
})

const view = (): SpeechModels => ({
  selection: { provider: 'sensevoice', locale: '' }, senseVoiceInstalled: true, listening: false, preparing: false,
  autoLocales: ['zh_CN', 'ja_JP', 'en_US'], activeLocale: null, appleError: null,
  apple: { isApple: true, available: true, reason: null, defaultLocale: 'zh_CN', installedLocales: [], osVersion: '26',
    locales: ['zh_CN', 'ja_JP', 'en_US'].map((id) => ({ id, name: id, status: 'installed' })) },
})

test('Apple Auto is the default language choice while explicit saved/manual choices persist', () => {
  const v = view()
  assert.equal(appleLanguageChoice(v, ''), 'auto')
  v.selection = { provider: 'apple', locale: 'ja_JP' }
  assert.equal(appleLanguageChoice(v, ''), 'ja_JP')
  assert.equal(appleLanguageChoice(v, 'auto'), 'auto')
  assert.equal(appleLanguageChoice(v, 'en_US'), 'en_US')
  v.selection.locale = 'removed-locale'
  assert.equal(appleLanguageChoice(v, ''), 'auto')
})

test('Apple Auto checks all three module states instead of the installed-locales inventory', () => {
  const v = view()
  assert.equal(appleLanguageState(v, 'auto'), 'installed')
  for (const state of ['supported', 'downloading', 'unknown', 'unsupported'] as const) {
    v.apple!.locales[1].status = state
    assert.equal(appleLanguageState(v, 'auto'), state)
    assert.equal(appleLanguageState(v, 'en_US'), 'installed')
  }
  v.apple!.locales[1].status = 'installed'
  v.autoLocales.pop()
  assert.equal(appleLanguageState(v, 'auto'), 'unsupported')
  const missing = view()
  missing.senseVoiceInstalled = false
  assert.equal(appleLanguageState(missing, 'auto'), 'unsupported')
  assert.deepEqual(appleAssetIssue('APPLE_SPEECH_ASSETS_NOT_READY:auto:supported'), { locale: 'auto', state: 'supported' })
})
