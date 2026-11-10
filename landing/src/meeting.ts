// The sample meeting the page tells: a short release review, with when each line was said.

export const MEETING = 12 * 60 // seconds

export const LINES = [
  { t: 30, who: 'A', say: 'Let’s list what this release needs.' },
  { t: 108, who: 'A', say: 'Run the regression tests, ship on Friday.' },
  { t: 192, who: 'B', say: 'Canary at 10%, then 50% if it holds.' },
  { t: 336, who: 'C', say: 'Not Friday. Make it next Tuesday.' },
  { t: 474, who: 'B', say: 'Star this one.' },
  { t: 624, who: 'C', say: 'Who owns the rollback plan?' },
]

/** A moment of the meeting as a clock time; it started at 14:02. */
export const clockAt = (sec: number) => {
  const m = 2 + Math.floor(sec / 60)
  return `14:${String(m).padStart(2, '0')}:${String(Math.floor(sec % 60)).padStart(2, '0')}`
}
