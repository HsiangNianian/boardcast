// 准备手写模式用的笔画数据（dev 和 build 前自动跑，已经准备好就跳过）：
// - 汉字：hanzi-writer-data 每个字一份 JSON，只保留按笔顺排列的骨架线（medians），
//   复制到 public/hanzi/，运行时按需加载。数据使用 Arphic Public License，许可文本一并带上。
// - 拉丁字母和数字：从 hersheytext 里抽出 futural 这一套单线字体，写成一个小 JSON。

import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync, copyFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const require = createRequire(import.meta.url)
const root = fileURLToPath(new URL('..', import.meta.url))

const hanziSrc = dirname(require.resolve('hanzi-writer-data/package.json'))
const hanziOut = join(root, 'public/hanzi')
const marker = join(hanziOut, '.done-2.0.1')
if (!existsSync(marker)) {
  mkdirSync(hanziOut, { recursive: true })
  let n = 0
  for (const f of readdirSync(hanziSrc)) {
    if (!f.endsWith('.json') || f === 'package.json') continue
    const { medians } = JSON.parse(readFileSync(join(hanziSrc, f), 'utf8'))
    if (!medians) continue
    writeFileSync(join(hanziOut, f), JSON.stringify(medians))
    n++
  }
  copyFileSync(join(hanziSrc, 'ARPHICPL.TXT'), join(hanziOut, 'ARPHICPL.TXT'))
  writeFileSync(marker, String(n))
  console.log(`glyphs: ${n} 个汉字 → public/hanzi/`)
}

// futural：手写印刷体；scripts：单线连写体，潦草度高的时候用
const hersheyOut = join(root, 'src/ink/hershey.json')
const hersheyDir = dirname(require.resolve('hersheytext/package.json'))
const fonts = JSON.parse(readFileSync(join(hersheyDir, 'hersheytext.min.json'), 'utf8'))
const pick = (name) => fonts[name].chars.map((c) => [c.d, Number(c.o)])
const hershey = JSON.stringify({ print: pick('futural'), script: pick('scripts') }) + '\n'
if (!existsSync(hersheyOut) || readFileSync(hersheyOut, 'utf8') !== hershey) {
  mkdirSync(dirname(hersheyOut), { recursive: true })
  writeFileSync(hersheyOut, hershey)
  console.log('glyphs: Hershey futural + scripts → src/ink/hershey.json')
}
