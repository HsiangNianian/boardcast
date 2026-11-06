import assert from 'node:assert/strict'
import { test } from 'node:test'
import { toolbarPosition } from '../ink/toolbar.ts'

test('long translated toolbar stays within every canvas edge and switches below a top selection', () => {
  for (const width of [320, 720, 920]) {
    const height = 300
    for (const x of [-100, 0, width / 2, width, width + 100]) {
      for (const y of [-50, 0, 150, height, height + 100]) {
        const size = { width: Math.min(490, width - 16), height: width < 490 ? 80 : 36 }
        const placed = toolbarPosition({ x, top: y, bottom: y + 40 }, size, { width, height })
        assert.ok(placed.left >= 8 && placed.left + size.width <= width - 8)
        assert.ok(placed.top >= 8 && placed.top + size.height <= height - 8)
      }
    }
  }
  assert.equal(toolbarPosition({ x: 400, top: 10, bottom: 50 }, { width: 490, height: 36 }, { width: 920, height: 600 }).top, 58)
})
