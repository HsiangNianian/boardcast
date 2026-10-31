import assert from 'node:assert/strict'
import { test } from 'node:test'
import { type PaperState, type TextPiece, analyzePage } from '../board/paper.ts'

// 造一页双栏论文：页宽 900，左栏 74–422，右栏 454–802，正文字高 13、行距 18
const W = 900
const H = 1165
const L = 74
const R = 454
const CW = 348
const line = (x: number, y: number, text: string, w = CW, h = 13): TextPiece => ({ text, x, y, w, h })
/** 一段正文：首行缩进，最后一行写不满 */
function para(x: number, y: number, n: number, word = 'residual'): TextPiece[] {
  return Array.from({ length: n }, (_, i) => {
    const indent = i === 0 ? 14 : 0
    const w = i === n - 1 ? CW * 0.55 : CW - indent
    return line(x + indent, y + i * 18, `${word} learning makes very deep networks easier to optimize and gain accuracy`.slice(0, 66), w)
  })
}

function page(): TextPiece[] {
  return [
    // 侧边竖排的 arXiv 标记、页码
    { text: 'arXiv:1512.03385v1 [cs.CV] 10 Dec 2015', x: 20, y: 300, w: 20, h: 400, rotated: true },
    line(445, 1090, '3', 8),
    // 左栏：章节标题、两段正文、一个图（图里的零碎标注 + 图注在下面）
    line(L, 100, '3. Deep Residual Learning', 230, 16),
    line(L, 128, '3.1. Residual Learning', 160, 14),
    ...para(L, 156, 4),
    ...para(L, 236, 3, 'shortcut'),
    line(L + 40, 320, 'training error (%)', 90, 9),
    line(L + 150, 340, '56-layer', 40, 9),
    line(L + 150, 360, '20-layer', 40, 9),
    line(L + 60, 420, 'iter. (1e4)', 50, 9),
    line(L, 450, 'Figure 1. Training error (left) and test error (right) on CIFAR-10', CW, 12),
    line(L, 466, 'with 20-layer and 56-layer plain networks.', 200, 12),
    ...para(L, 500, 3, 'degradation'),
    // 正文里引用图的行（不是图注）
    line(L, 560, 'Fig. 1). When deeper networks are able to start converging, a problem', CW),
    // 右栏：公式（编号单独一片）、表注在上的表、正文
    ...para(R, 100, 3, 'identity'),
    line(R + 80, 166, 'y = F(x, {Wi}) + x.', 140),
    line(R + 330, 166, '(1)', 18),
    ...para(R, 196, 3, 'projection'),
    line(R, 270, 'Table 1. Architectures for ImageNet. Building blocks are shown in brackets', CW, 12),
    ...['layer name', 'output size', '18-layer', '34-layer', 'conv1', '112x112', '7x7, 64', 'conv2_x', '56x56'].map((t, i) =>
      line(R + 10 + (i % 3) * 110, 300 + Math.floor(i / 3) * 18, t, 80, 10),
    ),
    ...para(R, 380, 4, 'plain'),
    line(R, 470, '4. Experiments', 120, 16),
    ...para(R, 498, 3, 'imagenet'),
  ]
}

test('a two-column paper page is split by column, section, paragraph, figure, table and equation', () => {
  const state: PaperState = { inRefs: false }
  const r = analyzePage(page(), W, H, state)
  assert.equal(r.columns, 2)
  const names = r.areas.filter((a) => a.name).map((a) => a.name)
  assert.deepEqual(
    names.filter((n) => n!.startsWith('§')),
    ['§3 Deep Residual Learning', '§3.1 Residual Learning', '§4 Experiments'],
  )
  assert.deepEqual(r.headings, ['§3 Deep Residual Learning', '§3.1 Residual Learning', '§4 Experiments'])
  // 图：图注加上面的零碎标注合成一块，名字用图注
  const fig = r.areas.find((a) => a.kind === 'figure')!
  assert.match(fig.name!, /^图 1：Training error \(left\)/)
  assert.ok(fig.y < 325 && fig.y + fig.h >= 476, '图的范围包括上面的坐标轴和下面两行图注')
  assert.ok(!r.areas.some((a) => /56-layer|iter\. \(1e4\)/.test(a.text) && a.kind === 'text'), '图里的标注并进了图')
  // 表注在上：表格的格子在下面
  const table = r.areas.find((a) => a.kind === 'table')!
  assert.match(table.name!, /^表 1：Architectures/)
  assert.ok(table.y <= 272 && table.y + table.h >= 345)
  // 公式和它的编号合成一块
  const eq = r.areas.find((a) => a.kind === 'equation')!
  assert.equal(eq.name, '公式 (1)')
  assert.match(eq.text, /y = F\(x.*\(1\)$/)
  // 竖排的 arXiv 标记、页码不要；正文里引用图的「Fig. 1).」不是图注
  assert.ok(!r.areas.some((a) => /arXiv|^3$/.test(a.text)))
  assert.equal(r.areas.filter((a) => a.kind === 'figure').length, 1)
  // 段落按首行缩进分开，不跨栏
  const paras = r.areas.filter((a) => a.kind === 'text')
  assert.ok(paras.every((a) => a.x + a.w <= 430 || a.x >= 445), '没有跨栏的段落')
  assert.ok(paras.some((a) => a.text.startsWith('residual')) && paras.some((a) => a.text.startsWith('shortcut')), '两段没拼在一起')
})

test('the reference list is marked, and the section after it ends it', () => {
  const state: PaperState = { inRefs: false }
  const refs = [
    line(L, 100, 'References', 90, 15),
    line(L, 125, '[1] Y. Bengio, P. Simard, and P. Frasconi. Learning long-term dependencies', CW, 11),
    line(L, 139, 'with gradient descent is difficult. IEEE Transactions, 1994.', 300, 11),
    line(L, 155, '[2] C. M. Bishop. Neural networks for pattern recognition. 1995.', CW, 11),
  ]
  const a = analyzePage(refs, W, H, state)
  assert.ok(state.inRefs)
  assert.equal(a.areas.filter((x) => x.kind === 'reference').length, 2, '一条引用的续行接在它后面')
  // 下一页还是参考文献
  const b = analyzePage([line(L, 100, '[3] K. He and J. Sun. Convolutional neural networks. 2015.', CW, 11)], W, H, state)
  assert.equal(b.areas[0].kind, 'reference')
  // 附录开始：不再是参考文献
  const c = analyzePage([line(L, 100, 'A. Object Detection Baselines', 240, 16), ...para(L, 130, 3)], W, H, state)
  assert.ok(!state.inRefs)
  assert.equal(c.areas[0].name, '§A Object Detection Baselines')
})

test('sentence-like lines and figure labels are not section headings', () => {
  const r = analyzePage(
    [
      line(L, 100, '10 The responses are the outputs of each layer', 300),
      line(L, 130, '3 x3 conv, 64', 80),
      line(L, 160, '100 K 4.92 25.7', 120),
      ...para(L, 190, 3),
    ],
    W,
    H,
  )
  assert.deepEqual(r.headings, [])
})
