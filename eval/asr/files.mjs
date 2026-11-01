import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createReadStream, existsSync, mkdirSync, renameSync, rmSync, statSync } from 'node:fs'
import { dirname } from 'node:path'

export async function sha256(path) {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest('hex')
}

// Check restored caches too: a cache hit is not evidence that its contents are intact.
export async function download(file, destination) {
  if (!/^[a-f0-9]{64}$/.test(file.sha256) || !Number.isSafeInteger(file.bytes) || file.bytes <= 0) {
    throw new Error(`Invalid checksum or size: ${destination}`)
  }
  const valid = async (path) => existsSync(path) && statSync(path).size === file.bytes && await sha256(path) === file.sha256
  if (await valid(destination)) return
  mkdirSync(dirname(destination), { recursive: true })
  const partial = `${destination}.${process.pid}.part`
  try {
    console.log(`Download ${file.url}`)
    execFileSync('curl', ['-fL', '--retry', '3', '--connect-timeout', '20', '--max-time', '900', '-o', partial, file.url], { stdio: 'inherit' })
    if (!await valid(partial)) throw new Error(`Size/SHA-256 mismatch: ${file.url}`)
    renameSync(partial, destination)
  } finally {
    rmSync(partial, { force: true })
  }
}
