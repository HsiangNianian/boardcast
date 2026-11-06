import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { appleAssetIssue, speechMessage } from './speech.ts'
import { en } from '../i18n/en.ts'
import { langChoice, msg, setLangChoice } from '../i18n/index.ts'

test('fixed speech diagnostic keys correspond to the native implementations', () => {
  const source = ['src/voice.rs', 'src/voice_preview.rs', 'src/apple_speech.rs', 'apple-speech/main.swift', 'apple-speech/system-audio.swift']
    .map(path => readFileSync(new URL(`../../src-tauri/${path}`, import.meta.url), 'utf8')).join('\n')
  for (const key of Object.keys(en.speechErrors.fixed)) assert(source.includes(key), key)
})

for (const language of ['zh', 'en'] as const) {
  test(`${language}: speech failures retain recovery meaning and complete unknown diagnostics`, () => {
    const previous = langChoice(), zh = language === 'zh'
    const detail = 'EACCES C:\\Users\\会议 notes\\audio\nSystem audio error: nested cause'
    const fixed = [
      ['No microphone found', '未找到麦克风，请连接设备或选择其他音源后重试。'],
      ['Timed out starting audio input', '启动收音超时，请检查音源后重试。'],
      ['An input check is already running', '输入检测正在运行，请等待完成或先停止检测。'],
      ['Apple did not finish installing the language model', 'Apple 未完成语种模型安装，请在模型管理中重试下载。'],
      ['Apple transcription could not keep up with audio; recording stopped and the last phrase may be incomplete', 'Apple 转写未能跟上收音，录音已停止，最后一句可能不完整。请检查原话后重试。'],
    ]
    const prefixes = [
      ['Microphone could not be opened', '无法打开麦克风'],
      ['System audio error', '系统声音错误'],
      ['Cannot start Apple speech helper', '无法启动 Apple 语音服务'],
      ['Save failed', '语音设置保存失败'],
    ]
    try {
      setLangChoice(language)
      for (const [raw, chinese] of fixed) assert.equal(speechMessage(raw), zh ? chinese : raw)
      for (const [raw, chinese] of prefixes) assert.equal(speechMessage(`${raw}: ${detail}`), (zh ? `${chinese}：` : `${raw}: `) + detail)
      assert.equal(speechMessage('Microphone sample format F64 is not supported'), zh ? '麦克风不支持声音采样格式 F64，请选择其他音源。' : 'Microphone sample format F64 is not supported')
      assert.equal(speechMessage('System audio sample format I8 is not supported'), zh ? '系统声音不支持声音采样格式 I8，请选择其他音源。' : 'System audio sample format I8 is not supported')
      for (const raw of ['', '__proto__', 'constructor: detail', 'No microphone found: custom device detail', 'Microphone could not be opened: ', 'Microphone sample format F64 is not supported; custom detail', 'APPLE_SPEECH_ASSETS_NOT_READY:en-US:custom', 'Unknown vendor error 中文']) assert.equal(speechMessage(raw), raw)
      assert.equal(speechMessage('SYSTEM_AUDIO_PERMISSION'), msg().settings.systemAudioPermission)
      assert.equal(speechMessage('APPLE_AUTO_LANGUAGE_UNCERTAIN'), msg().models.appleAutoUncertain)
      assert.equal(speechMessage('Download the speech recognition model first'), msg().app.needVoiceModel)
    } finally { setLangChoice(previous) }
  })
}

test('existing speech codes follow language changes without changing recovery classification', () => {
  const previous = langChoice(), raw = 'APPLE_SPEECH_ASSETS_NOT_READY:ja-JP:downloading'
  try {
    setLangChoice('zh')
    const chinese = speechMessage(raw)
    assert.equal(chinese, msg().models.speechSetupHint)
    setLangChoice('en')
    assert.equal(speechMessage(raw), msg().models.speechSetupHint)
    assert.notEqual(speechMessage(raw), chinese)
    assert.deepEqual(appleAssetIssue(raw), { locale: 'ja-JP', state: 'downloading' })
    assert.equal(appleAssetIssue('SYSTEM_AUDIO_PERMISSION'), null)
  } finally { setLangChoice(previous) }
})
