// 最小的 PDF 生成器：每页铺一张 JPEG。导出整场会用——板书、摘要、会议记录都先画到画布上，再一页页装订。
// 只用到 PDF 最基本的几样东西（目录、页、图片对象、内容流、交叉引用表），不依赖任何库。

export interface PdfPage {
  /** JPEG 字节 */
  jpeg: Uint8Array
  /** 图片像素尺寸 */
  width: number
  height: number
}

/** 页面大小（单位：点，1/72 英寸）。默认 A4 横向 */
export const A4_LANDSCAPE = { w: 842, h: 595 }

const enc = new TextEncoder()

export function makePdf(pages: PdfPage[], size = A4_LANDSCAPE, title = 'boardcast'): Uint8Array {
  const chunks: Uint8Array[] = []
  const offsets: number[] = []
  let length = 0
  const push = (b: Uint8Array | string) => {
    const bytes = typeof b === 'string' ? enc.encode(b) : b
    chunks.push(bytes)
    length += bytes.length
  }
  // 对象编号：1 目录，2 页树，3 文档信息；之后每页三个：页、内容流、图片
  const pageObj = (i: number) => 4 + i * 3
  const obj = (n: number, body: () => void) => {
    offsets[n] = length
    push(`${n} 0 obj\n`)
    body()
    push('\nendobj\n')
  }
  // PDF 字符串里的中文用 UTF-16BE 十六进制写
  const utf16 = (s: string) => {
    let hex = 'FEFF'
    for (const ch of s) {
      const c = ch.codePointAt(0)!
      if (c > 0xffff) {
        const v = c - 0x10000
        hex += ((0xd800 + (v >> 10)).toString(16) + (0xdc00 + (v & 0x3ff)).toString(16)).toUpperCase().padStart(8, '0')
      } else hex += c.toString(16).toUpperCase().padStart(4, '0')
    }
    return `<${hex}>`
  }

  push('%PDF-1.4\n%\xe2\xe3\xcf\xd3\n')
  obj(1, () => push('<< /Type /Catalog /Pages 2 0 R >>'))
  obj(2, () => push(`<< /Type /Pages /Count ${pages.length} /Kids [${pages.map((_, i) => `${pageObj(i)} 0 R`).join(' ')}] >>`))
  obj(3, () => push(`<< /Title ${utf16(title)} /Producer (boardcast) >>`))
  pages.forEach((p, i) => {
    const n = pageObj(i)
    // 图片按比例放进页面，居中
    const k = Math.min(size.w / p.width, size.h / p.height)
    const w = p.width * k
    const h = p.height * k
    const x = (size.w - w) / 2
    const y = (size.h - h) / 2
    const content = `q ${w.toFixed(2)} 0 0 ${h.toFixed(2)} ${x.toFixed(2)} ${y.toFixed(2)} cm /Im0 Do Q`
    obj(n, () =>
      push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${size.w} ${size.h}] /Resources << /XObject << /Im0 ${n + 2} 0 R >> >> /Contents ${n + 1} 0 R >>`),
    )
    obj(n + 1, () => push(`<< /Length ${content.length} >>\nstream\n${content}\nendstream`))
    obj(n + 2, () => {
      push(`<< /Type /XObject /Subtype /Image /Width ${p.width} /Height ${p.height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${p.jpeg.length} >>\nstream\n`)
      push(p.jpeg)
      push('\nendstream')
    })
  })
  const count = 4 + pages.length * 3
  const xref = length
  let table = `xref\n0 ${count}\n0000000000 65535 f \n`
  for (let i = 1; i < count; i++) table += `${String(offsets[i]).padStart(10, '0')} 00000 n \n`
  push(table)
  push(`trailer\n<< /Size ${count} /Root 1 0 R /Info 3 0 R >>\nstartxref\n${xref}\n%%EOF\n`)

  const out = new Uint8Array(length)
  let at = 0
  for (const c of chunks) {
    out.set(c, at)
    at += c.length
  }
  return out
}
