// 一场会议的板书会话：快车道出铅笔稿，慢车道（模型）描墨线。
//
// 模型同一时间只跑一个请求。它忙的时候新来的句子先攒着，下一轮合并成一句处理：
// 宁可跳过中间状态，也不能让板书越拖越晚。

import { type Graph, emptyGraph, findNode } from './graph.ts'
import { type LlmConfig, type LlmResult, proposeOps } from './llm.ts'
import { type Op, applyOps, eraseDrafts, markDrafts, parseOps } from './ops.ts'
import type { Utterance } from './prompt.ts'
import type { Term } from './glossary.ts'
import { mentions, quickOps } from './rules.ts'

export interface SessionEvents {
  /** 板书变了（铅笔稿或墨线） */
  graph?: (g: Graph, why: 'draft' | 'ink') => void
  /** 这句话提到了哪些已有节点，用来做高亮 */
  mention?: (ids: string[]) => void
  /** 模型的一轮结果，用来显示延迟和调试 */
  llm?: (r: LlmResult & { ops: Op[]; batch: Utterance[] }) => void
  error?: (e: unknown) => void
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
  private busy = false

  constructor(llm: LlmConfig, on: SessionEvents = {}, context = 2) {
    this.llm = llm
    this.on = on
    this.context = context
  }

  /** 一句话说完（转写定稿）时调用。 */
  say(u: Utterance): void {
    this.on.mention?.(mentions(this.graph, u.text))
    const quick = quickOps(u.text)
    if (quick.length) {
      this.graph = applyOps(this.graph, quick, 'draft')
      this.on.graph?.(this.graph, 'draft')
    }
    this.pending.push(u)
    void this.pump()
  }

  /** 等所有排队的句子处理完，评测时用。 */
  async drain(): Promise<void> {
    while (this.busy || this.pending.length) await new Promise((r) => setTimeout(r, 5))
  }

  private async pump(): Promise<void> {
    if (this.busy || !this.pending.length) return
    this.busy = true
    const batch = this.pending.splice(0)
    const next: Utterance =
      batch.length === 1
        ? batch[0]
        : { text: batch.map((u) => (u.speaker ? `${u.speaker}：${u.text}` : u.text)).join(' ') }
    try {
      const drafts = markDrafts(this.graph)
      const r = await proposeOps(this.llm, this.graph, this.history.slice(-this.context), next, undefined, this.terms)
      const ops = parseOps(r.text)
      this.graph = eraseDrafts(applyOps(this.graph, ops, 'ink'), drafts)
      this.on.graph?.(this.graph, 'ink')
      // 指给大家看的：板书不变，高亮一下
      const shown = ops.flatMap((o) => {
        if (o.op !== 'H') return []
        const t = o.target
        const n = 'id' in t ? this.graph.nodes.find((x) => x.id === t.id) : findNode(this.graph, t.name)
        return n ? [n.id] : []
      })
      if (shown.length) this.on.mention?.(shown)
      this.on.llm?.({ ...r, ops, batch })
    } catch (e) {
      this.on.error?.(e)
    } finally {
      this.history.push(...batch)
      this.busy = false
      void this.pump()
    }
  }
}
