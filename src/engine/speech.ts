import type { SpeechModels, VoiceSource, voice } from '../desktop.ts'

/** Finish the old capture before opening new inputs; a stop during either await cancels the start. */
export async function startVoice(
  input: Pick<typeof voice, 'start' | 'stop'>,
  request: { stop: boolean },
  device: string | undefined,
  source: VoiceSource,
  restart = false,
): Promise<void> {
  if (restart) await input.stop()
  if (request.stop) return
  await input.start(device, source)
  if (request.stop) await input.stop()
}

export type SpeechAssetState = 'installed' | 'downloading' | 'supported' | 'unsupported' | 'unknown'

export function appleLanguageState(view: SpeechModels, locale: string): SpeechAssetState {
  if (!view.apple?.available) return 'unsupported'
  if (locale !== 'auto') return view.apple.locales.find((l) => l.id === locale)?.status ?? 'unsupported'
  if (!view.senseVoiceInstalled || view.autoLocales?.length !== 3) return 'unsupported'
  const states = view.autoLocales.map((id) => view.apple!.locales.find((l) => l.id === id)?.status ?? 'unknown')
  for (const state of ['unsupported', 'unknown', 'downloading', 'supported'] as const) {
    if (states.includes(state)) return state
  }
  return 'installed'
}

export function appleLanguageChoice(view: SpeechModels, previous: string): string {
  return [previous, view.selection.provider === 'apple' ? view.selection.locale : '', 'auto']
    .find((id) => id === 'auto' || view.apple?.locales.some((l) => l.id === id)) ?? 'auto'
}

/** A stable native error code; ordinary device/permission errors must not open model setup. */
export function appleAssetIssue(message: string): { locale: string; state: Exclude<SpeechAssetState, 'installed'> } | null {
  const match = /^APPLE_SPEECH_ASSETS_NOT_READY:([^:\s]+):(supported|downloading|unsupported|unknown)$/.exec(message)
  return match ? { locale: match[1], state: match[2] as Exclude<SpeechAssetState, 'installed'> } : null
}
