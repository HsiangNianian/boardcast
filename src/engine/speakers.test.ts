import assert from 'node:assert/strict'
import { test } from 'node:test'
import { initial, speakerSlot } from '../ui/speakers.ts'

test('an avatar shows one recognisable character of the name', () => {
  assert.equal(initial('小王'), '王')
  assert.equal(initial('老张'), '张')
  assert.equal(initial('李工'), '李')
  assert.equal(initial('欧阳娜娜'), '欧')
  assert.equal(initial('说话人 2'), '2')
  assert.equal(initial('Speaker 12'), '12')
  assert.equal(initial('alice'), 'A')
  assert.equal(initial('B'), 'B')
  assert.equal(initial('  '), '?')
})

test('speaker colours follow the order speakers appear in, wrapping after six', () => {
  const s = ['A', 'B', 'C', '小王', '小李', '老张', '王总']
  assert.equal(speakerSlot(s, '小王'), 3)
  assert.equal(speakerSlot(s, '王总'), 0)
  assert.equal(speakerSlot(s, '没出现过'), 0)
})
