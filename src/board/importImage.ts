// 导入图片（截图、拍的白板、架构图）：一张板，图片垫在底下。
// 位置靠图像处理（segment.ts）找，字交给本地视觉模型一块一块地读——它读字准，但自己给的框偏得厉害。
// 没读出字的块（箭头头、装饰）扔掉。图片只在本机处理，不会发到云端。

import type { Page } from '../meetings.ts'
import { type Area, pageGraph } from '../engine/regions.ts'
import { segment } from './segment.ts'

/** 图片在板上的宽度（板书坐标），和 PDF 页一样 */
const WIDTH = 900
/** 找块时用的分辨率上限：太大慢，太小细字连成一片 */
const WORK = 1800
/** 一张图最多读多少块：每块要问一次模型 */
export const MAX_AREAS = 60

/** 读一小块图里的字；返回空字符串表示没有字 */
export type Reader = (png: string) => Promise<string>

const PROMPT = '图里写的是什么字？原样输出这些字，不要解释，不要加引号。图里没有字就只输出：无'

/** 让本地的 llama-server（带视觉投影文件）读一块图 */
export function llamaReader(baseUrl: string): Reader {
  return async (png) => {
    const res = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        model: 'local',
        temperature: 0,
        max_tokens: 160,
        chat_template_kwargs: { enable_thinking: false },
        messages: [
          {
            role: 'user',
            content: [
              { type: 'image_url', image_url: { url: png } },
              { type: 'text', text: PROMPT },
            ],
          },
        ],
      }),
    })
    if (!res.ok) {
      const body = await res.text()
      if (/image|mmproj|multimodal|vision/i.test(body)) throw new Error('本地模型还不能看图：在「本地模型」里装上「看图」组件')
      throw new Error(`本地模型 ${res.status}：${body.slice(0, 200)}`)
    }
    const j = (await res.json()) as { choices?: { message?: { content?: string } }[] }
    return cleanRead(j.choices?.[0]?.message?.content ?? '')
  }
}

/** 模型读出来的字收拾一下：去掉思考、引号、代码块；「无」「没有文字」之类当成空 */
export function cleanRead(s: string): string {
  const t = s
    .replace(/<think>[\s\S]*?(<\/think>|$)/g, '')
    .replace(/```[a-z]*\n?/gi, '')
    .replace(/^[「“"'『]+|[」”"'』]+$/g, '')
    .replace(/\s+/g, ' ')
    .trim()
  if (/^(无|没有|空|没有文字|图里没有字|（无）|\(无\)|none|n\/a)[。.]?$/i.test(t)) return ''
  return t
}

/** 读出来的是不是字：箭头头、线头常被读成「1>1」「->」之类，至少要有一个汉字、字母，或者三个以上数字 */
export function meaningful(s: string): boolean {
  return /\p{L}/u.test(s) || (s.match(/\d/g) ?? []).length >= 3
}

export interface ImageImport {
  page: Page
  /** 找到几块、读出字的几块 */
  found: number
  read: number
  /** 块太多，只读了前 MAX_AREAS 块 */
  capped: boolean
}

export async function importImage(file: Blob, read: Reader, onStep?: (done: number, total: number) => void): Promise<ImageImport> {
  let bmp: ImageBitmap
  try {
    bmp = await createImageBitmap(file)
  } catch {
    throw new Error('图片打不开（支持 PNG、JPEG、WebP、GIF）')
  }
  const k = Math.min(1, WORK / bmp.width)
  const work = document.createElement('canvas')
  work.width = Math.round(bmp.width * k)
  work.height = Math.round(bmp.height * k)
  const ctx = work.getContext('2d', { willReadFrequently: true })!
  // 透明的地方垫白底，和底图看到的一样
  ctx.fillStyle = '#fff'
  ctx.fillRect(0, 0, work.width, work.height)
  ctx.drawImage(bmp, 0, 0, work.width, work.height)
  bmp.close()

  const all = segment(ctx.getImageData(0, 0, work.width, work.height))
  const areas = all.slice(0, MAX_AREAS)
  const toBoard = WIDTH / work.width
  const found: Area[] = []
  for (const [i, a] of areas.entries()) {
    onStep?.(i, areas.length)
    // 切出这一块，四周留点边；字太小就放大一点，模型读得更准
    const pad = 6
    const x = Math.max(0, a.x - pad)
    const y = Math.max(0, a.y - pad)
    const w = Math.min(work.width - x, a.w + pad * 2)
    const h = Math.min(work.height - y, a.h + pad * 2)
    const up = Math.min(3, Math.max(1, 48 / h))
    const crop = document.createElement('canvas')
    crop.width = Math.round(w * up)
    crop.height = Math.round(h * up)
    crop.getContext('2d')!.drawImage(work, x, y, w, h, 0, 0, crop.width, crop.height)
    const text = await read(crop.toDataURL('image/png'))
    if (meaningful(text)) found.push({ text, x: a.x * toBoard, y: a.y * toBoard, w: a.w * toBoard, h: a.h * toBoard })
  }
  onStep?.(areas.length, areas.length)

  const image = work.toDataURL('image/jpeg', 0.85)
  const graph = pageGraph({ w: WIDTH, h: work.height * toBoard }, found)
  return { page: { graph, underlay: image }, found: areas.length, read: found.length, capped: all.length > MAX_AREAS }
}
