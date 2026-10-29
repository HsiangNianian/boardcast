#!/usr/bin/env node
// Cut a release:
//
//   pnpm release 0.2.0           bump versions, prepend the CHANGELOG section, commit and tag v0.2.0
//   pnpm release 0.2.0 --push    ...and push the commit to main and the tag, which starts the Release workflow
//   pnpm release 0.2.0 --dry-run print the CHANGELOG section and the files that would change, touch nothing
//
// The CHANGELOG section is generated from the Conventional Commits since the last tag (scripts/changelog.mjs).
// Edit CHANGELOG.md before pushing if you want to reword it: the Release workflow uses that section as the release body.

import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { commitsBetween, latestTag, repoUrl, section, today } from './changelog.mjs'

const args = process.argv.slice(2)
const version = args.find((a) => !a.startsWith('--'))?.replace(/^v/, '')
const push = args.includes('--push')
const dry = args.includes('--dry-run')

const git = (...a) => execFileSync('git', a, { encoding: 'utf8' }).trim()
const die = (msg) => {
  console.error(msg)
  process.exit(1)
}

if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) die('usage: pnpm release <x.y.z> [--push | --dry-run]')
const tag = `v${version}`
if (!dry && git('status', '--porcelain')) die('working tree is not clean; commit or stash first')
try {
  git('rev-parse', '-q', '--verify', `refs/tags/${tag}`)
  die(`${tag} already exists`)
} catch {
  /* good: the tag is new */
}

const current = JSON.parse(readFileSync('package.json', 'utf8')).version
const newer = (a, b) => {
  const [x, y] = [a, b].map((v) => v.split('-')[0].split('.').map(Number))
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i]
  return a !== b
}
if (!newer(version, current)) die(`${version} is not newer than the current version ${current}`)

// Every place the version lives
const edits = [
  ['package.json', (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`)],
  ['src-tauri/tauri.conf.json', (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`)],
  ['src-tauri/Cargo.toml', (s) => s.replace(/^(version\s*=\s*")[^"]+(")/m, `$1${version}$2`)],
  ['src-tauri/Cargo.lock', (s) => s.replace(/(\[\[package\]\]\nname = "boardcast"\nversion = ")[^"]+(")/, `$1${version}$2`)],
]

const from = latestTag()
const notes = section({ version, date: today(), commits: commitsBetween(from), repo: repoUrl(), from, to: tag })

if (dry) {
  console.log(notes)
  console.log(`would bump ${current} → ${version} in: ${edits.map(([f]) => f).join(', ')}`)
  process.exit(0)
}

for (const [file, edit] of edits) {
  const before = readFileSync(file, 'utf8')
  const after = edit(before)
  if (after === before) die(`could not find the version in ${file}`)
  writeFileSync(file, after)
}

const HEAD = '# Changelog\n\nAll notable changes to boardcast. Generated from Conventional Commits by `pnpm release`.\n'
const old = existsSync('CHANGELOG.md') ? readFileSync('CHANGELOG.md', 'utf8') : HEAD
const at = old.search(/^## /m)
const log = at < 0 ? `${old.trimEnd()}\n\n${notes}` : `${old.slice(0, at)}${notes}\n${old.slice(at)}`
writeFileSync('CHANGELOG.md', log)

git('add', 'CHANGELOG.md', ...edits.map(([f]) => f))
git('commit', '-q', '-m', `chore(release): ${tag}`)
// Signed tag when commits are signed, annotated otherwise
let sign = false
try {
  sign = git('config', '--get', 'tag.gpgSign') === 'true' || git('config', '--get', 'commit.gpgsign') === 'true'
} catch {
  /* not configured */
}
git('tag', sign ? '-s' : '-a', tag, '-m', tag)
console.log(notes)
console.log(`committed chore(release): ${tag} and tagged ${tag}`)

if (push) {
  git('push', 'origin', 'HEAD:main')
  git('push', 'origin', tag)
  console.log(`pushed; the Release workflow will build and publish ${tag}`)
} else {
  console.log(`review CHANGELOG.md, then: git push origin HEAD:main && git push origin ${tag}`)
}
