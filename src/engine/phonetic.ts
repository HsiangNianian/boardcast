import { type Graph, norm } from './graph.ts'
import type { Term } from './glossary.ts'
import type { Op, Ref } from './ops.ts'
import type { Utterance } from './prompt.ts'
import { pronunciations } from './pronunciation-client.ts'

export interface SpeechMatch {
  heard: string
  ids: string[]
  kind: 'exact' | 'sound'
  selected?: string
}

const key = (s: string) => norm(s.normalize('NFKC'))
const kana = /[\p{Script=Hiragana}\p{Script=Katakana}]/u
export const negated = (s: string) => /不要|不必|不用|别(?=把|将|画|删|连|圈|动|加|高亮)|\b(?:not|never|don['’]t|cannot|can['’]t)\b|ないで|ません/iu.test(s)

/** Resolve only against this board. Exact names/aliases win, even over a pointed homophone. */
export async function resolveTargets(
  g: Graph, texts: string[], language: 'zh' | 'ja', terms: Term[] = [], pointing: string[] = [],
): Promise<SpeechMatch[]> {
  const names = g.nodes.filter((n) => n.ink !== 'draft').flatMap((n) => {
    const forms = [n.label, ...n.aliases]
    for (const term of terms) if (forms.some((s) => key(s) === key(term.term))) forms.push(...term.also)
    return [...new Set(forms)].map((text) => ({ id: n.id, text }))
  })
  const choose = (heard: string, ids: string[], kind: SpeechMatch['kind']): SpeechMatch => {
    ids = [...new Set(ids)]
    const pointed = ids.filter((id) => pointing.includes(id))
    return { heard, ids, kind, selected: ids.length === 1 ? ids[0] : pointed.length === 1 ? pointed[0] : undefined }
  }
  const matches = texts.map((text) => choose(text, names.filter((n) => key(n.text) === key(text)).map((n) => n.id), 'exact'))
  const missing = matches.filter((m) => !m.ids.length)
  if (!names.length || !missing.length) return matches
  const forms = [...new Set([...names.map((n) => n.text), ...missing.map((m) => m.heard)])]
  const sounds = await pronunciations(forms, language)
  const byText = new Map(forms.map((s, i) => [s, sounds[i]]))
  const bySound = new Map<string, string[]>()
  for (const n of names) for (const sound of byText.get(n.text) ?? []) {
    // One-phoneme words such as "a" are too weak to identify a board label.
    if (sound.startsWith('en:') && sound.split(' ').length < 3) continue
    bySound.set(sound, [...(bySound.get(sound) ?? []), n.id])
  }
  return matches.map((m) => m.ids.length ? m : choose(m.heard,
    (byText.get(m.heard) ?? []).flatMap((s) => bySound.get(s) ?? []), 'sound'))
}

interface Command { targets: string[]; op: 'H' | 'M' | 'E' | 'D'; mark?: '圈出' | '下划线' | '星标'; negative: boolean }

/** Whole commands only; questions, quoted instructions and ordinary discussion stay with the agent. */
function command(text: string): Command | undefined {
  let s = text.normalize('NFKC').trim().replace(/[。！!.]+$/, '').trim()
  if (s.length > 160 || /[?？\n]/.test(s)) return
  const negative = /^(?:please\s+)?(?:do not|don['’]t|never)\s+|^(?:请)?(?:不要|不必|别)|ないで(?:ください)?$/iu.test(s)
  s = s.replace(/^(?:please\s+)?(?:do not\s+|don't\s+|don’t\s+|never\s+)?(?:please\s+)?/i, '')
    .replace(/^(?:请)?(?:不要|不必|别)?(?:请)?/, '').replace(/,?\s+please$/i, '')
  const clean = (t: string) => t.trim().replace(/^(?:the\s+|「|『|“|")/i, '').replace(/[」』”"]$/, '').trim()
  const make = (op: Command['op'], targets: string[], mark?: Command['mark']): Command => ({ op, targets: targets.map(clean), mark, negative })
  const actions = [
    ['H', undefined, /^(?:highlight|high light|show)\s+(.+)$/i, /^(?:高亮|指一下|指出)(.+)$/, /^(.+?)(?:を|に)(?:ハイライトして|ハイライトしないで|注目して)(?:ください)?$/],
    ['M', '圈出', /^circle\s+(.+)$/i, /^(?:圈出|圈一下)(.+)$/, /^(.+?)(?:を|に)(?:丸を(?:付けて|つけて|付けないで|つけないで)|丸で囲んで|丸で囲まないで)(?:ください)?$/],
    ['M', '下划线', /^under\s*line\s+(.+)$/i, /^(?:给|把)(.+?)(?:加下划线|加下画线|画下划线)$/, /^(.+?)(?:を|に)(?:下線を(?:引いて|引かないで)|かせんをひいて)(?:ください)?$/],
    ['M', '星标', /^(?:star|put a star (?:on|beside))\s+(.+)$/i, /^(?:给|把)(.+?)(?:标星|加星标)$/, /^(.+?)(?:を|に)星を(?:付けて|つけて|付けないで|つけないで)(?:ください)?$/],
    ['D', undefined, /^(?:delete|remove)\s+(.+)$/i, /^(?:删除|删掉)(.+)$/, /^(.+?)を(?:削除|消去)(?:して|しないで)(?:ください)?$/],
  ] as const
  for (const [op, mark, ...patterns] of actions) for (const pattern of patterns) {
    const m = pattern.exec(s)
    if (m) return make(op, [m[1]], mark)
  }
  const circle = /^(?:把|将)(.+?)(?:圈起来|圈出来)$/.exec(s)
  if (circle) return make('M', [circle[1]], '圈出')
  for (const pattern of [/^connect\s+(.+?)\s+to\s+(.+)$/i, /^(?:把|将)(.+?)(?:连到|连接到)(.+)$/, /^(.+?)から(.+?)へ(?:線を引いて|つないで|つながないで)(?:ください)?$/]) {
    const m = pattern.exec(s)
    if (m) return make('E', [m[1], m[2]])
  }
}

function surfaces(text: string, language: 'zh' | 'ja'): string[] {
  // Bound work on batched transcripts; commands above are always checked in full.
  const words = [...new Intl.Segmenter(language, { granularity: 'word' }).segment(text.slice(0, 2048))]
  const parts: string[] = []
  for (let i = 0; i < words.length; i++) {
    if (!words[i].isWordLike) continue
    let count = 0
    for (let j = i; j < words.length && count < 4; j++) {
      if (!words[j].isWordLike && words[j].segment.trim()) break
      if (words[j].isWordLike) {
        count++
        const part = text.slice(words[i].index, words[j].index + words[j].segment.length)
        if (part.length > 64) break
        if (part.length >= 2) parts.push(part)
      }
    }
  }
  return [...new Set(parts)]
}

export interface SpeechPlan { matches: SpeechMatch[]; hints: string; direct?: Op[] }

export async function speechPlan(g: Graph, recent: Utterance[], next: Utterance, terms: Term[]): Promise<SpeechPlan> {
  // Revisions may need to undo the old meaning, so always let the agent handle them.
  const cmd = next.replaces === undefined ? command(next.text) : undefined
  if (cmd?.negative) return { matches: [], hints: '', direct: [] }
  const language = kana.test([next.text, ...recent.map((u) => u.text)].join(' ')) ? 'ja' : 'zh'
  const texts = cmd?.targets ?? surfaces(next.text, language)
  let matches: SpeechMatch[]
  try {
    matches = await resolveTargets(g, texts, language, terms, next.pointing)
  } catch (error) {
    // A missing dictionary must not prevent transcription or the normal agent from working.
    console.warn('Pronunciation fallback unavailable:', error)
    return { matches: [], hints: '' }
  }
  if (cmd) {
    const targets = cmd.targets.map((t) => matches.find((m) => m.heard === t)!)
    if (targets.some((m) => m.ids.length && !m.selected)) return { matches, hints: '', direct: [] }
    if (targets.every((m) => m.selected)) {
      const refs = targets.map((m) => ({ id: m.selected! }))
      // Deletion needs an exact name/alias, never sound alone.
      const direct: Op[] = cmd.op === 'D' ? (targets[0].kind === 'exact' ? [{ op: 'D', target: refs[0] }] : [])
        : cmd.op === 'E' ? [{ op: 'E', from: refs[0], to: refs[1] }]
        : cmd.op === 'M' ? [{ op: 'M', target: refs[0], mark: cmd.mark! }]
        : [{ op: 'H', target: refs[0] }]
      return { matches, hints: '', direct }
    }
  }
  const sound = matches.filter((m) => m.kind === 'sound' && m.ids.length).sort((a, b) => b.heard.length - a.heard.length).slice(0, 8)
  const hints = sound.length ? '读音候选（只定位已有标签；不改原话，不据此改变否定/动作，不合并新概念；歧义结合前文和手指，无法确定就不操作；删除/改名必须有明确原话依据）：\n'
    + sound.map((m) => `${JSON.stringify(m.heard)} → ${m.ids.map((id) => `${id} ${JSON.stringify(g.nodes.find((n) => n.id === id)!.label)}`).join(' / ')}${m.selected ? '' : '（有歧义）'}`).join('\n') : ''
  return { matches, hints }
}

/** Repair references, never new labels, rename text, permanent aliases or the transcript itself. */
export function bindSpeechOps(ops: Op[], plan: SpeechPlan, next: Utterance, g: Graph): Op[] {
  if (negated(next.text) || next.replaces !== undefined) return ops
  const introducing = /新增|新建|新(?:的)?(?:服务|节点|模块|系统|组件|项目|概念)|\bnew\b|新(?:しい|規)|追加/i.test(next.text)
  const created = ops.flatMap((o) => o.op === 'N' || o.op === 'T' || o.op === 'L' || o.op === 'R' ? [key(o.name)] : [])
  const ref = (r: Ref): Ref | undefined => {
    if ('id' in r) {
      const id = r.id
      const draft = g.nodes.find((n) => n.id === id && n.ink === 'draft')
      if (!draft) return r
      r = { name: draft.label }
    }
    if (introducing || created.includes(key(r.name))) return r
    const m = plan.matches.find((m) => key(m.heard) === key(r.name) && m.ids.length)
    return !m ? r : m.selected ? { id: m.selected } : undefined
  }
  return ops.flatMap((o): Op[] => {
    if (o.op === 'D' || o.op === 'R' || o.op === 'X' || o.op === 'A') return [o]
    if (o.op === 'E') {
      const from = ref(o.from), to = ref(o.to)
      return from && to ? [{ ...o, from, to }] : []
    }
    if ('target' in o) {
      const target = ref(o.target)
      return target ? [{ ...o, target }] : []
    }
    if ('parent' in o && o.parent) {
      const parent = ref(o.parent)
      return parent ? [{ ...o, parent }] : []
    }
    if (o.op === 'G') {
      const members = o.members.map(ref)
      return members.every((r) => r !== undefined) ? [{ ...o, members }] : []
    }
    return [o]
  })
}
