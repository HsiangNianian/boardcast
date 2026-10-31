// 导出的回放 HTML 里带的数据，和生成这份 HTML 的工具函数（应用里用）。

import type { LayoutMemory } from '../board/layout.ts'
import type { Graph } from '../engine/graph.ts'
import type { Step } from '../engine/steps.ts'
import type { MeetingLine } from '../meetings.ts'

export interface ReplayPage {
  graph: Graph
  steps?: Step[]
  /** 导入页的底图（data URL） */
  underlay?: string
  /** 这张板最终的排版：回放从它开始，东西出现在最终的位置上，也用不上 ELK */
  layout?: LayoutMemory
}

export interface ReplayData {
  title: string
  createdAt: number
  summary?: string
  lines: MeetingLine[]
  pages: ReplayPage[]
}

/** 板书里会写到的汉字：每张板最终的样子和回放过程中出现过的，都算上 */
export function usedHan(data: ReplayData): string[] {
  const out = new Set<string>()
  const add = (s: string | undefined) => {
    for (const ch of s ?? '') if (/\p{Script=Han}/u.test(ch)) out.add(ch)
  }
  for (const p of data.pages) {
    for (const n of p.graph.nodes) add(n.label)
    for (const e of p.graph.edges) add(e.label)
    for (const x of p.graph.groups) add(x.label)
    for (const s of p.steps ?? []) {
      for (const n of s.d.nodes?.set ?? []) add(n.label)
      for (const e of s.d.edges?.set ?? []) add(e.label)
      for (const x of s.d.groups?.set ?? []) add(x.label)
    }
  }
  return [...out]
}

/** JSON 放进 <script> 里：「</script>」和 U+2028/2029 要转义，不然会提前结束脚本 */
const LS = String.fromCharCode(0x2028)
const PS = String.fromCharCode(0x2029)
const safe = (v: unknown) => JSON.stringify(v).replace(/</g, '\\u003c').split(LS).join('\\u2028').split(PS).join('\\u2029')

const escapeHtml = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!)

/** 拼成一个自带一切的 HTML：播放器代码、会议数据、用到的汉字笔画 */
export function replayHtml(data: ReplayData, player: string, hanzi: Record<string, unknown>): string {
  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="boardcast">
<title>${escapeHtml(data.title)} · boardcast 回放</title>
</head>
<body>
<div id="root"></div>
<script>window.__boardcastData = ${safe(data)};
window.__boardcastHanzi = ${safe(hanzi)};</script>
<script>${player.replace(/<\/script/gi, '<\\/script')}</script>
</body>
</html>
`
}
