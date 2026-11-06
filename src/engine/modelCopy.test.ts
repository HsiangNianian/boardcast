import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { modelCopy, modelMessage } from '../desktop.ts'
import { en } from '../i18n/en.ts'
import { langChoice, setLangChoice } from '../i18n/index.ts'

test('translated model metadata covers the actual native catalog without changing English descriptions', () => {
  const native = readFileSync(new URL('../../src-tauri/src/models.rs', import.meta.url), 'utf8')
  const entries = [...native.matchAll(/id: "([^"]+)",\s+label: "([^"]+)",\s+note: "([^"]+)"/g)]
  assert.deepEqual(Object.fromEntries(entries.map(([, id, label, note]) => [id, { label, note }])), en.models.catalog)
})

test('catalog text follows language without rewriting model data; unknown ids retain their descriptions', () => {
  const previous = langChoice()
  const voice = { id: 'voice', label: 'Speech recognition model', note: 'Original native details' }
  try {
    setLangChoice('zh')
    assert.equal(modelCopy(voice).label, '语音识别模型')
    setLangChoice('en')
    assert.equal(modelCopy(voice).label, voice.label)
    assert.equal(voice.note, 'Original native details')
    for (const id of ['future-model', '__proto__', 'constructor', 'toString']) {
      const unknown = { id, label: 'Private model 名称', note: 'Retain custom details' }
      assert.equal(modelCopy(unknown), unknown)
    }
  } finally { setLangChoice(previous) }
})

for (const language of ['zh', 'en'] as const) {
  test(`${language}: download messages preserve file, hash, capacity and unknown diagnostic details`, () => {
    const previous = langChoice(), hash = 'a1'.repeat(32)
    const detail = 'EACCES C:\\Users\\会议 (old)\\model.part\nWrite failed: nested detail'
    const cases: [string, string][] = [
      ['App data folder not found', '找不到应用数据目录，请重新启动应用后重试。'],
      ['Download in progress; cancel it first', '模型正在下载，请先暂停下载再删除。'],
      ['Verifying the downloaded portion…', '正在校验已下载的部分…'],
      ['Verifying file…', '正在校验文件…'],
      ['All download sources failed', '所有下载源均失败，请检查网络后重试。'],
      ['Model not found: custom-model', '找不到模型：custom-model'],
      ["Not enough disk space: Speech recognition model needs 0.2 GB more (plus 0.3 GB spare); only 0.0 GB is free on the model folder's disk", '磁盘空间不足：语音识别模型还需 0.2 GB，另需保留 0.3 GB 空间；模型目录所在磁盘仅剩 0.0 GB。'],
      ['Speech recognition model is downloading; wait for it to finish or cancel it first', '语音识别模型正在下载，请等待完成或先暂停该下载。'],
      ["Vision add-on · Qwen3.5-4B can't be selected as the built-in model", '视觉组件 · Qwen3.5-4B不能作为内置板书模型使用。'],
      ['Qwen3.5-9B has not been downloaded yet', 'Qwen3.5-9B尚未下载，请先下载后再使用。'],
      [`Couldn't check free disk space: ${detail}`, `无法检查磁盘剩余空间：${detail}`],
      [`Couldn't create the folder: ${detail}`, `无法创建模型目录：${detail}`],
      [`Couldn't write to the model folder: ${detail}`, `无法写入模型目录：${detail}`],
      [`Write failed: ${detail}`, `模型文件写入失败：${detail}`],
      [`Verification failed (sha256 ${hash}); file deleted. Please download it again`, `文件校验失败（sha256 ${hash}），已删除该文件，请重新下载。`],
    ]
    try {
      setLangChoice(language)
      for (const [raw, chinese] of cases) assert.equal(modelMessage(raw), language === 'zh' ? chinese : raw)
      assert.equal(modelMessage('voice is downloading; wait for it to finish or cancel it first'), language === 'zh'
        ? '语音识别模型正在下载，请等待完成或先暂停该下载。' : 'Speech recognition model is downloading; wait for it to finish or cancel it first')
      for (const raw of ['', 'constructor', 'ModelScope: HTTP 403 Forbidden', 'Write failed: ', 'All download sources failed: TLS detail', `Verification failed (sha256 ${hash.slice(1)}); file deleted. Please download it again`]) assert.equal(modelMessage(raw), raw)
    } finally { setLangChoice(previous) }
  })
}
