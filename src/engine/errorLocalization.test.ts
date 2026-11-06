import assert from 'node:assert/strict'
import { test } from 'node:test'
import { adoptSceneOwner, type FreeScene, validateScene } from '../board/freeScene.ts'
import { importExcalidraw } from '../board/importExcalidraw.ts'
import { langChoice, setLangChoice } from '../i18n/index.ts'
import { type Meeting, meetings, nextStorageRevision } from '../meetings.ts'
import { emptyGraph } from './graph.ts'
import { applyOps, parseOps } from './ops.ts'
import { AssetIntegrityError, assetReadIssue } from './assetIntegrity.ts'

for (const lang of ['zh', 'en'] as const) {
  test(`${lang}: image error text preserves typed recovery reasons and native hash signals`, () => {
    const previous = langChoice()
    setLangChoice(lang)
    try {
      const key = '0123456789abcdef0123456789abcdef.png'
      for (const [reason, en, zh] of [
        ['missing', 'is missing', '缺失'], ['unreadable', 'could not be read', '无法读取'],
        ['hash-mismatch', 'does not match its content hash', '与保存的校验值不符'],
        ['decode', 'is damaged or could not be decoded', '已损坏或无法解码'],
      ] as const) {
        const issue = { key, reason }, error = new AssetIntegrityError(issue)
        assert.equal(error.message, lang === 'zh'
          ? `会议图片 ${key}${zh}。现有文件和引用均已保留，请恢复原图片后再保存。`
          : `Meeting image ${key} ${en}. Existing files and references are preserved. Restore the original image before saving.`)
        assert.deepEqual(assetReadIssue(key, error), issue)
      }
      assert.deepEqual(assetReadIssue(key, `ASSET_HASH_MISMATCH: ${key}. The original image is preserved.`), { key, reason: 'hash-mismatch' })
      assert.deepEqual(assetReadIssue(key, new Error('Permission denied')), { key, reason: 'unreadable' })
    } finally { setLangChoice(previous) }
  })

  test(`${lang}: localized rejection preserves storage revisions and invalid drawing data`, async () => {
    const previous = langChoice()
    setLangChoice(lang)
    const zh = lang === 'zh'
    try {
      const graph = emptyGraph()
      const meeting: Meeting = { version: 2, storageRevision: 3, id: 'localization', title: 'User title 中文', createdAt: 1, updatedAt: 1, graph, history: [], lines: [] }
      const original = structuredClone(meeting)
      assert.throws(() => nextStorageRevision(meeting, 2), { message: zh
        ? '这场会议已在其他窗口更改，无法覆盖保存。当前修改仍在这里，可先导出副本，再重新打开会议。'
        : 'This meeting changed in another window. Reopen it before saving; your current edits are still here.' })
      const newer = zh ? '无法覆盖由新版 Boardcast 保存的会议。' : 'Cannot overwrite a meeting from a newer Boardcast version.'
      assert.throws(() => nextStorageRevision({ ...meeting, version: 99 }, 3), { message: newer })
      await assert.rejects(meetings.save({ ...meeting, version: 99 }), { message: newer })
      assert.equal(nextStorageRevision(meeting, 3), 4)
      assert.deepEqual(meeting, original)

      const drawings = [
        { meta: { version: 99, graph }, message: zh ? '这份绘图需要新版 Boardcast，请更新后再导入。' : 'This drawing needs a newer Boardcast version' },
        { meta: { version: 2, graph: { nodes: [] } }, message: zh ? '绘图中的 Boardcast 结构数据无效。' : 'Invalid Boardcast graph' },
        { meta: { version: 2, graph: { ...graph, underlay: { w: 10, h: 10 } }, underlay: 'https://example.invalid/tracker.png' }, message: zh ? '绘图中的 Boardcast 背景图片无效。' : 'Invalid Boardcast background image' },
      ]
      for (const { meta, message } of drawings) {
        const source = { elements: [], files: {}, boardcast: { generated: [], ...meta } }
        const before = structuredClone(source)
        assert.throws(() => importExcalidraw(JSON.stringify(source)), { message })
        assert.deepEqual(source, before)
      }
      assert.throws(() => importExcalidraw(' '.repeat(80 * 1024 * 1024 + 1)), { message: zh
        ? '绘图超过 80 MB 导入上限，请缩小文件后重试。' : 'Drawing exceeds the 80 MB import limit' })

      const base = { elements: [], generated: [], files: {} }
      for (const [scene, message] of [
        [{ ...base, elements: null }, zh ? '保存的自由编辑内容格式不正确。' : 'Invalid saved board scene'],
        [{ ...base, elements: [{ id: 'a', type: 'rectangle', x: 0, y: 0, width: 10, height: 10, link: 'javascript:alert(1)' }] }, zh ? '绘图包含不支持的链接类型，无法打开。' : 'Unsafe board link'],
        [{ ...base, files: { a: { dataURL: 'https://example.invalid/tracker.png' } } }, zh ? '保存的绘图图片数据无效，或引用了外部图片。' : 'Invalid saved board image'],
      ] as const) {
        const before = structuredClone(scene)
        assert.throws(() => validateScene(scene), { message })
        assert.deepEqual(scene, before)
      }
    } finally { setLangChoice(previous) }
  })

  test(`${lang}: rejected model restoration is atomic and can be retried after its dependencies return`, () => {
    const previous = langChoice()
    setLangChoice(lang)
    const zh = lang === 'zh'
    try {
      const graph = applyOps(emptyGraph(), parseOps('E 「Parent 中文」 「Child 中文」'))
      const node = { ...graph.nodes[1], parent: graph.nodes[0].id }
      const scene: FreeScene = { elements: [], generated: [], files: {}, modelCandidates: {
        [node.id]: { kind: 'node', id: node.id, restore: node },
      } }
      const missing = emptyGraph(), original = structuredClone({ missing, scene })
      assert.throws(() => adoptSceneOwner(missing, scene, { kind: 'node', id: node.id }), { message: zh
        ? '这条模型建议对应的上级节点已不存在，无法采用。' : 'The proposed node parent no longer exists.' })
      assert.deepEqual({ missing, scene }, original)
      const recovered = adoptSceneOwner({ ...missing, nodes: [graph.nodes[0]] }, scene, { kind: 'node', id: node.id })
      assert.deepEqual(recovered.graph.nodes.find(n => n.id === node.id), node)

      const edge = graph.edges[0], id = `${edge.from}>${edge.to}`
      const edgeScene: FreeScene = { elements: [], generated: [], files: {}, modelCandidates: {
        [id]: { kind: 'edge', id, from: edge.from, to: edge.to, restore: edge },
      } }
      const beforeEdge = structuredClone(edgeScene)
      assert.throws(() => adoptSceneOwner(missing, edgeScene, { kind: 'edge', from: edge.from, to: edge.to }), { message: zh
        ? '这条模型建议连接的节点已不存在，无法采用。' : 'The proposed edge endpoints no longer exist.' })
      assert.deepEqual(edgeScene, beforeEdge)
      assert.deepEqual(missing, original.missing)
      assert.deepEqual(adoptSceneOwner({ ...graph, edges: [] }, edgeScene, { kind: 'edge', from: edge.from, to: edge.to }).graph.edges, [edge])
    } finally { setLangChoice(previous) }
  })
}
