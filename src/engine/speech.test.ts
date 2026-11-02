import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appleAssetIssue, appleLanguageChoice, appleLanguageState } from './speech.ts'
import type { SpeechModels } from '../desktop.ts'

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
