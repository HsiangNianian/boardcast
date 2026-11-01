// 官网上所有「手写」出来的字都要笔画数据（dev / build 前自动跑）：
// - 扫一遍 src/ 下的 .ts，收集字符串里出现的汉字（注释不算），从 hanzi-writer-data 里取按笔顺排好的骨架线；
// - 拉丁字母和数字用 Hershey futural 单线字体。
// 结果写到 src/gen/，只包含页面真正用到的字，几十 KB。

import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(import.meta.url)

function walk(dir, out = []) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f)
    if (statSync(p).isDirectory()) {
      if (f !== 'gen') walk(p, out)
    } else if (f.endsWith('.ts')) out.push(p)
  }
  return out
}

const chars = new Set()
for (const file of walk(join(root, 'src'))) {
  const code = readFileSync(file, 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:'"`])\/\/.*$/gm, '$1')
  for (const ch of code) if (/[㐀-鿿]/.test(ch)) chars.add(ch)
}

const hanziDir = dirname(require.resolve('hanzi-writer-data/package.json'))
const hanzi = {}
const missing = []
for (const ch of [...chars].sort()) {
  const p = join(hanziDir, `${ch}.json`)
  if (!existsSync(p)) {
    missing.push(ch)
    continue
  }
  hanzi[ch] = JSON.parse(readFileSync(p, 'utf8')).medians
}

const hersheyDir = dirname(require.resolve('hersheytext/package.json'))
const fonts = JSON.parse(readFileSync(join(hersheyDir, 'hersheytext.min.json'), 'utf8'))
const hershey = fonts.futural.chars.map((c) => [c.d, Number(c.o)])

const out = join(root, 'src/gen')
mkdirSync(out, { recursive: true })
const write = (name, data) => {
  const text = `${JSON.stringify(data)}\n`
  const file = join(out, name)
  if (!existsSync(file) || readFileSync(file, 'utf8') !== text) writeFileSync(file, text)
}
write('hanzi.json', hanzi)
write('hershey.json', hershey)
console.log(`glyphs: ${Object.keys(hanzi).length} 个汉字 → landing/src/gen/hanzi.json`)
if (missing.length) console.warn(`glyphs: hanzi-writer-data 里没有这些字，会写成空白：${missing.join('')}`)
