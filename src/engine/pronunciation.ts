import type { IpadicFeatures, Tokenizer } from 'kuromoji'

let japanese: Promise<Tokenizer<IpadicFeatures>> | undefined

async function tokenizer() {
  japanese ??= (async () => {
    const browser = typeof XMLHttpRequest !== 'undefined'
    const nodePackage = 'kuromoji', nodeUrl = 'node:url'
    const kuromoji: typeof import('kuromoji') = browser ? (await import('kuromoji/build/kuromoji.js')).default
      : (await import(/* @vite-ignore */ nodePackage)).default
    const dicPath = browser ? `${import.meta.env.BASE_URL}pronunciation/ja/`
      : (await import(/* @vite-ignore */ nodeUrl)).fileURLToPath(new URL(/* @vite-ignore */ '../../node_modules/kuromoji/dict/', import.meta.url))
    return new Promise<Tokenizer<IpadicFeatures>>((resolve, reject) => {
      kuromoji.builder({ dicPath }).build((error, result) => error ? reject(error) : resolve(result))
    })
  })().catch((error) => { japanese = undefined; throw error })
  return japanese
}

// Letter names, rather than the dictionary's article pronunciation of "a".
const LETTERS = ['EY', 'B IY', 'S IY', 'D IY', 'IY', 'EH F', 'JH IY', 'EY CH', 'AY', 'JH EY', 'K EY', 'EH L', 'EH M', 'EH N', 'OW', 'P IY', 'K Y UW', 'AA R', 'EH S', 'T IY', 'Y UW', 'V IY', 'D AH B AH L Y UW', 'EH K S', 'W AY', 'Z IY']

// shortcut: OOV names need explicit aliases; add G2P only after measuring false merges on meeting names.
/** Offline pronunciation keys; unknown words deliberately produce no key. */
export async function pronunciationKeys(texts: string[], language: 'zh' | 'ja'): Promise<string[][]> {
  const normalized = texts.map((s) => s.normalize('NFKC').trim())
  const [cmu, pinyin, ja] = await Promise.all([
    normalized.some((s) => /[a-z]/i.test(s)) ? import('cmu-pronouncing-dictionary') : undefined,
    language === 'zh' && normalized.some((s) => /\p{Script=Han}/u.test(s)) ? import('pinyin-pro') : undefined,
    language === 'ja' && normalized.some((s) => /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u.test(s)) ? tokenizer() : undefined,
  ])
  function english(s: string): string[] {
    const words = s.replace(/([a-z])([A-Z])/g, '$1 $2').match(/[a-z]+(?:'[a-z]+)?/gi) ?? []
    let variants = ['']
    for (const word of words) {
      const key = word.toLowerCase()
      const sounds = [cmu?.dictionary[key], ...[1, 2, 3].map((n) => cmu?.dictionary[`${key}(${n})`])].filter((s): s is string => !!s)
      if (/^[A-Z]{1,6}$/.test(word)) sounds.unshift(Array.from(word, (c) => LETTERS[c.charCodeAt(0) - 65]).join(' '))
      if (!sounds.length) return []
      variants = variants.flatMap((prefix) => sounds.map((s) => `${prefix} ${s.replace(/[012]/g, '')}`.trim())).slice(0, 16)
    }
    return words.length ? [...new Set(variants)] : []
  }
  return normalized.map((s) => {
    if (!s || /[^\p{L}\s'’-]/u.test(s)) return []
    if (/^[a-z\s'’-]+$/i.test(s)) return english(s).map((s) => `en:${s}`)
    // Mixed-script terms are matched via their explicit aliases; don't invent transliterations.
    if (/[a-z]/i.test(s)) return []
    if (ja) {
      const tokens = ja.tokenize(s)
      if (tokens.some((t) => !t.reading && /\p{Script=Han}/u.test(t.surface_form))) return []
      const kana = tokens.map((t) => t.reading ?? t.surface_form).join('').replace(/\s/g, '')
        .replace(/[ぁ-ゖ]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 0x60))
      return kana.length >= 3 ? [`ja:${kana}`] : []
    }
    if (pinyin && /^[\p{Script=Han}\s]+$/u.test(s) && s.replace(/\s/g, '').length >= 2) {
      return [`zh:${pinyin.pinyin(s.replace(/\s/g, ''), { toneType: 'none', type: 'array' }).join(' ')}`]
    }
    return []
  })
}
