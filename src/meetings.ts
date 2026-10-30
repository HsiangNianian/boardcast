// 会议的保存和读取。
// - 一场会议存成一个 JSON：板书结构、给模型看的上下文、会议记录、排版记住的位置；
// - 桌面版由外壳存在应用数据目录的 meetings/ 下；网页版存在浏览器的 IndexedDB；
// - 带版本号，以后改数据结构时可以迁移。

import type { Step } from './engine/steps.ts'
import type { Voice } from './engine/voices.ts'
import type { LayoutMemory } from './board/layout.ts'
import { isDesktop } from './desktop.ts'
import type { Graph } from './engine/graph.ts'
import type { Utterance } from './engine/prompt.ts'

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
  return '未命名会议'
}

/** 读进来的会议：补齐缺的字段；上次关闭时还没画完的句子，标成没来得及画 */
export function normalize(m: Meeting): Meeting {
  return {
    ...m,
    version: m.version ?? MEETING_VERSION,
    history: m.history ?? [],
    lines: (m.lines ?? []).map((l) => (l.ms === undefined ? { ...l, ms: 0, ops: '（应用关闭时这句还没画完）' } : l)),
  }
}

export function info(m: Meeting): MeetingInfo {
  return { id: m.id, title: m.title, createdAt: m.createdAt, updatedAt: m.updatedAt, lines: m.lines.filter((l) => !l.manual).length }
}

// —— 网页版：IndexedDB ——

const DB = 'boardcast'
const STORE = 'meetings'

function db(): Promise<IDBDatabase> {
  return new Promise((ok, fail) => {
    const req = indexedDB.open(DB, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: 'id' })
    req.onsuccess = () => ok(req.result)
    req.onerror = () => fail(req.error)
  })
}

async function tx<T>(mode: IDBTransactionMode, run: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const d = await db()
  return new Promise((ok, fail) => {
    const req = run(d.transaction(STORE, mode).objectStore(STORE))
    req.onsuccess = () => ok(req.result)
    req.onerror = () => fail(req.error)
  })
}

// —— 桌面版：外壳存文件 ——

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

export const meetings = {
  /** 最近改过的在前 */
  async list(): Promise<MeetingInfo[]> {
    const all = isDesktop ? await call<MeetingInfo[]>('meetings_list') : (await tx('readonly', (s) => s.getAll())).map(info)
    return all.sort((a, b) => b.updatedAt - a.updatedAt)
  },
  async load(id: string): Promise<Meeting | null> {
    const raw = isDesktop
      ? await call<string | null>('meeting_load', { id }).then((t) => (t ? (JSON.parse(t) as Meeting) : null))
      : ((await tx('readonly', (s) => s.get(id))) as Meeting | undefined) ?? null
    return raw ? normalize(raw) : null
  },
  async save(m: Meeting): Promise<void> {
    if (isDesktop) await call('meeting_save', { id: m.id, json: JSON.stringify(m) })
    else await tx('readwrite', (s) => s.put(m))
  },
  async remove(id: string): Promise<void> {
    if (isDesktop) await call('meeting_delete', { id })
    else await tx('readwrite', (s) => s.delete(id))
  },
}
