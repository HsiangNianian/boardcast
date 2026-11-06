import { msg } from '../i18n/index.ts'

export const IMAGE_EXT: Record<string, string> = { 'image/jpeg': 'jpg', 'image/png': 'png', 'image/webp': 'webp', 'image/gif': 'gif', 'image/svg+xml': 'svg' }
export const IMAGE_MIME: Record<string, string> = Object.fromEntries(Object.entries(IMAGE_EXT).map(([mime, ext]) => [ext, mime]))
export type AssetIssue = { key: string; reason: 'missing' | 'unreadable' | 'hash-mismatch' | 'decode' }
export type ImageDecoder = (dataUrl: string) => Promise<void>

export class AssetIntegrityError extends Error {
  readonly issue: AssetIssue
  constructor(issue: AssetIssue) {
    super(msg().errors.assetIntegrity(issue.key, issue.reason))
    this.issue = issue
  }
}

/** Hash the exact stored data URL, as v1 did. Never learn hashes from filenames. */
async function digest(dataUrl: string): Promise<string> {
  const bytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(dataUrl))
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

export async function assetKey(dataUrl: string): Promise<string | null> {
  const ext = IMAGE_EXT[/^data:([^;,]+)/.exec(dataUrl)?.[1] ?? '']
  return ext ? `${(await digest(dataUrl)).slice(0, 32)}.${ext}` : null
}

// Cache only successful decodes of immutable strings, never filename associations.
const decoded = new Map<string, Promise<void>>()
export const decodeAssetImage: ImageDecoder = async (dataUrl) => {
  const known = decoded.get(dataUrl)
  if (known) return known
  const task = new Promise<void>((resolve, reject) => {
    if (typeof Image === 'undefined') { reject(new Error('Image decoder unavailable')); return }
    const image = new Image()
    const finish = (error?: Error) => {
      clearTimeout(timer)
      image.onload = image.onerror = null
      image.removeAttribute('src')
      error ? reject(error) : resolve()
    }
    const timer = setTimeout(() => finish(new Error('Image decoding timed out')), 5000)
    image.onerror = () => finish(new Error('Invalid image'))
    image.onload = () => finish(image.naturalWidth > 0 && image.naturalHeight > 0 ? undefined : new Error('Empty image'))
    image.src = dataUrl
  })
  decoded.set(dataUrl, task)
  if (decoded.size > 64) decoded.delete(decoded.keys().next().value!)
  try { await task } catch (error) { decoded.delete(dataUrl); throw error }
}

export async function verifyAsset(dataUrl: string, key: string, decode: ImageDecoder = decodeAssetImage): Promise<void> {
  const match = /^([a-f0-9]{16,64})\.(jpg|png|webp|gif|svg)$/i.exec(key)
  if (!match || !dataUrl.startsWith(`data:${IMAGE_MIME[match[2].toLowerCase()]};base64,`)
    || (await digest(dataUrl)).slice(0, match[1].length) !== match[1].toLowerCase()) {
    throw new AssetIntegrityError({ key, reason: 'hash-mismatch' })
  }
  try { await decode(dataUrl) }
  catch { throw new AssetIntegrityError({ key, reason: 'decode' }) }
}

export function assetReadIssue(key: string, error: unknown): AssetIssue {
  if (error instanceof AssetIntegrityError) return error.issue
  // Native loading also verifies the hash before returning any bytes.
  return { key, reason: String(error).includes('ASSET_HASH_MISMATCH:') ? 'hash-mismatch' : 'unreadable' }
}
