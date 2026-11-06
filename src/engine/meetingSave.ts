import type { Meeting } from '../meetings.ts'

const transient = new Set(['version', 'versionNonce', 'updated', 'seed', 'index'])
export function sceneContent(scene: Meeting['scene']) {
  if (!scene) return undefined
  const generated = new Set(scene.generated.map((e) => e.id))
  const elements = scene.elements.filter((e) => !generated.has(e.id) || Object.keys(scene.overrides?.[e.id] ?? {}).length)
    .map((e) => Object.fromEntries(Object.entries(e).filter(([key]) => !transient.has(key))))
  if (!elements.length && !Object.keys(scene.files).length && !Object.keys(scene.modelCandidates ?? {}).length) return undefined
  return { elements, files: scene.files,
    overrides: Object.keys(scene.overrides ?? {}).length ? scene.overrides : undefined,
    modelCandidates: Object.keys(scene.modelCandidates ?? {}).length ? scene.modelCandidates : undefined,
    hiddenGenerated: scene.hiddenGenerated?.length ? scene.hiddenGenerated : undefined }
}

/** View position, layout projection and SVG caches do not edit the meeting's content. */
export function meetingContent(meeting: Meeting): string {
  const { updatedAt: _updatedAt, graph, layout: _layout, scene, steps, pages, page: _page, history,
    missingAssets: _missingAssets, assetIssues: _assetIssues, ...metadata } = meeting
  const boards = (pages?.length ? pages : [{ graph, scene, steps }]).map(({ layout: _pageLayout, thumb: _thumb, scene: pageScene, steps: pageSteps, ...board }) => {
    let previous: string | undefined
    const meaningful = (pageSteps ?? []).flatMap((step) => {
      const content = sceneContent(step.scene ?? undefined)
      const signature = JSON.stringify(content)
      const changed = signature !== previous
      previous = signature
      return Object.keys(step.d).length || changed ? [{ ...step, scene: content }] : []
    })
    return { ...board, scene: sceneContent(pageScene), steps: meaningful }
  })
  // Cancellation may rebuild model context from transcript rows. Their telemetry and
  // page fields are already compared in `lines`; they do not change the utterance.
  const context = history.map(({ id, revision, replaces, speaker, text, rawText, pointing }) =>
    ({ id, revision, replaces, speaker, text, rawText, pointing }))
  return JSON.stringify({ ...metadata, history: context, boards })
}

export type SaveStatus = {
  state: 'idle' | 'dirty' | 'saving' | 'saved' | 'error'
  error?: string
  savedAt?: number
}

/** Only an acknowledgement for the latest revision can clear the unsaved state. */
export class MeetingSave {
  private revision = 0
  private generation = 0
  private attempt = 0
  private savedContent: string | undefined
  private writing = 0
  status: SaveStatus = { state: 'idle' }
  private write: (meeting: Meeting) => Promise<void>
  private notify: (status: SaveStatus) => void

  constructor(write: (meeting: Meeting) => Promise<void>, notify: (status: SaveStatus) => void) {
    this.write = write
    this.notify = notify
  }

  private show(status: SaveStatus) {
    this.status = status
    this.notify(status)
  }

  reset(savedAt?: number) {
    this.generation++
    this.revision = 0
    this.savedContent = undefined
    this.show(savedAt === undefined ? { state: 'idle' } : { state: 'saved', savedAt })
  }

  remember(meeting: Meeting) { this.savedContent = meetingContent(meeting) }

  /** A read-only meeting with missing assets must remain closable without rewriting it. */
  unchanged(meeting: Meeting): boolean {
    if (this.writing || this.savedContent === undefined || meetingContent(meeting) !== this.savedContent) return false
    this.show({ state: 'saved', savedAt: this.status.savedAt })
    return true
  }

  changed() {
    this.revision++
    if (this.status.state !== 'error') this.show({ state: 'dirty', savedAt: this.status.savedAt })
  }

  failed(error: unknown) {
    this.show({ state: 'error', error: error instanceof Error ? error.message : String(error), savedAt: this.status.savedAt })
  }

  async save(meeting: Meeting) {
    const content = meetingContent(meeting)
    const generation = this.generation
    const revision = this.revision
    const attempt = ++this.attempt
    const current = () => generation === this.generation && attempt === this.attempt
    this.show({ state: 'saving', savedAt: this.status.savedAt })
    this.writing++
    try {
      await this.write(meeting)
      if (current()) {
        this.savedContent = content
        this.show({ state: revision === this.revision ? 'saved' : 'dirty', savedAt: meeting.updatedAt })
      }
    } catch (error) {
      if (current()) this.show({ state: 'error', error: error instanceof Error ? error.message : String(error), savedAt: this.status.savedAt })
      throw error
    } finally {
      this.writing--
    }
  }

  async flush(snapshot: () => Meeting) {
    const generation = this.generation
    let revision: number
    do {
      revision = this.revision
      await this.save(snapshot())
    } while (generation === this.generation && revision !== this.revision)
  }
}

/** Save and delete share the same queue, so a late save cannot resurrect a deleted meeting. */
export class MeetingWrites {
  private tails = new Map<string, Promise<void>>()

  run(id: string, operation: () => Promise<void>): Promise<void> {
    const result = (this.tails.get(id) ?? Promise.resolve()).catch(() => undefined).then(operation)
    this.tails.set(id, result)
    void result.finally(() => {
      if (this.tails.get(id) === result) this.tails.delete(id)
    }).catch(() => undefined)
    return result
  }
}
