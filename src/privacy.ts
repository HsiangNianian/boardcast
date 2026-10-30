// 隐私：用远程模型时，会上说的话会发到别人的服务器上。启用前要用户确认过一次（按地址记），
// 界面上要一直看得出来现在是「云端」还是「本地」。填的是本机地址（LM Studio、本地 vLLM）不算云端。

/** 本机地址：数据不出这台电脑 */
export function isLoopback(url: string): boolean {
  try {
    const h = new URL(url.trim()).hostname.replace(/^\[|\]$/g, '')
    return h === 'localhost' || h.endsWith('.localhost') || /^127\./.test(h) || h === '::1' || h === '0.0.0.0'
  } catch {
    return false
  }
}

/** 按主机名记确认过的地址（换了路径、模型不用再问，换了服务商要问） */
export function hostOf(url: string): string {
  try {
    return new URL(url.trim()).host.toLowerCase()
  } catch {
    return url.trim().toLowerCase()
  }
}

const KEY = 'boardcast.cloud-ok'

function read(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '[]')
    return Array.isArray(v) ? v.filter((x) => typeof x === 'string') : []
  } catch {
    return []
  }
}

/** 这个地址要不要先确认 */
export function needsConsent(url: string): boolean {
  return !isLoopback(url) && !read().includes(hostOf(url))
}

export function rememberConsent(url: string) {
  try {
    const all = new Set(read())
    all.add(hostOf(url))
    localStorage.setItem(KEY, JSON.stringify([...all]))
  } catch {
    // 存不下就下次再问一遍，不影响使用
  }
}

/** 启用前给用户看的：会发出去什么 */
export const SENT = [
  '会上说的每一句话（语音识别出来的文字，或者打的字）和说话人的名字',
  '当前白板上的内容：话题、条目、流程图的节点和连线的文字',
  '点「摘要」时，整场会的会议记录',
]

/** 不会发出去的 */
export const KEPT = ['声音本身：语音在这台电脑上识别成文字，录音不会上传', '会议文件：保存在这台电脑上']
