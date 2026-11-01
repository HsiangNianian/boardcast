// 首屏巨字用的粗黑体：思源黑体（Noto Sans CJK SC）Black，只留页面上用到的那几十个字，压成 woff2。
// 结果已经放在 public/fonts/display.woff2 里，改了 DISPLAY 里的字才需要重跑：
//   node landing/scripts/font.mjs [/path/to/NotoSansCJK-Black.ttc]
// 需要 uv（会临时装 fonttools + brotli）。字体许可：SIL Open Font License 1.1，见 public/fonts/OFL.txt。

import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

/** 用巨字排的所有文字（标题、幕名、数字） */
const DISPLAY = [
  '说话，板书自己长出来。',
  '连线成图当场涂改拖拽回弹白板黑板',
  '0123456789↔—',
].join('')

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const src = process.argv[2] ?? '/usr/share/fonts/noto-cjk/NotoSansCJK-Black.ttc'
if (!existsSync(src)) {
  console.error(`找不到 ${src}，把 Noto Sans CJK Black 的 .ttc / .otf 路径作为参数传进来`)
  process.exit(1)
}
const text = [...new Set(DISPLAY)].join('')
execFileSync(
  'uv',
  [
    'run', '--quiet', '--with', 'fonttools', '--with', 'brotli',
    'pyftsubset', src,
    // .ttc 里第 3 个是简体中文
    ...(src.endsWith('.ttc') ? ['--font-number=2'] : []),
    `--text=${text}`,
    '--flavor=woff2',
    '--layout-features=*',
    '--no-hinting',
    `--output-file=${join(root, 'public/fonts/display.woff2')}`,
  ],
  { stdio: 'inherit' },
)
console.log(`font: ${text.length} 个字 → landing/public/fonts/display.woff2`)
