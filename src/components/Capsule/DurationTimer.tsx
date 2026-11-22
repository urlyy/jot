import { useEffect, useRef, useState } from 'react'
import { useAppStore } from '../../stores/appStore'

function formatDuration(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds))
  const minutes = String(Math.floor(safe / 60)).padStart(2, '0')
  const seconds = String(safe % 60).padStart(2, '0')
  return `${minutes}:${seconds}`
}

export function DurationTimer() {
  const recordingDeadline = useAppStore((state) => state.recordingDeadline)
  const fallbackStartedAt = useRef(Date.now())
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 250)
    return () => window.clearInterval(timer)
  }, [])

  const startedAt = recordingDeadline?.startedAtUnixMs ?? fallbackStartedAt.current
  const displayNow = recordingDeadline ? Math.min(now, recordingDeadline.deadlineAtUnixMs) : now
  const seconds = (displayNow - startedAt) / 1000

  return (
    <span
      className="shrink-0 font-mono text-[11px] font-medium tabular-nums tracking-[0.02em] text-white/75"
      aria-live="off"
    >
      {formatDuration(seconds)}
    </span>
  )
}
