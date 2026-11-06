import { useState } from 'react'
import type { Graph } from '../engine/graph.ts'
import { useT } from '../i18n/index.ts'
import type { MeetingLine } from '../meetings.ts'
import Dialog from './Dialog.tsx'
import './speakers-dialog.css'

export default function SpeakersDialog({ initial, names, lines, boards, readOnly, onRename, onChoose, onClose }: {
  initial: string; names: string[]; lines: MeetingLine[]; boards: Graph[]; readOnly: boolean
  onRename: (from: string, to: string) => void; onChoose: (name: string) => void; onClose: () => void
}) {
  const t = useT()
  const s = t.speakers
  const [from, setFrom] = useState(initial)
  const [draft, setDraft] = useState(initial)
  const to = draft.trim()
  const merging = to !== from && names.includes(to)
  const count = lines.filter((line) => line.speaker === from).length
  const counts = boards.map((graph) => [...graph.nodes, ...graph.edges].filter((item) => item.by === from).length)
  return <Dialog label={s.title} onClose={onClose}>
    <form className="panel speakers-panel" onSubmit={(event) => { event.preventDefault(); if (!readOnly && to && to !== from) onRename(from, to) }}>
      <h2>{s.title}</h2>
      <label>{s.speaker}<select value={from} onChange={(event) => { setFrom(event.target.value); setDraft(event.target.value) }}>
        {names.map((name) => <option key={name} value={name}>{name}</option>)}
      </select></label>
      <button type="button" className="secondary" disabled={readOnly} onClick={() => { onChoose(from); onClose() }}>{s.useNext}</button>
      <label>{s.newName}<input value={draft} required disabled={readOnly} onChange={(event) => setDraft(event.target.value)} onFocus={(event) => event.currentTarget.select()} onKeyDown={(event) => { if (event.key === 'Enter' && event.nativeEvent.isComposing) event.preventDefault() }} /></label>
      <div className="speaker-impact" role="status">
        <p>{s.affects(count, counts.reduce((a, b) => a + b, 0), counts.filter(Boolean).length)}</p>
        {to && to !== from && <p>{merging ? s.mergeNotice(from, to) : s.renameNotice(from, to)}</p>}
        <p>{s.summaryNotice}</p>
      </div>
      <div className="row">
        <button type="submit" disabled={readOnly || !to || to === from}>{merging ? s.merge : t.app.renameSpeaker}</button>
        <button type="button" className="ghost" onClick={onClose}>{t.app.cancel}</button>
      </div>
    </form>
  </Dialog>
}
