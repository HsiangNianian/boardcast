// 把板书导出成 Markdown：话题和条目写成标题加编号列表，流程图写成 Mermaid，最后附上会议记录。
// 一场会有几张板时，每张板一节（「## 第 2 张板」，里面的标题降一级），会议记录也按板分开。
// 铅笔稿（还没被模型确认的草稿）不导出。

import { type Graph, type Node, children } from '../engine/graph.ts'
import { msg } from '../i18n/index.ts'
import { isDiagram, isHead } from './layout.ts'

export interface SaidLine {
  speaker?: string
  text: string
  /** 说这句时在第几张板（从 0 数） */
  page?: number
}

const isAttach = (n: Node) => n.kind === 'question' || n.kind === 'formula' || n.kind === 'note'
const LETTERS = 'abcdefghijklmnopqrstuvwxyz'

/** 条目文字加上标记：划掉 → 删除线，圈出 → 加粗，下划线 → 下划线，星标 → ★ */
function marked(n: Node): string {
  let t = n.label
  if (n.marks.includes('划掉')) t = `~~${t}~~`
  if (n.marks.includes('圈出')) t = `**${t}**`
  if (n.marks.includes('下划线')) t = `<u>${t}</u>`
  if (n.marks.includes('星标')) t = `${t} ★`
  return t
}

function attachLine(n: Node): string {
  if (n.kind === 'question') return `${msg().doc.question}${marked(n)}`
  if (n.kind === 'formula') return `\`${n.label}\``
  return `${msg().doc.note}${marked(n)}`
}

/** Mermaid 里的文字要转义引号 */
const mm = (s: string) => s.replace(/"/g, '#quot;')

/**
 * `pages`：一场会的所有板（从第一张起）；不给或者只有一张时，就是 `g` 这一块板。
 */
export function toMarkdown(
  g: Graph,
  lines: SaidLine[] = [],
  title = msg().doc.title,
  when = new Date(),
  summary?: string,
  pages?: Graph[],
): string {
  const d = msg().doc
  const out: string[] = []
  const stamp = `${when.getFullYear()}-${String(when.getMonth() + 1).padStart(2, '0')}-${String(when.getDate()).padStart(2, '0')} ${String(when.getHours()).padStart(2, '0')}:${String(when.getMinutes()).padStart(2, '0')}`
  out.push(`# ${title}`, '', `> ${stamp} · boardcast`, '')
  if (summary?.trim()) out.push(`## ${d.summary}`, '', summary.trim(), '')

  const many = pages && pages.length > 1
  if (many)
    pages.forEach((p, i) => {
      out.push(`## ${d.board(i + 1)}`, '')
      board(p, out, '###')
    })
  else board(g, out, '##')

  if (lines.length) {
    out.push(`## ${d.transcript}`, '')
    let page: number | undefined
    for (const l of lines) {
      if (many && (l.page ?? 0) !== page) {
        page = l.page ?? 0
        out.push('', `### ${d.board(page + 1)}`, '')
      }
      out.push(l.speaker ? `- **${l.speaker}**${d.colon}${l.text}` : `- ${l.text}`)
    }
    out.push('')
  }
  return out.join('\n').replace(/\n{3,}/g, '\n\n')
}

/** 一块板的内容；`h` 是这一层小节标题用几个 # */
function board(g: Graph, out: string[], h: string) {
  const d = msg().doc
  const live = (n: Node) => n.ink !== 'draft'

  // 话题：标题 + 编号条目，挂件跟在被挂的东西下面
  const walk = (parent: Node, depth: number) => {
    let index = 0
    const kids = children(g, parent.id)
      .filter(live)
      .sort((a, b) => Number(isAttach(a)) - Number(isAttach(b)))
    for (const c of kids) {
      const pad = '   '.repeat(Math.max(0, depth - 1))
      if (isAttach(c)) out.push(`${pad}- ${attachLine(c)}`)
      else {
        const prefix = depth === 1 ? `${index + 1}.` : depth === 2 ? `${LETTERS[index % 26]}.` : '-'
        out.push(`${pad}${depth === 2 ? '- ' + prefix : prefix} ${marked(c)}`)
        index++
        walk(c, depth + 1)
      }
    }
  }
  for (const n of g.nodes.filter(live)) {
    if (!isHead(g, n)) continue
    out.push(`${h} ${marked(n)}`, '')
    const before = out.length
    walk(n, 1)
    if (out.length > before) out.push('')
  }

  // 流程图：Mermaid
  const dia = g.nodes.filter((n) => live(n) && isDiagram(g, n))
  if (dia.length) {
    const ids = new Set(dia.map((n) => n.id))
    out.push(`${h} ${d.flow}`, '', '```mermaid', 'flowchart LR')
    const grouped = new Set<string>()
    for (const grp of g.groups) {
      const members = dia.filter((n) => n.group === grp.id)
      if (!members.length) continue
      out.push(`  subgraph ${grp.id}["${mm(grp.label)}"]`)
      for (const n of members) {
        out.push(`    ${n.id}["${mm(n.label)}"]`)
        grouped.add(n.id)
      }
      out.push('  end')
    }
    for (const n of dia) if (!grouped.has(n.id)) out.push(`  ${n.id}["${mm(n.label)}"]`)
    for (const e of g.edges) {
      if (e.ink === 'draft' || !ids.has(e.from) || !ids.has(e.to)) continue
      out.push(e.label ? `  ${e.from} -->|"${mm(e.label)}"| ${e.to}` : `  ${e.from} --> ${e.to}`)
    }
    out.push('```', '')
    // 挂在流程图节点上的问题、备注、公式
    const notes = g.nodes.filter((n) => live(n) && isAttach(n) && n.parent && ids.has(n.parent))
    for (const n of notes) out.push(`- ${g.nodes.find((p) => p.id === n.parent)?.label}${d.colon}${attachLine(n)}`)
    if (notes.length) out.push('')
  }

  // 导入页上被圈画过、写了批注的原文
  const pins = g.nodes.filter((n) => live(n) && n.kind === 'region' && (n.marks.length || n.spans?.length || children(g, n.id).some(live)))
  if (pins.length) {
    out.push(`${h} ${d.annotations}`, '')
    for (const n of pins) {
      out.push(`- ${marked(n)}`)
      for (const s of n.spans ?? []) out.push(`   - ${msg().app.marks[s.mark]}${d.colon}${s.text}`)
      for (const c of children(g, n.id).filter(live)) out.push(`   - ${attachLine(c)}`)
    }
    out.push('')
  }

  // 没有挂靠的问题和公式
  const loose = g.nodes.filter((n) => live(n) && isAttach(n) && (!n.parent || !g.nodes.some((p) => p.id === n.parent)))
  if (loose.length) {
    out.push(`${h} ${d.open}`, '')
    for (const n of loose) out.push(`- ${attachLine(n)}`)
    out.push('')
  }

  // 话题、条目之间的连线（跨区域的箭头），写成一句话
  const cross = g.edges.filter((e) => e.ink !== 'draft' && !(dia.some((n) => n.id === e.from) && dia.some((n) => n.id === e.to)))
  if (cross.length) {
    out.push(`${h} ${d.links}`, '')
    const name = (id: string) => g.nodes.find((n) => n.id === id)?.label ?? id
    for (const e of cross) out.push(`- ${name(e.from)} → ${name(e.to)}${e.label ? `${d.colon}${e.label}` : ''}`)
    out.push('')
  }
}
