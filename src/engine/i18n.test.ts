import assert from 'node:assert/strict'
import { test } from 'node:test'
import { en } from '../i18n/en.ts'
import { systemLang } from '../i18n/index.ts'
import { zh } from '../i18n/zh.ts'

test('system language picks the first one we have, English otherwise', () => {
  assert.equal(systemLang(['zh-CN', 'en-US']), 'zh')
  assert.equal(systemLang(['zh-Hant-TW']), 'zh')
  assert.equal(systemLang(['yue-HK']), 'zh')
  assert.equal(systemLang(['en-GB', 'zh-CN']), 'en')
  assert.equal(systemLang(['ja-JP', 'zh-CN']), 'zh')
  assert.equal(systemLang(['ja-JP', 'de-DE']), 'en')
  assert.equal(systemLang([]), 'en')
})

/** Every key path in a dictionary, with the kind of value it holds */
function shape(o: object, at = ''): string[] {
  return Object.entries(o).flatMap(([k, v]) => (v && typeof v === 'object' ? shape(v, `${at}${k}.`) : [`${at}${k}:${typeof v}`]))
}

test('every dictionary has exactly the keys of the English one', () => {
  assert.deepEqual(shape(zh).sort(), shape(en).sort())
})

test('English plurals', () => {
  assert.equal(en.app.count(1), '1 line')
  assert.equal(en.app.count(3), '3 lines')
  assert.equal(en.app.excalidrawNote('a.excalidraw', { nodes: 1, edges: 2, groups: 0, notes: 0 }), 'Imported a.excalidraw (1 box, 2 lines)')
})
