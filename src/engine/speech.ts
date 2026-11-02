export type SpeechAssetState = 'installed' | 'downloading' | 'supported' | 'unsupported' | 'unknown'

/** A stable native error code; ordinary device/permission errors must not open model setup. */
export function appleAssetIssue(message: string): { locale: string; state: Exclude<SpeechAssetState, 'installed'> } | null {
  const match = /^APPLE_SPEECH_ASSETS_NOT_READY:([^:\s]+):(supported|downloading|unsupported|unknown)$/.exec(message)
  return match ? { locale: match[1], state: match[2] as Exclude<SpeechAssetState, 'installed'> } : null
}
