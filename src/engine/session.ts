// 一场会议的板书会话：快车道出铅笔稿，慢车道（模型）描墨线。
//
// 模型同一时间只跑一个请求。它忙的时候新来的句子先攒着，下一轮合并成一句处理：
// 宁可跳过中间状态，也不能让板书越拖越晚。

import { attribute, renameBy } from './attribute.ts'
import { type Graph, emptyGraph, findNode } from './graph.ts'
import { type LlmConfig, type LlmResult, proposeOps } from './llm.ts'
import { type Op, applyOps, eraseDrafts, markDrafts, parseOps } from './ops.ts'
import type { Utterance } from './prompt.ts'
import type { Term } from './glossary.ts'
import { mentions, quickOps } from './rules.ts'

export class BoardModelUnavailable extends Error {
  constructor() { super('No board model is configured'); this.name = 'BoardModelUnavailable' }
}

export interface SessionEvents {
  /** Let a direct edit finish before reading the graph or applying returned operations. */
  beforeApply?: (signal: AbortSignal) => Promise<void>
  /** 板书变了（铅笔稿或墨线） */
  graph?: (g: Graph, why: 'draft' | 'ink') => void
  /** 这句话提到了哪些已有节点，用来做高亮 */
  mention?: (ids: string[]) => void
  /**
   * 模型的一轮结果，用来显示延迟和调试。`queuedMs`: how long each line of the batch waited for the model.
   * `seen`、`context`、`said`：发给模型的板书、前文和这一句（一批合成的那句），记纠错样本用
   */
  llm?: (r: LlmResult & { ops: Op[]; batch: Utterance[]; queuedMs: number[]; seen: Graph; context: Utterance[]; said: Utterance }) => void
  /** 模型这一轮出错了；`batch` 是这一轮的几句话，它们不会再画了 */
  error?: (e: unknown, batch: Utterance[]) => void
}

export class Session {
  graph: Graph = emptyGraph()
  history: Utterance[] = []
  llm: LlmConfig
  /** 术语表（全局的加这场会的），给模型起名用 */
  terms: Term[] = []
  private on: SessionEvents
  /** 给模型看几句前文 */
  private context: number
  private pending: Utterance[] = []
  /** When each pending line arrived (performance.now()) */
  private pendingAt: number[] = []
  private busy = false
  private active: Utterance[] = []
  private generation = 0
  private request: AbortController | null = null
  private failures = new Map<string, Utterance>()
  private completed = new Map<string, number>()
  private retrying = new Set<string>()

  constructor(llm: LlmConfig, on: SessionEvents = {}, context = 2) {
    this.llm = llm
    this.on = on
    this.context = context
  }

  /** 一句话说完（转写定稿）时调用。 */
  say(u: Utterance): void {
    if (u.id) {
      const queued = this.pending.findIndex((p) => p.id === u.id)
      const previous = (queued >= 0 ? this.pending[queued] : this.active.find((p) => p.id === u.id))
        ?? this.history.find((p) => p.id === u.id)
      if (previous && (u.revision ?? 0) <= (previous.revision ?? 0)) return
      if (previous?.text === u.text) {
        previous.revision = u.revision
        previous.rawText = u.rawText
        return
      }
      const drawn = this.history.find((p) => p.id === u.id)
      const failed = this.failures.get(u.id)
      if (drawn && !failed && !('failed' in drawn && drawn.failed)) u = { ...u, replaces: drawn.text }
      else if (failed?.replaces) u = { ...u, replaces: failed.replaces }
      if (queued >= 0) {
        this.pending[queued] = u
        return
      }
    }
    this.enqueue(u)
  }

  /** Retry exactly one failed revision, without appending a new transcript line. */
  retry(u: Utterance & { failed?: boolean }): boolean {
    if (!u.id || this.active.some((p) => p.id === u.id) || this.pending.some((p) => p.id === u.id)) return false
    const revision = u.revision ?? 0
    if ((this.completed.get(u.id) ?? -1) >= revision) return false
    const failed = this.failures.get(u.id)
    if ((!failed || (failed.revision ?? 0) !== revision) && !u.failed) return false
    const { failed: _failed, ...line } = u
    this.retrying.add(u.id)
    this.enqueue({ ...((failed?.revision ?? 0) === revision ? failed : {}), ...line })
    return true
  }

  private enqueue(u: Utterance) {
    // Session owns queued utterances so a rename can update an in-flight batch safely.
    u = { ...u }
    this.on.mention?.(mentions(this.graph, u.text))
    const quick = u.replaces === undefined ? quickOps(u.text) : []
    if (quick.length) {
      this.graph = applyOps(this.graph, quick, 'draft')
      this.on.graph?.(this.graph, 'draft')
    }
    this.pending.push(u)
    this.pendingAt.push(performance.now())
    void this.pump()
  }

  renameSpeaker(from: string, to: string) {
    this.graph = renameBy(this.graph, from, to)
    this.history = this.history.map((u) => u.speaker === from ? { ...u, speaker: to } : u)
    // The awaited request retains this batch; update its attribution before it settles.
    for (const u of [...this.active, ...this.pending, ...this.failures.values()]) if (u.speaker === from) u.speaker = to
  }

  isProcessing(line: Pick<Utterance, 'id' | 'revision'>): boolean {
    return !!line.id && this.active.some((u) => u.id === line.id && (u.revision ?? 0) === (line.revision ?? 0))
  }

  /** 等所有排队的句子处理完，评测时用。 */
  async drain(): Promise<void> {
    while (this.busy || this.pending.length) await new Promise((r) => setTimeout(r, 5))
  }

  /** Native IPC may ignore abort; the generation also fences its late callbacks. */
  cancel(): Utterance[] {
    const unfinished = [...this.active, ...this.pending]
    this.generation++
    this.request?.abort()
    this.request = null
    this.active = []
    this.pending = []
    this.pendingAt = []
    this.busy = false
    this.failures.clear()
    this.completed.clear()
    this.retrying.clear()
    this.graph = eraseDrafts(this.graph)
    return unfinished
  }

  private async pump(): Promise<void> {
    if (this.busy || !this.pending.length) return
    this.busy = true
    const generation = this.generation
    const request = new AbortController()
    this.request = request
    // 指代随各自那句话发送，不能把两句「这个」的目标合成一份。
    const pointed = this.pending.findIndex((u) => u.pointing?.length || u.replaces !== undefined || u.id && this.retrying.has(u.id))
    const count = pointed < 0 ? this.pending.length : Math.max(1, pointed)
    const batch = this.pending.splice(0, count)
    this.active = batch
    const started = performance.now()
    const queuedMs = this.pendingAt.splice(0, count).map((t) => Math.round(started - t))
    const next: Utterance =
      batch.length === 1
        ? batch[0]
        : { text: batch.map((u) => (u.speaker ? `${u.speaker}：${u.text}` : u.text)).join(' ') }
    const drafts = markDrafts(this.graph)
    try {
      if (!this.llm.send && !this.llm.baseUrl.trim()) throw new BoardModelUnavailable()
      if (this.on.beforeApply) {
        await this.on.beforeApply(request.signal)
        if (generation !== this.generation) return
      }
      const seen = this.graph
      const context = this.history.filter((u) => !u.id || !this.retrying.has(u.id)).slice(-this.context)
      const r = await proposeOps(this.llm, seen, context, next, request.signal, this.terms)
      if (generation !== this.generation) return
      await this.on.beforeApply?.(request.signal)
      if (generation !== this.generation) return
      if (batch.some((u) => u.id && this.pending.some((p) => p.id === u.id))) {
        // The request may be a native IPC call which cannot be aborted; discard stale output.
        this.graph = eraseDrafts(this.graph, drafts)
        this.on.graph?.(this.graph, 'draft')
        return
      }
      const ops = parseOps(r.text)
      for (const u of batch) if (u.id) { this.completed.set(u.id, u.revision ?? 0); this.failures.delete(u.id) }
      // 这一轮新写成墨线的，记下是这批话里谁说的
      this.graph = attribute(this.graph, eraseDrafts(applyOps(this.graph, ops, 'ink'), drafts), batch)
      this.on.graph?.(this.graph, 'ink')
      // 指给大家看的：板书不变，高亮一下
      const shown = ops.flatMap((o) => {
        if (o.op !== 'H') return []
        const t = o.target
        const n = 'id' in t ? this.graph.nodes.find((x) => x.id === t.id) : findNode(this.graph, t.name)
        return n ? [n.id] : []
      })
      if (shown.length) this.on.mention?.(shown)
      this.on.llm?.({ ...r, ops, batch, queuedMs, seen, context, said: next })
    } catch (e) {
      if (generation !== this.generation) return
      if (!batch.some((u) => u.id && this.pending.some((p) => p.id === u.id))) {
        this.graph = eraseDrafts(this.graph, drafts)
        this.on.graph?.(this.graph, 'draft')
        for (const u of batch) if (u.id) this.failures.set(u.id, u)
        this.on.error?.(e, batch)
      }
    } finally {
      if (generation !== this.generation) return
      const superseded = batch.some((u) => u.id && this.pending.some((p) => p.id === u.id))
      if (superseded) {
        const newer = batch.map((u) => {
          const i = u.id ? this.pending.findIndex((p) => p.id === u.id) : -1
          if (i < 0) return u
          this.pendingAt.splice(i, 1)
          return this.pending.splice(i, 1)[0]
        })
        this.pending.unshift(...newer)
        this.pendingAt.unshift(...newer.map(() => started))
      } else {
        for (const u of batch) {
          const i = u.id ? this.history.findIndex((p) => p.id === u.id) : -1
          const { replaces: _previous, ...current } = u
          if (i < 0) this.history.push(current)
          else this.history[i] = current
        }
      }
      this.active = []
      for (const u of batch) if (u.id) this.retrying.delete(u.id)
      this.request = null
      this.busy = false
      void this.pump()
    }
  }
}
