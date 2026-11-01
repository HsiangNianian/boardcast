// 下载 llama.cpp 官方预编译的 llama-server，精简后放到 src-tauri/llama/，打包时作为资源带进安装包。
//   node scripts/fetch-llama.mjs [linux-x64 | windows-x64 | windows-arm64 | macos-arm64 | macos-x64]
// 不传参数就按当前机器判断。已经准备好同一个版本就跳过。
//
// Linux 和 Windows 用 Vulkan 版：N 卡、A 卡、Intel 核显、骁龙 Adreno 都能用；
// 后端是运行时动态加载的，机器上没有 Vulkan 也会自动退回 CPU。
// macOS 用官方的 Metal 版。
// 升级版本时记得同步 .github/workflows/eval.yml 里的 LLAMA_BUILD。

import { execFileSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const LLAMA_BUILD = 'b11954'

const ASSETS = {
  'linux-x64': `llama-${LLAMA_BUILD}-bin-ubuntu-vulkan-x64.tar.gz`,
  'windows-x64': `llama-${LLAMA_BUILD}-bin-win-vulkan-x64.zip`,
  'windows-arm64': `llama-${LLAMA_BUILD}-bin-win-vulkan-arm64.zip`,
  'macos-arm64': `llama-${LLAMA_BUILD}-bin-macos-arm64.tar.gz`,
  'macos-x64': `llama-${LLAMA_BUILD}-bin-macos-x64.tar.gz`,
}

function hostTarget() {
  const os = { linux: 'linux', win32: 'windows', darwin: 'macos' }[process.platform]
  const arch = { x64: 'x64', arm64: 'arm64' }[process.arch]
  return `${os}-${arch}`
}

const target = process.argv[2] ?? hostTarget()
const asset = ASSETS[target]
if (!asset) {
  console.error(`fetch-llama: 不支持的平台 ${target}（支持：${Object.keys(ASSETS).join('、')}）`)
  process.exit(1)
}

const root = fileURLToPath(new URL('..', import.meta.url))
const out = join(root, 'src-tauri/llama')
const marker = join(out, '.version')
const want = `${LLAMA_BUILD} ${target}`
if (existsSync(marker) && readFileSync(marker, 'utf8').trim() === want) process.exit(0)

const cache = join(root, 'node_modules/.cache/llama')
mkdirSync(cache, { recursive: true })
const archive = join(cache, asset)
if (!existsSync(archive)) {
  const url = `https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_BUILD}/${asset}`
  console.log(`fetch-llama: 下载 ${url}`)
  // 用 curl 而不是 fetch：curl 会走系统代理，Windows 10 以上也自带
  execFileSync('curl', ['-fL', '--retry', '3', '-o', `${archive}.part`, url], { stdio: 'inherit' })
  copyFileSync(`${archive}.part`, archive)
  rmSync(`${archive}.part`)
}

const tmp = join(cache, `x-${target}`)
rmSync(tmp, { recursive: true, force: true })
mkdirSync(tmp, { recursive: true })
// bsdtar 能解 zip，Windows 自带的 tar 就是 bsdtar。
// Windows 上要明确用系统里那个：从 Git Bash 里跑时 PATH 前面是 GNU tar，它解不了 zip，还会把 D:\ 当成远程主机
const tar = process.platform === 'win32' ? join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar'
execFileSync(tar, ['-xf', archive, '-C', tmp], { stdio: 'inherit' })

// 包里可能有一层目录
const top = readdirSync(tmp)
const src = top.length === 1 && statSync(join(tmp, top[0])).isDirectory() ? join(tmp, top[0]) : tmp

rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })
const exe = target.startsWith('windows') ? 'llama-server.exe' : 'llama-server'
const names = readdirSync(src)
/** 留不留这个文件：只要 llama-server、它运行时会加载的库，加上许可证 */
function wanted(name) {
  if (name === exe || name === 'LICENSE') return true
  if (!/\.(dll|so|dylib)(\.\d+)*$/.test(name)) return false
  // 别的命令行工具（bench、cli、quantize……）各自的实现库，llama-server 用不到
  if (/llama-.+-impl\.(dll|so|dylib)$/.test(name) && !/llama-server-impl/.test(name)) return false
  // RPC 后端用不到；但 macOS 版的 llama-server 和各个库都直接链接了它，少了就起不来
  if (/ggml-rpc/.test(name) && !target.startsWith('macos')) return false
  // Linux：程序按 .so.0 这种 SONAME 加载；.so.x.y.z 是本体，无版本号的 .so 是给编译用的链接，
  // 两者都和 .so.0 内容一样。CPU、Vulkan 这些后端没有 SONAME 版本，按无版本号的名字动态加载，要留着
  if (/\.so\.\d+\.\d+\.\d+$/.test(name)) return false
  if (name.endsWith('.so') && names.includes(`${name}.0`)) return false
  // macOS 同理：程序按 libX.0.dylib 加载；libX.0.27.0.dylib 是本体，libX.dylib 是给编译用的链接
  if (/\.\d+\.\d+\.\d+\.dylib$/.test(name)) return false
  if (/\.dylib$/.test(name) && !/\.\d+\.dylib$/.test(name) && names.includes(name.replace(/\.dylib$/, '.0.dylib'))) return false
  return true
}
let kept = 0
let bytes = 0
for (const name of names) {
  if (!wanted(name)) continue
  // copyFileSync 会跟随符号链接，复制出来的是实际内容：打包工具对符号链接的处理不可靠
  copyFileSync(join(src, name), join(out, name))
  kept++
  bytes += statSync(join(out, name)).size
}
if (!existsSync(join(out, exe))) {
  console.error(`fetch-llama: 包里没有找到 ${exe}`)
  process.exit(1)
}
writeFileSync(marker, `${want}\n`)
rmSync(tmp, { recursive: true, force: true })
console.log(`fetch-llama: ${target} → src-tauri/llama/（${kept} 个文件，${(bytes / 1e6).toFixed(1)} MB）`)
