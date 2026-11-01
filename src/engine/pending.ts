// 模型处理完一批话（或者这批出错了），会议记录里对应的那几句标上结果。
// 会话按先来后到处理：一批就是当时排着队的最早那几句。所以在还在等的句子里，从最早的开始按原话依次对上；
// 不能从最后一句往前数，模型忙的时候后面又来了几句，结果会记到那几句还没轮到的句子上。

interface Pending {
  text: string
  /** undefined：还在等模型 */
  ms?: number
}

/** `batch` 的第 k 句对应的那句会议记录改成 `patch(k)`；对不上的（不该有）跳过，不乱记 */
export function settle<L extends Pending>(lines: L[], batch: { text: string }[], patch: (k: number) => Partial<L>): L[] {
  const out = [...lines]
  let from = 0
  batch.forEach((u, k) => {
    for (let i = from; i < out.length; i++) {
      if (out[i].ms !== undefined || out[i].text !== u.text) continue
      out[i] = { ...out[i], ...patch(k) }
      from = i + 1
      return
    }
  })
  return out
}
