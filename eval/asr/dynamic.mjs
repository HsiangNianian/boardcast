import { errorRate } from './score.mjs'

/** Paired runs use identical input bytes; reference text is only consulted after inference. */
export function comparison(sample, actual) {
  const baseline = actual.baseline
  if (!baseline || baseline.duration !== actual.duration) throw new Error(`${sample.id}: missing paired baseline`)
  const primary = new Map()
  for (const s of actual.trace) if (!primary.has(s.id)) primary.set(s.id, s)
  const first = [...primary.values()]
  if (first.length !== baseline.segments.length || first.some((s, i) => s.start !== baseline.segments[i].start
    || s.end !== baseline.segments[i].end || s.text !== baseline.segments[i].text)) {
    throw new Error(`${sample.id}: dynamic first pass differs from the static control`)
  }
  if (new Set(actual.segments.map((s) => s.id)).size !== actual.segments.length) throw new Error(`${sample.id}: duplicate final segment`)
  const reviews = actual.segments.filter((s) => s.review).map((s) => ({ id: s.id, start: s.start, end: s.end, quality: s.quality, ...s.review }))
  for (const r of reviews) {
    if (!['accepted', 'retained', 'unchanged', 'expired', 'queue_full', 'too_long'].includes(r.decision)
      || !Number.isFinite(r.asrMs) || r.asrMs < 0) throw new Error(`${sample.id}: incomplete review`)
    const events = actual.trace.filter((s) => s.id === r.id)
    if (events.length > 2 || (r.asrMs > 0 && r.end - r.start > 12.001)) throw new Error(`${sample.id}: retry budget exceeded`)
  }
  const text = (segments) => segments.map((s) => s.text).join(' ').trim()
  const reference = sample.reference.map((s) => s.text).join(' ')
  const scored = ['reference', 'provisional'].includes(sample.scoring)
  const before = scored ? errorRate(reference, text(baseline.segments), sample.language) : null
  const after = scored ? errorRate(reference, text(actual.segments), sample.language) : null
  return { hypothesis: text(baseline.segments), score: before, errorDelta: before ? after.errors - before.errors : null,
    elapsedMs: baseline.elapsedMs, extraMs: actual.elapsedMs - baseline.elapsedMs,
    retryMs: reviews.reduce((sum, r) => sum + r.asrMs, 0),
    attempts: reviews.filter((r) => ['accepted', 'retained', 'unchanged'].includes(r.decision)).length,
    accepted: reviews.filter((r) => r.decision === 'accepted').length, reviews }
}
