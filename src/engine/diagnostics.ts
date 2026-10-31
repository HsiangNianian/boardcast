// 诊断信息：把版本、状态、日志和最近的报错打成一个 zip，方便反馈问题。
// 不带会议内容、录音和 key；导出前会把每个文件的内容摆出来让人看（见 ui/DiagnosticsPanel.tsx）。
// 纯函数，方便测试。

export interface DiagFile {
  name: string
  text: string
}

/** 抹掉可能的秘密和用户名 */
export function scrub(s: string): string {
  return s
    .replace(/\bsk-[A-Za-z0-9_-]{8,}/g, '[key]')
    .replace(/\b(api[_-]?key|authorization)(["'\s:=]+)(?:Bearer\s+)?[^\s"',}]+/gi, '$1$2[key]')
    .replace(/\bBearer\s+[^\s"',}]+/gi, 'Bearer [key]')
    .replace(/(\/(?:home|Users)\/)[^/\s"']+/g, '$1[user]')
    .replace(/([A-Za-z]:\\Users\\)[^\\\s"']+/g, '$1[user]')
}

/** 日志里只留纯 ASCII 的行：正常的日志没有中文，带中文的很可能是会上说的话，宁可不要 */
export function cleanLog(log: string): string {
  const kept = log.split('\n').filter((l) => /^[\x20-\x7e\t]*$/.test(l))
  return scrub(kept.join('\n'))
}

const CRC = Array.from({ length: 256 }, (_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})

function crc32(b: Uint8Array): number {
  let c = 0xffffffff
  for (const x of b) c = CRC[(c ^ x) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}

/** 不压缩的 zip：诊断包只有几十 KB，用不着再加一个依赖 */
export function makeZip(files: DiagFile[]): Uint8Array {
  const enc = new TextEncoder()
  const u16 = (n: number) => [n & 0xff, (n >>> 8) & 0xff]
  const u32 = (n: number) => [n & 0xff, (n >>> 8) & 0xff, (n >>> 16) & 0xff, (n >>> 24) & 0xff]
  const parts: number[][] = []
  const central: number[][] = []
  let offset = 0
  for (const f of files) {
    const name = enc.encode(f.name)
    const data = enc.encode(f.text)
    const crc = crc32(data)
    // 版本 2.0，UTF-8 文件名，不压缩，时间留空
    const common = [...u16(20), ...u16(0x0800), ...u16(0), ...u16(0), ...u16(0x21), ...u32(crc), ...u32(data.length), ...u32(data.length), ...u16(name.length), ...u16(0)]
    const local = [...u32(0x04034b50), ...common, ...name, ...data]
    central.push([...u32(0x02014b50), ...u16(20), ...common, ...u16(0), ...u16(0), ...u16(0), ...u32(0), ...u32(offset), ...name])
    parts.push(local)
    offset += local.length
  }
  const dir = central.flat()
  const end = [...u32(0x06054b50), ...u16(0), ...u16(0), ...u16(files.length), ...u16(files.length), ...u32(dir.length), ...u32(offset), ...u16(0)]
  return Uint8Array.from([...parts.flat(), ...dir, ...end])
}

// —— Where the time goes ——

/** One line's stages in ms: pause waited for, recognition, voiceprint, queue for the model, model */
export interface Stages {
  wait?: number
  asr?: number
  voiceprint?: number
  queue?: number
  model?: number
}

interface TimedLine {
  ms?: number
  manual?: boolean
  queueMs?: number
  heard?: { waitMs: number; asrMs: number; voiceprintMs: number; soft: boolean }
}

export function lineTiming(l: TimedLine): Stages {
  return {
    wait: l.heard?.waitMs,
    asr: l.heard?.asrMs,
    voiceprint: l.heard?.voiceprintMs,
    queue: l.queueMs,
    model: l.ms === undefined ? undefined : Math.round(l.ms),
  }
}

const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))]
}

/**
 * p50 / p95 / max of each stage over a meeting's lines, plus the end-to-end time from the end of
 * speech to the board. Only numbers: this goes into the diagnostics zip.
 */
export function timingReport(lines: TimedLine[], model?: string) {
  const done = lines.filter((l) => !l.manual && l.ms !== undefined)
  const stages = done.map(lineTiming)
  const stat = (pick: (s: Stages) => number | undefined) => {
    const xs = stages.map(pick).filter((x): x is number => x !== undefined)
    return xs.length ? { n: xs.length, p50: pct(xs, 50), p95: pct(xs, 95), max: Math.max(...xs) } : null
  }
  const spoken = stages.filter((s) => s.wait !== undefined)
  return {
    model,
    lines: done.length,
    spoken: spoken.length,
    softBreaks: done.filter((l) => l.heard?.soft).length,
    waitMs: stat((s) => s.wait),
    asrMs: stat((s) => s.asr),
    voiceprintMs: stat((s) => s.voiceprint),
    queueMs: stat((s) => s.queue),
    modelMs: stat((s) => s.model),
    /** Spoken lines only: end of speech → on the board */
    totalMs: stat((s) => (s.wait === undefined ? undefined : (s.wait ?? 0) + (s.asr ?? 0) + (s.voiceprint ?? 0) + (s.queue ?? 0) + (s.model ?? 0))),
  }
}

// —— 最近的报错 ——

const MAX = 60
const recent: string[] = []

export function recordError(where: string, e: unknown): void {
  const msg = e instanceof Error ? (e.stack ?? e.message) : typeof e === 'string' ? e : JSON.stringify(e)
  recent.push(`${new Date().toISOString()} [${where}] ${scrub(String(msg)).slice(0, 800)}`)
  if (recent.length > MAX) recent.shift()
}

export const recentErrors = () => [...recent]

/** 接住 console.error 和没人处理的报错 */
export function captureErrors(target: { addEventListener: (type: string, fn: (e: never) => void) => void }, con: { error: (...a: unknown[]) => void }): void {
  const raw = con.error.bind(con)
  con.error = (...a: unknown[]) => {
    recordError('console', a.map((x) => (x instanceof Error ? (x.stack ?? x.message) : String(x))).join(' '))
    raw(...a)
  }
  target.addEventListener('error', (e: { message?: string }) => recordError('window', e.message ?? 'error'))
  target.addEventListener('unhandledrejection', (e: { reason?: unknown }) => recordError('promise', e.reason))
}
