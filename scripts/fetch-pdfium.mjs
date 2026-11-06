// Download a pinned PDFium build to target/pdfium, which the import crate loads and packaging ships.
//   node scripts/fetch-pdfium.mjs [linux-x64 | linux-arm64 | mac-arm64 | mac-x64 | win-x64]
// The build is checked against its SHA-256, and nothing is done if that build is already there.

import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const BUILD = 'chromium/8138'
const SHA256 = {
  'linux-x64': 'd6b1e28950a0392590d79bbdbb7eb19686872b688f8a36f170acb19e41473113',
  'linux-arm64': 'c70b1ce4d13257f741af19e0aeee575890875995eb81370a65be942e601a1862',
  'mac-arm64': '21a1567b397b571f4f77f21ca91eac7b892774219c3c715a5127a20950ccb18b',
  'mac-x64': '72662ac8eff7bf5381ef82ceebd0a0ad5bddb051b303981115114e6972fd201c',
  'win-x64': '8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5',
}
const LIBRARY = { linux: 'lib/libpdfium.so', mac: 'lib/libpdfium.dylib', win: 'bin/pdfium.dll' }

const platform = process.argv[2] ?? `${{ linux: 'linux', darwin: 'mac', win32: 'win' }[process.platform]}-${process.arch}`
const digest = SHA256[platform]
if (!digest) throw new Error(`no PDFium build for ${platform}`)
const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const out = join(root, 'target/pdfium')
const marker = join(out, '.version')
const wanted = `${BUILD} ${platform}`
if (existsSync(marker) && readFileSync(marker, 'utf8').trim() === wanted) process.exit(0)

const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/${encodeURIComponent(BUILD)}/pdfium-${platform}.tgz`
const response = await fetch(url)
if (!response.ok) throw new Error(`${url}: ${response.status}`)
const bytes = Buffer.from(await response.arrayBuffer())
const actual = createHash('sha256').update(bytes).digest('hex')
if (actual !== digest) throw new Error(`PDFium download does not match its checksum: ${actual}`)

const work = mkdtempSync(join(tmpdir(), 'pdfium-'))
try {
  writeFileSync(join(work, 'pdfium.tgz'), bytes)
  execFileSync('tar', ['-xzf', join(work, 'pdfium.tgz'), '-C', work])
  rmSync(out, { recursive: true, force: true })
  mkdirSync(out, { recursive: true })
  const library = LIBRARY[platform.split('-')[0]]
  copyFileSync(join(work, library), join(out, library.split('/').pop()))
  copyFileSync(join(work, 'LICENSE'), join(out, 'LICENSE'))
  writeFileSync(marker, wanted)
} finally {
  rmSync(work, { recursive: true, force: true })
}
