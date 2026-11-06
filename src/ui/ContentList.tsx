import { useEffect, useRef, useState } from 'react'
import { freeObjectName, ownerOf, type FreeScene } from '../board/freeScene.ts'
import type { Selection } from '../engine/edit.ts'
import type { Graph } from '../engine/graph.ts'
import { useT } from '../i18n/index.ts'
import './content-list.css'

export default function ContentList({ graph, scene, page, sourcePages, blocked, readOnly, onPage, onLocate, onRename, onDelete, onSource, onLocateFree, onEditFree }: {
  graph: Graph; scene?: FreeScene; page: number; sourcePages: (number | undefined)[]; blocked: boolean; readOnly: boolean
  onPage: (page: number) => void
  onLocate: (selection: Selection) => void
  onRename: (selection: Selection, text: string) => void
  onDelete: (selection: Selection) => void
  onSource: (selection: Selection) => void
  onLocateFree: (id: string) => void
  onEditFree: (id: string, name: string | null) => void
}) {
  const t = useT()
  const c = t.contents
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  useEffect(() => { if (readOnly) setEditing(null) }, [readOnly])
  const list = useRef<HTMLDivElement>(null)
  const nameOf = (id: string) => graph.nodes.find((n) => n.id === id)?.label ?? id
  const free = scene?.elements.filter((e) => !e.isDeleted && !ownerOf(e)) ?? []
  const finish = () => { setEditing(null); requestAnimationFrame(() => (list.current?.querySelector<HTMLButtonElement>(`[data-content-edit="${CSS.escape(editing ?? '')}"]`) ?? list.current)?.focus()) }
  const actions = (id: string, label: string, locate: () => void, rename?: (text: string) => void, remove?: () => void, source?: () => void, allowEmpty = false, limit?: number) => <>
    {editing === id ? <form className="content-rename" onSubmit={(event) => { event.preventDefault(); if (readOnly || !allowEmpty && !draft.trim()) return; rename?.(draft); finish() }}>
      <label>{c.name}<input value={draft} maxLength={limit} disabled={readOnly} autoFocus onFocus={(event) => event.currentTarget.select()} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) { if (event.key === 'Enter') event.preventDefault(); return }
        if (event.key === 'Escape') { event.stopPropagation(); finish() }
      }} /></label>
      <button type="submit" className="small" disabled={readOnly || !allowEmpty && !draft.trim()}>{c.save}</button>
      <button type="button" className="ghost small" onClick={finish}>{t.app.cancel}</button>
    </form> : <div className="content-actions">
      <button type="button" className="ghost small" onClick={locate}>{c.locate}</button>
      {source && <button type="button" className="ghost small" onClick={source}>{c.source}</button>}
      {rename && <button type="button" className="ghost small" data-content-edit={id} disabled={readOnly} onClick={() => { setDraft(label); setEditing(id) }}>{c.rename}</button>}
      {remove && <button type="button" className="ghost small" disabled={readOnly} onClick={() => { remove(); requestAnimationFrame(() => list.current?.focus()) }}>{c.remove}</button>}
    </div>}
  </>
  return <div className="content-list" ref={list} tabIndex={-1}>
    <label className="content-page">{c.page}<select value={page} disabled={blocked} onChange={(event) => {
      const panel = list.current?.parentElement
      onPage(Number(event.target.value))
      // Page changes remount the list to isolate drafts; restore only focus lost by that remount.
      requestAnimationFrame(() => {
        const picker = panel?.querySelector<HTMLSelectElement>('.content-page select')
        if (document.activeElement === document.body && picker?.getClientRects().length) picker.focus()
      })
    }}>
      {sourcePages.map((source, i) => <option key={i} value={i}>{t.pages.board(i + 1)}{t.pages.sourcePage(source) && ` · ${t.pages.sourcePage(source)}`}</option>)}
    </select></label>
    {!graph.nodes.length && !graph.edges.length && !free.length && <p>{graph.underlay ? c.imageOnly : c.empty}</p>}
    {graph.nodes.length > 0 && <section aria-label={c.nodes}><h3>{c.nodes}</h3><ul>{graph.nodes.map((node) => {
      const sel: Selection = { kind: 'node', id: node.id }
      const group = graph.groups.find((g) => g.id === node.group)
      return <li key={node.id}>
        <span className="content-kind">{c.kinds[node.kind]}</span><strong>{node.label}</strong>
        {node.parent && <p>{c.parent(nameOf(node.parent))}</p>}{group && <p>{c.group(group.label)}</p>}
        {node.marks.length > 0 && <p>{node.marks.map((mark) => t.app.marks[mark]).join(' · ')}</p>}
        {node.text && <details><summary>{c.original}</summary><p className="content-original">{node.text}</p></details>}
        {!!node.spans?.length && <ul>{node.spans.map((span, i) => <li key={i}>{t.app.marks[span.mark]}: {span.text}</li>)}</ul>}
        {node.by && <p>{c.by(node.by)}</p>}
        {actions(node.id, node.label, () => onLocate(sel), node.kind === 'region' ? undefined : (text) => onRename(sel, text), node.kind === 'region' ? undefined : () => onDelete(sel), node.by ? () => onSource(sel) : undefined)}
        {node.kind === 'region' && <small>{c.fixedSource}</small>}
      </li>
    })}</ul></section>}
    {graph.edges.length > 0 && <section aria-label={c.edges}><h3>{c.edges}</h3><ul>{graph.edges.map((edge) => {
      const sel: Selection = { kind: 'edge', from: edge.from, to: edge.to }
      return <li key={`${edge.from}>${edge.to}`}><strong>{nameOf(edge.from)} → {nameOf(edge.to)}</strong>{edge.label && <p>{edge.label}</p>}{edge.by && <p>{c.by(edge.by)}</p>}
        {actions(`${edge.from}>${edge.to}`, edge.label ?? '', () => onLocate(sel), (text) => onRename(sel, text), () => onDelete(sel), edge.by ? () => onSource(sel) : undefined, true)}
      </li>
    })}</ul></section>}
    {free.length > 0 && <section aria-label={c.free}><h3>{c.free}</h3><ul>{free.map((element) => {
      const name = freeObjectName(element)
      return <li key={element.id}><span className="content-kind">{c.types[element.type]}</span><strong>{name || c.unnamed}</strong>
        {element.type === 'text' && <p className="content-original">{element.originalText || element.text}</p>}
        {actions(element.id, name, () => onLocateFree(element.id), (text) => onEditFree(element.id, text), () => onEditFree(element.id, null), undefined, true, 120)}
      </li>
    })}</ul></section>}
  </div>
}
