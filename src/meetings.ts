// 会议的保存和读取。
// - 一场会议存成一个 JSON：板书结构、给模型看的上下文、会议记录、排版记住的位置；
// - 桌面版由外壳存在应用数据目录的 meetings/ 下；网页版存在浏览器的 IndexedDB；
// - 带版本号，以后改数据结构时可以迁移。

import type { Thumb } from './board/thumb.ts'
import type { Correction } from './engine/corrections.ts'
import type { Step } from './engine/steps.ts'
import type { Voice } from './engine/voices.ts'
import type { LayoutMemory } from './board/layout.ts'
import { type Heard, type HeardTiming, isDesktop } from './desktop.ts'
import type { Graph } from './engine/graph.ts'
import type { Utterance } from './engine/prompt.ts'
import { type Hit, type Searchable, searchMeeting } from './engine/search.ts'
import { currentLang, msg } from './i18n/index.ts'
import { MeetingWrites } from './engine/meetingSave.ts'
import { type FreeScene, validateScene } from './board/freeScene.ts'
import { type AssetIssue, type ImageDecoder, AssetIntegrityError, assetKey, assetReadIssue, decodeAssetImage, IMAGE_MIME, verifyAsset } from './engine/assetIntegrity.ts'

export const MEETING_VERSION = 2

/** 会议记录里的一句话（和界面上显示的一致） */
export interface MeetingLine extends Utterance {
  /** 模型处理这句话用了多久；undefined 表示还在排队 */
  ms?: number
  ops?: string
  via?: string
  fellBack?: boolean
  /** 手写板上手动改的（删掉、改字、标记），不是会上说的话 */
  manual?: boolean
  /** 这次手动修改改的是模型刚画的，那一轮记成了纠错样本 */
  fix?: boolean
  /** 这句画在第几张板上（从 0 数） */
  page?: number
  /** Spoken lines: time spent in the shell (pause, recognition, voiceprint) */
  heard?: HeardTiming
  audio?: Pick<Heard, 'start' | 'end' | 'sources' | 'quality' | 'review'>
  /** Time from reaching the app until the model started on it (it was busy with earlier lines) */
  queueMs?: number
  /** 模型这一轮出错了，这句没画上 */
  failed?: boolean
  boardFailure?: 'unavailable' | 'request' | 'cancelled'
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
  /** The meeting-content snapshot behind the current summary, for stale-content feedback. */
  summarySource?: string
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
  /** 模型刚画完就被手动改了的那几轮（纠错样本，可以导出成评测用例、训练数据） */
  corrections?: Correction[]
  scene?: FreeScene
  /** Hash of the preserved v1 original. A changed original requires recovery, never overwrite. */
  migration?: { sourceHash: string }
  missingAssets?: string[]
  /** Load-time diagnostics; never committed as meeting content. */
  assetIssues?: AssetIssue[]
  storageRevision?: number
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
  scene?: FreeScene
}

/** 历史列表里显示的样子 */
export interface MeetingInfo {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  lines: number
  pages?: number
  unfinished?: number
  readError?: string
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
    version: m.version ?? 1,
    history: m.history ?? [],
    lines: (m.lines ?? []).map((l) => (l.ms === undefined ? { ...l, ms: 0, failed: true, boardFailure: 'cancelled', ops: msg().doc.unfinished } : l)),
  }
}

export function info(m: Meeting): MeetingInfo {
  return { id: m.id, title: m.title, createdAt: m.createdAt, updatedAt: m.updatedAt,
    lines: m.lines.filter((l) => !l.manual).length, pages: m.pages?.length || 1,
    unfinished: m.lines.filter((l) => !l.manual && (l.failed || l.ms === undefined)).length }
}

// —— 导入页的图片单独存：会议 JSON 里只记 `asset:<内容哈希>.<扩展名>` ——
// 不然几十页的 PDF 能让一场会到好几 MB，每次自动保存都要整份重写。

export interface AssetStore {
  put(meeting: string, key: string, dataUrl: string): Promise<void>
  get(meeting: string, key: string): Promise<string | null>
}

const REF = 'asset:'

/** Validate current disk contents even after a successful load; never overwrite a damaged original. */
export async function stashAssets(m: Meeting, store: AssetStore, stored: Set<string>, decode: ImageDecoder = decodeAssetImage): Promise<Meeting> {
  // This is a load-time diagnostic, not an enduring restriction: restored files
  // must make the same in-memory meeting saveable without reopening it.
  const { missingAssets: _missingAssets, assetIssues: _assetIssues, ...snapshot } = m
  const read = async (key: string) => {
    try { return await store.get(m.id, key) }
    catch (error) { throw new AssetIntegrityError(assetReadIssue(key, error)) }
  }
  return mapAssets(snapshot, async (url) => {
      if (url.startsWith(REF)) {
        const key = url.slice(REF.length), data = await read(key)
        if (!data) throw new AssetIntegrityError({ key, reason: 'missing' })
        await verifyAsset(data, key, decode)
        return url
      }
      if (!url.startsWith('data:')) return url
      const key = await assetKey(url)
      if (!key) return url
      await verifyAsset(url, key, decode)
      const existing = await read(key)
      if (existing) await verifyAsset(existing, key, decode)
      else {
        await store.put(m.id, key, url)
      }
      stored.add(`${m.id}/${key}`)
      return REF + key
  })
}

/** Visit only resource fields, including historical scenes. Never rewrite arbitrary user text. */
async function mapAssets(m: Meeting, map: (url: string) => Promise<string>): Promise<Meeting> {
  const convert = map
  const pending = new Map<string, Promise<string>>()
  map = (url) => {
    if (!pending.has(url)) pending.set(url, convert(url))
    return pending.get(url)!
  }
  const scene = async (s: FreeScene | null | undefined): Promise<FreeScene | null | undefined> => {
    if (!s) return s
    const files = Object.fromEntries(await Promise.all(Object.entries(s.files).map(async ([id, file]) => [id, { ...file, dataURL: await map(file.dataURL) }]))) as FreeScene['files']
    return { ...s, files, overlay: s.overlay ? { ...s.overlay, url: await map(s.overlay.url) } : undefined }
  }
  const steps = async (ss: Step[] | undefined) => ss && Promise.all(ss.map(async (s) => ({ ...s, ...(s.scene !== undefined ? { scene: await scene(s.scene) } : {}) })))
  if (!m.pages?.length && !m.scene && !m.steps?.some((s) => s.scene)) return m
  return {
    ...m, scene: (await scene(m.scene)) ?? undefined, steps: await steps(m.steps),
    pages: m.pages && await Promise.all(m.pages.map(async (p) => ({ ...p, underlay: p.underlay ? await map(p.underlay) : undefined, scene: (await scene(p.scene)) ?? undefined, steps: await steps(p.steps) }))),
  }
}

/** Missing assets retain their reference so opening and saving cannot erase recovery information. */
export async function loadAssets(m: Meeting, store: AssetStore, stored?: Set<string>, decode: ImageDecoder = decodeAssetImage): Promise<Meeting> {
  if (m.missingAssets || m.assetIssues) {
    const { missingAssets: _missingAssets, assetIssues: _assetIssues, ...snapshot } = m
    m = snapshot
  }
  if (!JSON.stringify(m).includes('asset:')) return m
  const issues: AssetIssue[] = []
  const result = await mapAssets(m, async (url) => {
      if (!url.startsWith(REF)) return url
      const key = url.slice(REF.length)
      try {
        const data = await store.get(m.id, key)
        if (!data) throw new AssetIntegrityError({ key, reason: 'missing' })
        await verifyAsset(data, key, decode)
        stored?.add(`${m.id}/${key}`)
        return data
      } catch (error) {
        stored?.delete(`${m.id}/${key}`)
        issues.push(assetReadIssue(key, error))
        return url
      }
  })
  return issues.length ? { ...result, missingAssets: issues.map((issue) => issue.key), assetIssues: issues } : result
}

// —— 网页版：IndexedDB ——

const DB = 'boardcast-v2'
const STORE = 'meetings'
const ASSETS = 'assets'
const deletedKey = (id: string) => `deleted:${id}`

function db(legacy = false): Promise<IDBDatabase> {
  return new Promise((ok, fail) => {
    const req = indexedDB.open(legacy ? 'boardcast' : DB, legacy ? 2 : 1)
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

async function tx<T>(mode: IDBTransactionMode, run: (s: IDBObjectStore) => IDBRequest<T>, stores = [STORE], legacy = false): Promise<T> {
  const d = await db(legacy)
  return new Promise((ok, fail) => {
    const transaction = d.transaction(stores, mode)
    let req: IDBRequest<T>
    try { req = run(transaction.objectStore(stores[0])) }
    catch (error) { transaction.abort(); d.close(); fail(error); return }
    transaction.oncomplete = () => { d.close(); ok(req.result) }
    transaction.onabort = transaction.onerror = () => { d.close(); fail(transaction.error ?? req.error ?? new Error(msg().errors.meetingTransaction)) }
  })
}

// —— 桌面版：外壳存文件 ——

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  try { return await invoke<T>(cmd, args) }
  catch (error) {
    // shortcut: match fixed Rust messages exactly; update together if the native error contract changes.
    switch (error instanceof Error ? error.message : error) {
      case 'This meeting has been deleted. Your current edits are still here.': throw new Error(msg().errors.meetingDeletedEdits)
      case 'This meeting changed in another window. Reopen it before saving; current edits are preserved.': throw new Error(msg().errors.meetingChanged)
      case 'Cannot overwrite a newer meeting format': throw new Error(msg().errors.meetingNewerSave)
      default: throw error
    }
  }
}

const assetsFor = (legacy = false): AssetStore => ({
  async put(meeting, key, dataUrl) {
    if (isDesktop) await call('meeting_asset_save', { id: meeting, key, data: dataUrl.slice(dataUrl.indexOf(',') + 1), storageVersion: legacy ? 1 : 2 })
    else {
      let deleted = false
      await tx('readwrite', (s) => {
        const request = s.get(deletedKey(meeting))
        request.onsuccess = () => {
          deleted = request.result === true
          if (deleted) s.transaction.abort()
          else s.put(dataUrl, `${meeting}/${key}`)
        }
        return request
      }, [ASSETS], legacy).catch((error) => { throw deleted ? new Error(msg().errors.meetingDeletedEdits) : error })
    }
  },
  async get(meeting, key) {
    if (!isDesktop) return ((await tx('readonly', (s) => s.get(`${meeting}/${key}`), [ASSETS], legacy)) as string | undefined) ?? null
    const b64 = await call<string | null>('meeting_asset_load', { id: meeting, key, storageVersion: legacy ? 1 : 2 })
    return b64 !== null ? `data:${IMAGE_MIME[key.split('.').pop()!] ?? 'image/jpeg'};base64,${b64}` : null
  },
})
const assets = assetsFor()
/** 已经存过的图（「会议编号/图片名」），自动保存时跳过 */
const stored = new Set<string>()
/** 搜索用：读过的会议按更新时间缓存，边打字边搜不用每次把文件读一遍 */
const searchCache = new Map<string, { updatedAt: number; m: Searchable }>()
const writes = new MeetingWrites()
const deletingOrDeleted = new Set<string>()
const origins = new Map<string, Meeting['migration']>()
const revisions = new Map<string, number>()
const LEGACY_COPY = 'legacy:'

async function readRaw(id: string, legacy = false): Promise<Meeting | null> {
  return isDesktop
    ? call<string | null>('meeting_load', { id, storageVersion: legacy ? 1 : 2 }).then((t) => t ? JSON.parse(t) as Meeting : null)
    : ((await tx('readonly', (s) => s.get(id), [STORE], legacy)) as Meeting | undefined) ?? null
}

export async function sourceHash(m: Meeting): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(JSON.stringify(m)))
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

export function validateMeetingScenes(m: Meeting): void {
  for (const page of [m, ...(m.pages ?? [])]) {
    if (page.scene !== undefined) validateScene(page.scene)
    for (const step of page.steps ?? []) if (step.scene) validateScene(step.scene)
  }
}

export function nextStorageRevision(existing: Meeting | undefined | null, expected: number): number {
  if ((existing?.storageRevision ?? 0) !== expected) throw new Error(msg().errors.meetingChanged)
  if (existing && existing.version > MEETING_VERSION) throw new Error(msg().errors.meetingNewerSave)
  return expected + 1
}

async function compareAndPut(m: Meeting, expected: number): Promise<void> {
  const d = await db()
  return new Promise((ok, fail) => {
    const transaction = d.transaction([STORE, ASSETS], 'readwrite')
    const store = transaction.objectStore(STORE)
    const request = transaction.objectStore(ASSETS).get(deletedKey(m.id))
    let conflict: unknown
    request.onsuccess = () => {
      if (request.result === true) {
        conflict = new Error(msg().errors.meetingDeletedEdits)
        transaction.abort()
        return
      }
      const existing = store.get(m.id)
      existing.onsuccess = () => {
        try { nextStorageRevision(existing.result, expected); store.put(m) }
        catch (error) { conflict = error; transaction.abort() }
      }
    }
    transaction.oncomplete = () => { d.close(); ok() }
    transaction.onabort = transaction.onerror = () => { d.close(); fail(conflict ?? transaction.error ?? request.error) }
  })
}

export const meetings = {
  /** 最近改过的在前 */
  async list(strict = false): Promise<MeetingInfo[]> {
    const read = async (legacy: boolean): Promise<MeetingInfo[]> => isDesktop ? call('meetings_list', { storageVersion: legacy ? 1 : 2, strict }) : (await tx<Meeting[]>('readonly', (s) => s.getAll(), [STORE], legacy)).map(info)
    const [old, current] = await Promise.all([read(true), read(false)])
    const all = [...new Map([...old.map((m) => m.readError ? { ...m, id: LEGACY_COPY + m.id } : m), ...current].map((m) => [m.id, m])).values()]
    // Keep a readable v1 recoverable when v2 is damaged or the older app edited v1.
    for (const legacy of old) {
      const existing = current.find((m) => m.id === legacy.id)
      if (legacy.readError || !existing) continue
      const saved = existing.readError ? null : await readRaw(legacy.id)
      const original = saved?.migration ? await readRaw(legacy.id, true) : null
      if (existing.readError || (saved?.migration && original && await sourceHash(original) !== saved.migration.sourceHash)) {
        all.push({ ...legacy, id: LEGACY_COPY + legacy.id, title: `${legacy.title} (${currentLang() === 'zh' ? '旧版本副本' : 'older version copy'})` })
      }
    }
    return all.sort((a, b) => b.updatedAt - a.updatedAt)
  },
  /** 全文搜索：命中的会议和命中处，顺序同 list() */
  async search(query: string): Promise<{ info: MeetingInfo; hits: Hit[] }[]> {
    const out: { info: MeetingInfo; hits: Hit[] }[] = []
    for (const info of await this.list()) {
      if (info.readError) continue
      let m = searchCache.get(info.id)
      if (m?.updatedAt !== info.updatedAt) {
        // 不读图：搜索用不着
        const raw = await this.raw(info.id)
        if (!raw) continue
        m = { updatedAt: info.updatedAt, m: raw }
        searchCache.set(info.id, m)
      }
      const hits = searchMeeting(m.m, query)
      if (hits) out.push({ info, hits })
    }
    return out
  },
  /** 存着的样子，不读图 */
  async raw(id: string): Promise<Meeting | null> {
    if (id.startsWith(LEGACY_COPY)) return readRaw(id.slice(LEGACY_COPY.length), true)
    return await readRaw(id) ?? readRaw(id, true)
  },
  async load(id: string): Promise<Meeting | null> {
    if (id.startsWith(LEGACY_COPY)) {
      const original = await readRaw(id.slice(LEGACY_COPY.length), true)
      if (!original) return null
      if (original.version > MEETING_VERSION) throw new Error(msg().errors.meetingNewerOpen)
      validateMeetingScenes(original)
      return normalize({ ...await loadAssets(original, assetsFor(true)), id: newMeetingId(), title: `${original.title} (${currentLang() === 'zh' ? '旧版本副本' : 'older version copy'})`, migration: undefined, storageRevision: undefined })
    }
    const current = await readRaw(id)
    const raw = current ?? await readRaw(id, true)
    if (!raw) return null
    if (raw.version > MEETING_VERSION) throw new Error(msg().errors.meetingNewerOpen)
    validateMeetingScenes(raw)
    const migration = current ? current.migration : { sourceHash: await sourceHash(raw) }
    origins.set(id, migration)
    revisions.set(id, current?.storageRevision ?? 0)
    return normalize({ ...await loadAssets(raw, assetsFor(!current), current ? stored : undefined), migration })
  },
  /** 所有会议里攒下的纠错样本（最近改过的会议在前） */
  async corrections(): Promise<{ id: string; corrections: Correction[] }[]> {
    const out: { id: string; corrections: Correction[] }[] = []
    for (const info of await this.list(true)) {
      const m = await this.raw(info.id)
      if (m?.corrections?.length) out.push({ id: info.id, corrections: m.corrections })
    }
    return out
  },
  async save(meeting: Meeting): Promise<void> {
    if (deletingOrDeleted.has(meeting.id)) throw new Error(msg().errors.meetingDeleted)
    if (meeting.version > MEETING_VERSION) throw new Error(msg().errors.meetingNewerSave)
    validateMeetingScenes(meeting)
    const snapshot = structuredClone(meeting)
    return writes.run(meeting.id, async () => {
      const existing = await readRaw(meeting.id)
      const expectedRevision = revisions.get(meeting.id) ?? snapshot.storageRevision ?? 0
      const storageRevision = nextStorageRevision(existing, expectedRevision)
      const migration = origins.get(meeting.id) ?? snapshot.migration ?? existing?.migration
      if (migration && !existing) {
        const original = await readRaw(meeting.id, true)
        if (!original) throw new Error(msg().errors.meetingDeletedEdits)
        if (await sourceHash(original) !== migration.sourceHash) throw new Error(msg().errors.meetingLegacyChanged)
      }
      const m = await stashAssets({ ...snapshot, version: MEETING_VERSION, migration, storageRevision }, assets, stored)
      if (isDesktop) await call('meeting_save', { id: m.id, json: JSON.stringify(m), storageVersion: 2, expectedRevision })
      else await compareAndPut(m, expectedRevision)
      revisions.set(m.id, storageRevision)
      searchCache.delete(m.id)
    })
  },
  async remove(id: string): Promise<void> {
    deletingOrDeleted.add(id)
    return writes.run(id, async () => {
      const legacyCopy = id.startsWith(LEGACY_COPY)
      const storedId = legacyCopy ? id.slice(LEGACY_COPY.length) : id
      if (isDesktop) {
        if (!legacyCopy) await call('meeting_delete', { id: storedId, storageVersion: 2 })
        await call('meeting_delete', { id: storedId, storageVersion: 1 })
      }
      else {
        for (const legacy of legacyCopy ? [true] : [false, true]) {
          // Keep a content-free marker outside records so older versions can still list v1.
          await tx('readwrite', (s) => {
            s.delete(storedId)
            const images = s.transaction.objectStore(ASSETS)
            images.delete(IDBKeyRange.bound(`${storedId}/`, `${storedId}/\uffff`))
            return images.put(true, deletedKey(storedId))
          }, [STORE, ASSETS], legacy)
        }
      }
      for (const k of [...stored]) if (k.startsWith(`${id}/`)) stored.delete(k)
      searchCache.delete(id)
    }).catch((error) => { deletingOrDeleted.delete(id); throw error })
  },
}
