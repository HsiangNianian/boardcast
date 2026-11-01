// 区分说话人：外壳每句给一个声纹（单位长度的向量），这里和这场会已经出现过的声音比，判断是谁。
// 在线聚类：和最像的那个声音够像（余弦相似度过 SAME）就算同一个人，并把这句并进那个人的声纹里；
// 都不像就是新的人，按 A、B、C 起名。太短的句子没有声纹，算给上一个说话的人。
// 声纹跟着会议存，用户把「B」改成真名后，同一个声音以后都用这个名字。

export interface Voice {
  name: string
  /** 这个人所有句子声纹的平均方向（单位长度） */
  centroid: number[]
  /** 并进来过几句 */
  n: number
}

/** 同一个人的阈值。CAM++ 上同一个人一般 0.5 以上，不同的人 0.2 以下；会上的句子短，放宽一点 */
export const SAME = 0.45
/** 并进声纹时最多按这么多句加权：人的声音状态会变，老句子的分量不能无限大 */
const MEMORY = 30

const dot = (a: number[], b: number[]) => {
  let s = 0
  for (let i = 0; i < Math.min(a.length, b.length); i++) s += a[i] * b[i]
  return s
}

function unit(v: number[]): number[] {
  const len = Math.sqrt(dot(v, v)) || 1
  // 存进会议文件，四位小数足够
  return v.map((x) => Math.round((x / len) * 1e4) / 1e4)
}

/** 下一个没人用的名字：A、B、C……Z，然后 A2、B2…… */
export function nextName(taken: Iterable<string>): string {
  const used = new Set(taken)
  for (let round = 1; ; round++)
    for (let i = 0; i < 26; i++) {
      const name = String.fromCharCode(65 + i) + (round > 1 ? round : '')
      if (!used.has(name)) return name
    }
}

/** 最像的那个声音和相似度 */
export function closest(voices: Voice[], vp: number[]): { voice?: Voice; score: number } {
  let best: Voice | undefined
  let score = -1
  for (const v of voices) {
    const s = dot(v.centroid, vp)
    if (s > score) {
      score = s
      best = v
    }
  }
  return { voice: best, score }
}

/**
 * 认人：返回这句是谁说的，和更新后的声音表。
 * `taken`：会议里已经在用的名字（手动标的也算），新的人不和它们重名。
 */
export function identify(voices: Voice[], vp: number[], taken: Iterable<string> = []): { name: string; voices: Voice[] } {
  const { voice, score } = closest(voices, vp)
  if (voice && score >= SAME) {
    const w = Math.min(voice.n, MEMORY)
    const merged = unit(voice.centroid.map((x, i) => x * w + (vp[i] ?? 0)))
    return { name: voice.name, voices: voices.map((v) => (v === voice ? { ...v, centroid: merged, n: voice.n + 1 } : v)) }
  }
  const name = nextName([...taken, ...voices.map((v) => v.name)])
  return { name, voices: [...voices, { name, centroid: unit(vp), n: 1 }] }
}

/** 改名：改成一个已有的名字时，两个声音合并（之前被认成了两个人） */
export function renameVoice(voices: Voice[], from: string, to: string): Voice[] {
  const a = voices.find((v) => v.name === from)
  if (!a || from === to) return voices
  const b = voices.find((v) => v.name === to)
  if (!b) return voices.map((v) => (v === a ? { ...v, name: to } : v))
  const merged: Voice = {
    name: to,
    centroid: unit(b.centroid.map((x, i) => x * b.n + (a.centroid[i] ?? 0) * a.n)),
    n: a.n + b.n,
  }
  return voices.filter((v) => v !== a && v !== b).concat(merged)
}
