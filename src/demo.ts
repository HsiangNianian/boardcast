// Recorded sample meetings (issue #30): made once with a real model by eval/record.ts and served
// from public/demos/, so the web demo can show a board growing without any model.

import type { SpokenLang } from './engine/lang.ts'
import type { Graph } from './engine/graph.ts'
import type { Utterance } from './engine/prompt.ts'
import type { Step } from './engine/steps.ts'
import { MEETING_VERSION, type Meeting, type MeetingLine } from './meetings.ts'

export interface DemoInfo {
  id: string
  name: string
  /** Language spoken in the meeting */
  lang: SpokenLang
}

export interface Demo extends DemoInfo {
  /** The final board */
  graph: Graph
  history: Utterance[]
  lines: MeetingLine[]
  steps: Step[]
  summary?: string
}

const at = (path: string) => new URL(`demos/${path}`, document.baseURI).href

/** The recorded demos; empty when there are none (e.g. a build without public/demos) */
export async function demoIndex(): Promise<DemoInfo[]> {
  try {
    const r = await fetch(at('index.json'))
    return r.ok ? ((await r.json()) as DemoInfo[]) : []
  } catch {
    return []
  }
}

export async function loadDemo(id: string): Promise<Demo> {
  const r = await fetch(at(`${encodeURIComponent(id)}.json`))
  if (!r.ok) throw new Error(`demos/${id}.json: HTTP ${r.status}`)
  return (await r.json()) as Demo
}

/** A demo as a meeting the app can open and replay. It gets a fresh id and is not meant to be saved. */
export function demoMeeting(d: Demo, id: string): Meeting {
  const now = Date.now()
  return {
    version: MEETING_VERSION,
    id,
    title: d.name,
    titled: true,
    createdAt: now,
    updatedAt: now,
    graph: d.graph,
    history: d.history,
    lines: d.lines,
    steps: d.steps,
    summary: d.summary,
  }
}
