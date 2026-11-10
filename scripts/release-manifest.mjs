// The updater's manifest for a release: every Linux package variant in a folder, keyed by platform and
// variant (x86_64-linux for the full package, x86_64-linux-ja for another), with its size, hash and signature.
//   VERSION=0.22.0 NOTES='…' node scripts/release-manifest.mjs <packages dir> <release tag> > manifest.json
// GITHUB_REPOSITORY names the repository the packages are downloaded from.

import { createHash } from 'node:crypto'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const [dir, tag] = process.argv.slice(2)
const { VERSION: version, NOTES: notes = '', GITHUB_REPOSITORY: repo = 'HsiangNianian/boardcast' } = process.env
if (!dir || !tag || !version) {
  console.error('usage: VERSION=<version> node scripts/release-manifest.mjs <packages dir> <release tag>')
  process.exit(2)
}

const platforms = {}
for (const name of readdirSync(dir).sort()) {
  const m = name.match(/^boardcast-(.+)-(x86_64|aarch64)-linux(?:-([a-z]+))?\.tar\.gz$/)
  if (!m) continue
  const [, packaged, arch, variant] = m
  if (packaged !== version) throw new Error(`${name} is not version ${version}`)
  const bytes = readFileSync(join(dir, name))
  platforms[`${arch}-linux${variant ? `-${variant}` : ''}`] = {
    url: `https://github.com/${repo}/releases/download/${tag}/${name}`,
    size: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    signature: readFileSync(join(dir, `${name}.sig`), 'utf8').trim(),
  }
}
if (!platforms['x86_64-linux']) throw new Error('the full Linux package is missing')
console.log(JSON.stringify({ version, notes, platforms }, null, 1))
