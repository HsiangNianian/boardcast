// 会议的保存和读取。
// - 一场会议存成一个 JSON：板书结构、给模型看的上下文、会议记录、排版记住的位置；
// - 桌面版由外壳存在应用数据目录的 meetings/ 下；网页版存在浏览器的 IndexedDB；
// - 带版本号，以后改数据结构时可以迁移。

import type { Thumb } from './board/thumb.ts'
import type { Step } from './engine/steps.ts'
import type { Voice } from './engine/voices.ts'
import type { LayoutMemory } from './board/layout.ts'
import { type HeardTiming, isDesktop } from './desktop.ts'
import type { Graph } from './engine/graph.ts'
import type { Utterance } from './engine/prompt.ts'
import { type Hit, type Searchable, searchMeeting } from './engine/search.ts'
import { msg } from './i18n/index.ts'

export const MEETING_VERSION = 1

/** 会议记录里的一句话（和界面上显示的一致） */
export interface MeetingLine extends Utterance {
  /** 模型处理这句话用了多久；undefined 表示还在排队 */
  ms?: number
  ops?: string
  via?: string
  fellBack?: boolean
  /** 手写板上手动改的（删掉、改字、标记），不是会上说的话 */
  manual?: boolean
  /** 这句画在第几张板上（从 0 数） */
  page?: number
  /** Spoken lines: time spent in the shell (pause, recognition, voiceprint) */
  heard?: HeardTiming
  /** Time from reaching the app until the model started on it (it was busy with earlier lines) */
  queueMs?: number
}

export interface Meeting {
  version: number
  id: string
  title: string
  /** 用户自己改过标题：以后不再按内容自动起名 */
  titled?: boolean
  createdAt: number
  updatedAt: number
  graph: Graph
  /** 给模型看的前文 */
  history: Utterance[]
  lines: MeetingLine[]
  layout?: LayoutMemory
  /** 会议摘要（Markdown），生成后可以手改 */
  summary?: string
  /** 这场会里认出来的声音（区分说话人用） */
  voices?: Voice[]
  /** 墨线板书的每一次变化（回放用） */
  steps?: Step[]
  /** 这场会只用本地模型，不发到远程接口 */
  localOnly?: boolean
  /** 这场会自己的术语（和全局的合在一起用），一行一条 */
  glossary?: string
  /** 有几张板时：每张板的内容（当前这张和上面的 graph / layout / steps 一样），和现在在第几张 */
  pages?: Page[]
  page?: number
}

/** 一张板 */
export interface Page {
  graph: Graph
  layout?: LayoutMemory
  steps?: Step[]
  thumb?: Thumb
  /** 垫在底下的导入页（PDF 的一页、一张图片），data URL */
  underlay?: string
  /** 带着旧板开会（接着某场会的某张板开的新会）：开会时那张板的样子。和它一样的地方用淡墨画 */
  base?: Graph
}

/** 历史列表里显示的样子 */
export interface MeetingInfo {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  lines: number
}

const pad = (n: number) => String(n).padStart(2, '0')

/** 编号：时间 + 一小段随机，只用字母数字和横线（桌面版直接拿来当文件名） */
export function newMeetingId(now = new Date()): string {
  const t = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`
  return `${t}-${Math.random().toString(36).slice(2, 6)}`
}

/** 没改过名的会议，按内容起一个：第一个话题，其次第一个流程节点，再其次第一句话 */
export function autoTitle(graph: Graph, lines: MeetingLine[]): string {
  const topic = graph.nodes.find((n) => n.kind === 'topic' && n.ink === 'ink')
  if (topic) return topic.label
  const node = graph.nodes.find((n) => n.ink === 'ink')
  if (node) return node.label
  const first = lines[0]?.text.trim()
  if (first) return first.length > 20 ? `${first.slice(0, 20)}…` : first
  return msg().app.untitled
}

/** 读进来的会议：补齐缺的字段；上次关闭时还没画完的句子，标成没来得及画 */
export function normalize(m: Meeting): Meeting {
  return {
    ...m,
    version: m.version ?? MEETING_VERSION,
    history: m.history ?? [],
    lines: (m.lines ?? []).map((l) => (l.ms === undefined ? { ...l, ms: 0, ops: msg().doc.unfinished } : l)),
  }
}

export function info(m: Meeting): MeetingInfo {
  return { id: m.id, title: m.title, createdAt: m.createdAt, updatedAt: m.updatedAt, lines: m.lines.filter((l) => !l.manual).length }
}

// —— 导入页的图片单独存：会议 JSON 里只记 `asset:<内容哈希>.<扩展名>` ——
// 不然几十页的 PDF 能让一场会到好几 MB，每次自动保存都要整份重写。

export interface AssetStore {
  put(meeting: string, key: string, dataUrl: string): Promise<void>
  get(meeting: string, key: string): Promise<string | null>
}

const REF = 'asset:'
const EXT: Record<string, string> = { 'image/jpeg': 'jpg', 'image/png': 'png', 'image/webp': 'webp' }
const MIME: Record<string, string> = { jpg: 'image/jpeg', png: 'image/png', webp: 'image/webp' }

/** 同一张图（同一个字符串）只算一次哈希 */
const hashed = new Map<string, string>()
async function keyOf(dataUrl: string): Promise<string | null> {
  const ext = EXT[/^data:([^;,]+)/.exec(dataUrl)?.[1] ?? '']
  if (!ext) return null
  const known = hashed.get(dataUrl)
  if (known) return known
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(dataUrl))
  const hex = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('').slice(0, 32)
  const key = `${hex}.${ext}`
  hashed.set(dataUrl, key)
  return key
}

/** 存之前：把每张板上的图换成引用，没存过的图先存起来。`stored` 记着这场会已经存过哪些，下次不再写 */
export async function stashAssets(m: Meeting, store: AssetStore, stored: Set<string>): Promise<Meeting> {
  if (!m.pages?.some((p) => p.underlay?.startsWith('data:'))) return m
  const pages = await Promise.all(
    m.pages.map(async (p) => {
      if (!p.underlay?.startsWith('data:')) return p
      const key = await keyOf(p.underlay)
      if (!key) return p
      if (!stored.has(`${m.id}/${key}`)) {
        await store.put(m.id, key, p.underlay)
        stored.add(`${m.id}/${key}`)
      }
      return { ...p, underlay: REF + key }
    }),
  )
  return { ...m, pages }
}

/** 读出来以后：引用换回图。图找不到了就去掉底图，板书照常能看 */
export async function loadAssets(m: Meeting, store: AssetStore, stored?: Set<string>): Promise<Meeting> {
  if (!m.pages?.some((p) => p.underlay?.startsWith(REF))) return m
  const pages = await Promise.all(
    m.pages.map(async (p) => {
      if (!p.underlay?.startsWith(REF)) return p
      const key = p.underlay.slice(REF.length)
      const data = await store.get(m.id, key).catch(() => null)
      if (!data) return { ...p, underlay: undefined }
      stored?.add(`${m.id}/${key}`)
      hashed.set(data, key)
      return { ...p, underlay: data }
    }),
  )
  return { ...m, pages }
}

// —— 网页版：IndexedDB ——

const DB = 'boardcast'
const STORE = 'meetings'
const ASSETS = 'assets'

function db(): Promise<IDBDatabase> {
  return new Promise((ok, fail) => {
    const req = indexedDB.open(DB, 2)
    req.onupgradeneeded = () => {
      const d = req.result
      if (!d.objectStoreNames.contains(STORE)) d.createObjectStore(STORE, { keyPath: 'id' })
      // 第 2 版：导入页的图片，键是「会议编号/图片名」，值是 data URL
      if (!d.objectStoreNames.contains(ASSETS)) d.createObjectStore(ASSETS)
    }
    req.onsuccess = () => ok(req.result)
    req.onerror = () => fail(req.error)
  })
}

async function tx<T>(mode: IDBTransactionMode, run: (s: IDBObjectStore) => IDBRequest<T>, store = STORE): Promise<T> {
  const d = await db()
  return new Promise((ok, fail) => {
    const req = run(d.transaction(store, mode).objectStore(store))
    req.onsuccess = () => ok(req.result)
    req.onerror = () => fail(req.error)
  })
}

// —— 桌面版：外壳存文件 ——

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

const assets: AssetStore = {
  async put(meeting, key, dataUrl) {
    if (isDesktop) await call('meeting_asset_save', { id: meeting, key, data: dataUrl.slice(dataUrl.indexOf(',') + 1) })
    else await tx('readwrite', (s) => s.put(dataUrl, `${meeting}/${key}`), ASSETS)
  },
  async get(meeting, key) {
    if (!isDesktop) return ((await tx('readonly', (s) => s.get(`${meeting}/${key}`), ASSETS)) as string | undefined) ?? null
    const b64 = await call<string | null>('meeting_asset_load', { id: meeting, key })
    return b64 ? `data:${MIME[key.split('.').pop()!] ?? 'image/jpeg'};base64,${b64}` : null
  },
}
/** 已经存过的图（「会议编号/图片名」），自动保存时跳过 */
const stored = new Set<string>()
/** 搜索用：读过的会议按更新时间缓存，边打字边搜不用每次把文件读一遍 */
const searchCache = new Map<string, { updatedAt: number; m: Searchable }>()

export const meetings = {
  /** 最近改过的在前 */
  async list(): Promise<MeetingInfo[]> {
    const all = isDesktop ? await call<MeetingInfo[]>('meetings_list') : (await tx('readonly', (s) => s.getAll())).map(info)
    return all.sort((a, b) => b.updatedAt - a.updatedAt)
  },
  /** 全文搜索：命中的会议和命中处，顺序同 list() */
  async search(query: string): Promise<{ info: MeetingInfo; hits: Hit[] }[]> {
    const out: { info: MeetingInfo; hits: Hit[] }[] = []
    for (const info of await this.list()) {
      let m = searchCache.get(info.id)
      if (m?.updatedAt !== info.updatedAt) {
        // 不读图：搜索用不着
        const raw = isDesktop
          ? await call<string | null>('meeting_load', { id: info.id }).then((t) => (t ? (JSON.parse(t) as Meeting) : null))
          : ((await tx('readonly', (s) => s.get(info.id))) as Meeting | undefined) ?? null
        if (!raw) continue
        m = { updatedAt: info.updatedAt, m: raw }
        searchCache.set(info.id, m)
      }
      const hits = searchMeeting(m.m, query)
      if (hits) out.push({ info, hits })
    }
    return out
  },
  async load(id: string): Promise<Meeting | null> {
    const raw = isDesktop
      ? await call<string | null>('meeting_load', { id }).then((t) => (t ? (JSON.parse(t) as Meeting) : null))
      : ((await tx('readonly', (s) => s.get(id))) as Meeting | undefined) ?? null
    return raw ? normalize(await loadAssets(raw, assets, stored)) : null
  },
  async save(meeting: Meeting): Promise<void> {
    const m = await stashAssets(meeting, assets, stored)
    if (isDesktop) await call('meeting_save', { id: m.id, json: JSON.stringify(m) })
    else await tx('readwrite', (s) => s.put(m))
  },
  async remove(id: string): Promise<void> {
    if (isDesktop) await call('meeting_delete', { id })
    else {
      await tx('readwrite', (s) => s.delete(id))
      await tx('readwrite', (s) => s.delete(IDBKeyRange.bound(`${id}/`, `${id}/\uffff`)), ASSETS)
    }
    for (const k of [...stored]) if (k.startsWith(`${id}/`)) stored.delete(k)
  },
}
