// Holds the Rust reading of a real PDF to the older engine's, using the pieces the Rust side extracted:
//   node crates/import/fixtures/compare-with-typescript.mjs <repo> <out.json from read-pdf>
import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

const [root, file] = process.argv.slice(2)
const { analyzePage } = await import(pathToFileURL(`${root}/src/board/paper.ts`).href)
const pages = JSON.parse(readFileSync(file, 'utf8'))
const state = { inRefs: false }
const near = (a, b) => Math.abs(a - b) < 1e-6
const problems = []
let areas = 0
const kinds = {}
for (const [p, page] of pages.entries()) {
  const ts = analyzePage(page.pieces, page.w, page.h, state)
  if (ts.columns !== page.columns) problems.push(`page ${p + 1}: columns ${ts.columns} vs ${page.columns}`)
  if (JSON.stringify(ts.headings) !== JSON.stringify(page.headings)) problems.push(`page ${p + 1}: headings`)
  if (ts.areas.length !== page.areas.length) problems.push(`page ${p + 1}: ${ts.areas.length} blocks vs ${page.areas.length}`)
  for (const [i, a] of ts.areas.entries()) {
    const b = page.areas[i]
    if (!b) continue
    areas++
    kinds[a.kind] = (kinds[a.kind] ?? 0) + 1
    if (a.kind !== b.kind || a.text !== b.text || (a.name ?? null) !== b.name || !near(a.x, b.x) || !near(a.y, b.y) || !near(a.w, b.w) || !near(a.h, b.h) || (a.rows?.length ?? 0) !== b.rows) {
      problems.push(`page ${p + 1} block ${i}: ${a.kind}/${b.kind} ${JSON.stringify(a.text.slice(0, 40))} vs ${JSON.stringify(b.text.slice(0, 40))}`)
    }
  }
}
console.log(JSON.stringify({ pages: pages.length, areas, kinds, problems: problems.slice(0, 12), count: problems.length }, null, 1))
process.exit(problems.length ? 1 : 0)
