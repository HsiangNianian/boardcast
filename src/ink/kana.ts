/*! KanjiVG kana, Copyright (C) 2009/2010/2011 Ulrich Apel.
 * https://kanjivg.tagaini.net/ — CC BY-SA 3.0 https://creativecommons.org/licenses/by-sa/3.0/
 * Adapted to normalized centerlines; see public/licenses/kanjivg-CC-BY-SA-3.0.txt.
 */
import data from './kana.json' with { type: 'json' }

export const kana = data as unknown as Record<string, { strokes: [number, number][][]; advance: number }>

/** Join dakuten and expand half-width kana without changing math or other text. */
export function normalizeKana(text: string): string {
  return text.replace(/[\uff61-\uff9f]+/g, (s) => s.normalize('NFKC')).normalize('NFC')
}
