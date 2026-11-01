// 说话人的代表色。会议记录里的头像是浅底（style.css 的 .who.c0–c5），
// 白板上溯源时垫的荧光笔用同一色相、浓一点的，叠在字底下还看得出颜色。

export const SPEAKER_MARKERS = ['#f783ac', '#74c0fc', '#69db7c', '#ffa94d', '#b197fc', '#adb5bd'] as const

/** 第几种颜色：按说话人在这场会里出现的先后 */
export const speakerSlot = (speakers: string[], who: string) => Math.max(0, speakers.indexOf(who)) % SPEAKER_MARKERS.length

/**
 * 头像上的一个字。中文名取姓（李工 → 李），「小王」「老张」这种取后面那个字；带编号的取编号（说话人 2 → 2）；
 * 别的取第一个字母（Alice → A）。全名写在旁边，头像只要认得出、配上颜色就够了。
 */
export function initial(name: string): string {
  const s = name.trim()
  const num = /\d+$/.exec(s)
  if (num) return num[0].slice(-2)
  const chars = [...s]
  if (chars.length && /\p{Script=Han}/u.test(chars[0])) return chars.length > 1 && '小老阿'.includes(chars[0]) ? chars[1] : chars[0]
  return (chars.find((c) => /[\p{L}\p{N}]/u.test(c)) ?? '?').toUpperCase()
}
