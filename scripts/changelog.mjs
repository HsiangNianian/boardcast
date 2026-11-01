#!/usr/bin/env node
// Turn Conventional Commits into a CHANGELOG section.
//
//   node scripts/changelog.mjs notes v0.2.0        print the release notes for a tag:
//                                                   the matching CHANGELOG.md section if there is one,
//                                                   otherwise generated from the commits since the previous tag
//   node scripts/changelog.mjs preview [version]    print the section the next release would get (nothing is written)
//
// Release notes follow CHANGELOG.md: "## 0.2.0 - 2026-10-29", then one "###" group per commit type.
// Commit bodies written as "- ..." bullets are kept as nested points.

import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const GROUPS = [
  ['breaking', 'Breaking changes'],
  ['feat', 'Features'],
  ['fix', 'Fixes'],
  ['perf', 'Performance'],
  ['refactor', 'Refactoring'],
  ['deps', 'Dependencies'],
  ['docs', 'Documentation'],
  ['test', 'Tests'],
  ['ci', 'Build and CI'],
  ['chore', 'Maintenance'],
]

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim()

/** The tag before `ref`, or null when `ref` is the first release */
export function previousTag(ref) {
  try {
    return git('describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*', `${ref}^`)
  } catch {
    return null
  }
}

/** The newest release tag reachable from HEAD, or null before the first release */
export function latestTag() {
  try {
    return git('describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*')
  } catch {
    return null
  }
}

/** Commits in `from..to` (or everything up to `to`), oldest first */
export function commitsBetween(from, to = 'HEAD') {
  const range = from ? `${from}..${to}` : to
  const raw = git('log', '--reverse', '--no-merges', '--format=%H%x1f%s%x1f%b%x1e', range)
  return raw
    .split('\x1e')
    .map((r) => r.trim())
    .filter(Boolean)
    .map((r) => {
      const [sha, subject, body = ''] = r.split('\x1f')
      return { sha, subject, body }
    })
}

const HEADER = /^(\w+)(?:\(([^)]+)\))?(!)?:\s+(.+)$/

/** Which group a commit goes into, or null to leave it out (release commits, merges) */
export function classify(c) {
  const m = HEADER.exec(c.subject)
  if (!m) return { group: 'chore', scope: undefined, text: c.subject }
  const [, type, scope, bang, text] = m
  if (type === 'chore' && scope === 'release') return null
  const breaking = !!bang || /^BREAKING[ -]CHANGE:/m.test(c.body)
  let group = type
  if (scope === 'deps' || scope === 'deps-dev') group = 'deps'
  else if (type === 'build') group = 'ci'
  else if (type === 'style' || type === 'revert') group = 'chore'
  if (breaking) group = 'breaking'
  if (!GROUPS.some(([g]) => g === group)) group = 'chore'
  return { group, scope, text }
}

const capital = (s) => s.charAt(0).toUpperCase() + s.slice(1)

/** The "- ..." lines of a commit body, as nested bullets */
function points(body) {
  return body
    .split('\n')
    .map((l) => l.trimEnd())
    .filter((l) => /^\s*[-*] /.test(l))
    .map((l) => `  - ${l.replace(/^\s*[-*] /, '')}`)
}

/** One CHANGELOG section */
export function section({ version, date, commits, repo, from, to }) {
  const by = new Map(GROUPS.map(([g]) => [g, []]))
  for (const c of commits) {
    const k = classify(c)
    if (!k) continue
    const link = repo ? ` ([\`${c.sha.slice(0, 7)}\`](${repo}/commit/${c.sha}))` : ''
    const scope = k.scope ? `**${k.scope}:** ` : ''
    by.get(k.group).push([`- ${scope}${capital(k.text)}${link}`, ...points(c.body)].join('\n'))
  }
  const out = [`## ${version} - ${date}`, '']
  for (const [g, title] of GROUPS) {
    const items = by.get(g)
    if (!items.length) continue
    out.push(`### ${title}`, '', ...items, '')
  }
  if (out.length === 2) out.push('No user-facing changes.', '')
  if (repo && from) out.push(`**Full diff:** [${from}...${to}](${repo}/compare/${from}...${to})`, '')
  return out.join('\n')
}

/** https://github.com/owner/name for the origin remote (or the Actions environment) */
export function repoUrl() {
  if (process.env.GITHUB_SERVER_URL && process.env.GITHUB_REPOSITORY)
    return `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}`
  try {
    const u = git('remote', 'get-url', 'origin')
    const m = /github\.com[:/](.+?)(?:\.git)?$/.exec(u)
    return m ? `https://github.com/${m[1]}` : null
  } catch {
    return null
  }
}

/** The "## <version>" section of a CHANGELOG, without its heading line's trailing blank lines */
export function extract(changelog, version) {
  const lines = changelog.split('\n')
  const start = lines.findIndex((l) => l.startsWith(`## ${version} `) || l === `## ${version}`)
  if (start < 0) return null
  let end = lines.findIndex((l, i) => i > start && l.startsWith('## '))
  if (end < 0) end = lines.length
  return `${lines.slice(start, end).join('\n').trim()}\n`
}

export const today = () => new Date().toISOString().slice(0, 10)

function main() {
  const [cmd, arg] = process.argv.slice(2)
  if (cmd === 'notes' && arg) {
    const version = arg.replace(/^v/, '')
    if (existsSync('CHANGELOG.md')) {
      const s = extract(readFileSync('CHANGELOG.md', 'utf8'), version)
      if (s) return process.stdout.write(s)
    }
    const from = previousTag(arg)
    const date = git('log', '-1', '--format=%cs', arg)
    return process.stdout.write(section({ version, date, commits: commitsBetween(from, arg), repo: repoUrl(), from, to: arg }))
  }
  if (cmd === 'preview') {
    const last = latestTag()
    return process.stdout.write(
      section({ version: arg ?? 'Unreleased', date: today(), commits: commitsBetween(last), repo: repoUrl(), from: last, to: 'HEAD' }),
    )
  }
  console.error('usage: changelog.mjs notes <tag> | preview [version]')
  process.exit(2)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
