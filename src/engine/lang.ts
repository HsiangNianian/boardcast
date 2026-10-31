// Which language a meeting is held in, guessed from what was said. The board and the summary are
// written in that language; this is independent of the interface language.

export type SpokenLang = 'en' | 'zh'

/**
 * Chinese unless the text is clearly English. One Han character carries roughly as much as a
 * short English word, and Chinese meetings are full of English terms, so Han characters are
 * weighted 6:1 against Latin letters: a Chinese meeting full of English terms
 * ("graphRAG 这块要接 ClickHouse") stays Chinese, and an English meeting that mentions a Chinese
 * name ("ask 小王 about the API") stays English. No text at all is Chinese, matching the prompts'
 * original behaviour.
 */
export function spokenLang(texts: readonly string[]): SpokenLang {
  let han = 0
  let latin = 0
  for (const t of texts) {
    han += t.match(/\p{Script=Han}/gu)?.length ?? 0
    latin += t.match(/[A-Za-z]/g)?.length ?? 0
  }
  return latin > 0 && han * 6 < latin ? 'en' : 'zh'
}
