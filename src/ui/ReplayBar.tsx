// 回放时底部的控制条：播放 / 暂停、进度（可拖）、时间、倍速、从这一刻另起一板、退出。占输入栏的位置。

import type { CSSProperties } from 'react'
import { useT } from '../i18n/index.ts'
import { Close, Fork, Pause, Play } from './icons.tsx'

export const SPEEDS = [1, 2, 4] as const

/** 毫秒 → 「3:07」「1:02:45」 */
export const clock = (ms: number) => {
  const s = Math.max(0, Math.round(ms / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const pad = (n: number) => String(n).padStart(2, '0')
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`
}

export default function ReplayBar(p: {
  /** 已经放到第几步（0 = 空板） */
  at: number
  total: number
  playing: boolean
  speed: number
  /** 这一步离会议开始多久、整场多长（毫秒） */
  elapsed: number
  duration: number
  onToggle: () => void
  onSeek: (at: number) => void
  onSpeed: () => void
  /** 从这一刻另起一张板；不给就没有这个按钮（导出的回放网页） */
  onFork?: () => void
  onExit: () => void
}) {
  const s = useT().replayBar
  return (
    <div className="dock replay-dock" role="group" aria-label={s.aria}>
      <div className="bar">
        <button type="button" className="icon" onClick={p.onToggle} aria-label={p.playing ? s.pause : s.play} title={p.playing ? s.pauseTitle : s.playTitle}>
          {p.playing ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <input
          type="range"
          min={0}
          max={p.total}
          value={p.at}
          onChange={(e) => p.onSeek(Number(e.target.value))}
          aria-label={s.progress}
          style={{ '--fill': `${p.total ? (100 * p.at) / p.total : 0}%` } as CSSProperties}
        />
        <span className="time">
          {clock(p.elapsed)} / {clock(p.duration)}
        </span>
        <button type="button" className="speed" onClick={p.onSpeed} title={s.speed}>
          {p.speed}×
        </button>
        {p.onFork && (
          <button type="button" className="fork" onClick={p.onFork} disabled={!p.at} aria-label={s.fork} title={p.at ? s.forkTitle : s.forkEmpty}>
            <Fork size={15} />
            <span>{s.fork}</span>
          </button>
        )}
        <button type="button" className="icon" onClick={p.onExit} aria-label={s.exit} title={s.exitTitle}>
          <Close size={16} />
        </button>
      </div>
    </div>
  )
}
