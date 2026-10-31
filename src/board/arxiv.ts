// 从用户贴进来的东西里认出 arXiv 编号：链接（abs / pdf / html，带不带版本号、.pdf 后缀）、「arXiv:编号」、光秃秃的编号

const NEW = /(\d{4}\.\d{4,5}(?:v\d+)?)/
const OLD = /([a-z][a-z-]*(?:\.[A-Z]{2})?\/\d{7}(?:v\d+)?)/

export function parseArxiv(input: string): string | null {
  const s = input.trim()
  if (!s) return null
  const url = /arxiv\.org\/(?:abs|pdf|html|format)\/([^?#\s]+)/i.exec(s)
  const body = (url ? url[1] : s.replace(/^arxiv:\s*/i, '')).replace(/\.pdf$/i, '').replace(/\/$/, '')
  const m = new RegExp(`^(?:${NEW.source}|${OLD.source})$`).exec(body)
  return m ? (m[1] ?? m[2]) : null
}
