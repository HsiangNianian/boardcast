// 不用模型的翻页：整句话就是一条翻页命令时直接翻，又快又不会写到板上。
// 「这里其实是第三页的内容」不是命令，交给模型。

export type Nav = { to: 'page' | 'board'; n: number } | { by: 1 | -1 }

const CN: Record<string, number> = { 一: 1, 二: 2, 两: 2, 三: 3, 四: 4, 五: 5, 六: 6, 七: 7, 八: 8, 九: 9 }

/** 一到九十九，或者阿拉伯数字 */
function number(s: string): number | null {
  if (/^\d+$/.test(s)) return Number(s)
  const m = /^([一二两三四五六七八九])?(十)?([一二三四五六七八九])?$/.exec(s)
  if (!m || !s) return null
  if (!m[2]) return m[3] ? null : CN[m[1]]
  return (m[1] ? CN[m[1]] : 1) * 10 + (m[3] ? CN[m[3]] : 0)
}

const LEAD = '(?:那|好|嗯|好的)?[，,、 ]*(?:我们|咱们)?(?:先|再)?'
const TAIL = '(?:吧|看看|看一下)?[。！!.，, ]*$'
const NTH = new RegExp(`^${LEAD}(?:翻|跳|切|转|回|去|看)(?:到|去|回)?第?(\\d+|[一二两三四五六七八九十]+)(页|张板|张|块板)${TAIL}`)
const STEP = new RegExp(`^${LEAD}(?:翻|跳|切|转|回|去|看)?(?:到|去)?(下|上)一(?:页|张板|张)${TAIL}`)

export function parseNav(text: string): Nav | null {
  const t = text.trim()
  const step = STEP.exec(t)
  if (step) return { by: step[1] === '下' ? 1 : -1 }
  const m = NTH.exec(t)
  if (!m) return null
  const n = number(m[1])
  return n ? { to: m[2] === '页' ? 'page' : 'board', n } : null
}

/**
 * 翻到哪张板。「第 N 页」优先指导入文件的第 N 页，同一页在好几份文件里时取离现在最近的；
 * 没有导入文件时，页和板是一回事。找不到返回 -1。
 */
export function navTarget(nav: Nav, boards: { no?: number }[], current: number): number {
  if ('by' in nav) {
    const i = current + nav.by
    return i >= 0 && i < boards.length ? i : -1
  }
  if (nav.to === 'page') {
    let best = -1
    boards.forEach((b, i) => {
      if (b.no === nav.n && (best < 0 || Math.abs(i - current) < Math.abs(best - current))) best = i
    })
    if (best >= 0) return best
    // 这份文件里有页码，只是这一页没建板（比如参考文献）
    if (boards[current]?.no !== undefined) return -1
  }
  return nav.n <= boards.length ? nav.n - 1 : -1
}
