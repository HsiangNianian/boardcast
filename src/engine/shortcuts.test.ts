import assert from 'node:assert/strict'
import { test } from 'node:test'
import { accelFrom, actionFor, shortcutLabel } from './shortcuts.ts'

const key = (key: string, m: Partial<{ ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) => ({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...m })

test('app shortcuts map to their actions', () => {
  assert.equal(actionFor(key('PageDown')), 'nextPage')
  assert.equal(actionFor(key('[', { ctrlKey: true })), 'prevPage')
  assert.equal(actionFor(key('N', { ctrlKey: true, shiftKey: true })), 'newPage')
  assert.equal(actionFor(key('o', { metaKey: true })), 'import')
  assert.equal(actionFor(key('R', { ctrlKey: true, shiftKey: true })), 'replay')
  assert.equal(actionFor(key('S', { ctrlKey: true, shiftKey: true })), 'summary')
  assert.equal(actionFor(key('?', { shiftKey: true })), 'help')
})

test('other keys are left alone', () => {
  assert.equal(actionFor(key('n')), null)
  assert.equal(actionFor(key('s', { ctrlKey: true })), null)
  assert.equal(actionFor(key('o', { ctrlKey: true, altKey: true })), null)
})

test('shortcut labels follow the platform and native redo command', () => {
  assert.equal(shortcutLabel('Ctrl+Shift+R', 'MacIntel'), '⇧⌘R')
  assert.equal(shortcutLabel('Ctrl+Z / Ctrl+Y', 'MacIntel'), '⌘Z / ⇧⌘Z')
  assert.equal(shortcutLabel('Ctrl+[ / Ctrl+]', 'iPad'), '⌘[ / ⌘]')
  for (const platform of ['Linux x86_64', 'Win32', '']) {
    assert.equal(shortcutLabel('Ctrl+Z / Ctrl+Y', platform), 'Ctrl+Z / Ctrl+Y')
  }
  for (const keys of ['Hold Space', '按住空格', 'PageUp / PageDown', '?']) {
    assert.equal(shortcutLabel(keys, 'MacIntel'), keys)
  }
})

test('a pressed combination becomes an accelerator', () => {
  assert.equal(accelFrom({ ...key(' ', { ctrlKey: true, shiftKey: true }), code: 'Space' }), 'Ctrl+Shift+Space')
  assert.equal(accelFrom({ ...key('m', { altKey: true }), code: 'KeyM' }), 'Alt+M')
  assert.equal(accelFrom({ ...key('Control', { ctrlKey: true }), code: 'ControlLeft' }), null)
  assert.equal(accelFrom({ ...key('m'), code: 'KeyM' }), null)
})
