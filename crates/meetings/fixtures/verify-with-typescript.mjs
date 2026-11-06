// Holds meetings written by the Rust store to the older app's own rules.
// First run `boardcast-tools meetings-check <copy-of-data-dir>`, then:
//   node crates/meetings/fixtures/verify-with-typescript.mjs <repo> <copy-of-data-dir>
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

const [root, data] = process.argv.slice(2)
const load = (p) => import(pathToFileURL(`${root}/${p}`).href)
const { replays } = await load('src/engine/steps.ts')
const { describe } = await load('src/engine/graph.ts')
const { StableLayout } = await load('src/board/layout.ts')
const { sourceHash } = await load('src/meetings.ts')

const problems = []
let checked = 0
for (const name of readdirSync(`${data}/meetings`).filter((f) => f.endsWith('.json'))) {
  const saved = `${data}/meetings/v2/${name}`
  if (!existsSync(saved)) continue
  checked++
  const original = JSON.parse(readFileSync(`${data}/meetings/${name}`, 'utf8'))
  const meeting = JSON.parse(readFileSync(saved, 'utf8'))
  if (meeting.migration?.sourceHash !== (await sourceHash(original))) problems.push(`${name}: migration hash differs`)
  if (meeting.version !== 2 || meeting.storageRevision < 1) problems.push(`${name}: version or revision`)
  if (describe(original.graph) !== describe(meeting.graph)) problems.push(`${name}: board text changed`)
  if (original.steps?.length && replays(original.steps, original.graph) && !replays(meeting.steps, meeting.graph)) problems.push(`${name}: no longer replays`)
  if (JSON.stringify(original.graph.nodes) !== JSON.stringify(meeting.graph.nodes)) problems.push(`${name}: node text changed`)
  if (JSON.stringify(original.graph.edges) !== JSON.stringify(meeting.graph.edges)) problems.push(`${name}: edge text changed`)
  const layout = new StableLayout()
  layout.restore(meeting.layout)
  await layout.layout(meeting.graph)
}
console.log(JSON.stringify({ checked, problems }, null, 1))
if (problems.length || !checked) process.exit(1)
