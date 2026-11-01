// 圈着说（#48）：在板上用虚线圈出一块，圈里的东西随下一句话带给模型，
// 「把这里改成异步」「这一段标个星」「把这俩连起来」就对得上。
// 只认明明白白画出来的圈：鼠标停一下、点选都不算，免得误触；手机、电子屏上用手指画也行。

/** 一句话最多带几样圈着的东西：再多就不是「这个」「这俩」了，提示词也不该被撑长 */
export const MAX_POINTED = 8

/** 按先后留最近的几个，重复的挪到最后（先圈的在前） */
export function latest(ids: string[], max = MAX_POINTED): string[] {
  const out: string[] = []
  for (const id of ids) {
    const i = out.indexOf(id)
    if (i >= 0) out.splice(i, 1)
    out.push(id)
  }
  return out.slice(-max)
}
