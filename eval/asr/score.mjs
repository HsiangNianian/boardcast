export function tokens(text, language) {
  const clean = text.normalize('NFKC').toLowerCase().replace(/[’']/g, '').replace(/_/g, '')
  if (language === 'en') return clean.match(/[\p{L}\p{N}]+/gu) || []
  if (language === 'zh-en') return clean.match(/\p{Script=Han}|[a-z0-9]+|[\p{L}\p{N}]/gu) || []
  return [...clean.replace(/[\p{P}\p{S}\s]/gu, '')]
}

export function errorRate(reference, hypothesis, language) {
  const want = tokens(reference, language), got = tokens(hypothesis, language)
  let previous = Array.from({ length: got.length + 1 }, (_, i) => i)
  for (let i = 0; i < want.length; i++) {
    const next = [i + 1]
    for (let j = 0; j < got.length; j++) next.push(Math.min(next[j] + 1, previous[j + 1] + 1, previous[j] + (want[i] === got[j] ? 0 : 1)))
    previous = next
  }
  return { errors: previous[got.length], units: want.length, rate: want.length ? previous[got.length] / want.length : null }
}

export function keywordRecall(keywords, hypothesis, language) {
  const got = tokens(hypothesis, language)
  const hits = keywords.filter((word) => {
    const want = tokens(word, language)
    return want.length && got.some((_, i) => want.every((t, j) => got[i + j] === t))
  })
  return { hits, total: keywords.length, rate: keywords.length ? hits.length / keywords.length : null }
}
