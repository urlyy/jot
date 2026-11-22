import { useEffect, useRef } from 'react'
import { useAppStore } from '../../stores/appStore'

/** 中间高、两边低的 9 根波形柱 */
const BAR_WEIGHTS = [0.32, 0.5, 0.72, 0.9, 1, 0.9, 0.72, 0.5, 0.32]
const BAR_HEIGHT = 20
const MIN_SCALE = 0.2

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value))
}

/**
 * 录音波形：把 RMS 音量做一次平方根映射（语音的 RMS 通常只有 0.02~0.2），
 * 静音时保留一条轻微的呼吸波形，说话时再明显抬起来。
 */
export function Waveform() {
  const barsRef = useRef<(HTMLSpanElement | null)[]>([])

  useEffect(() => {
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const bars = barsRef.current

    if (reduced) {
      bars.forEach((bar, index) => {
        if (bar) bar.style.transform = `scaleY(${0.3 + BAR_WEIGHTS[index] * 0.4})`
      })
      return
    }

    let frame = 0
    let level = 0
    let last = performance.now()

    const render = (now: number) => {
      const delta = Math.min(64, Math.max(0, now - last))
      last = now

      const target = clamp01(Math.sqrt(useAppStore.getState().audioVolume) * 1.7)
      // 起音要快、收尾要慢，波形才跟得上说话又不会抖
      const easing = target > level ? 0.42 : 0.1
      level += (target - level) * Math.min(1, (easing * delta) / 16)

      const time = now / 1000
      bars.forEach((bar, index) => {
        if (!bar) return
        // 静音也让波形轻轻呼吸，否则整条胶囊看起来像卡住了
        const breath = 0.24 + 0.07 * Math.sin(time * 2.2 + index * 0.75)
        const ripple = 0.08 * Math.sin(time * 7.5 + index * 1.15)
        const energy = clamp01(breath + level * 0.9 + ripple * (0.25 + level))
        const scale = MIN_SCALE + (1 - MIN_SCALE) * energy * BAR_WEIGHTS[index]
        bar.style.transform = `scaleY(${scale.toFixed(3)})`
      })

      frame = requestAnimationFrame(render)
    }

    frame = requestAnimationFrame(render)
    return () => cancelAnimationFrame(frame)
  }, [])

  return (
    <div className="flex h-5 items-center gap-[3px]" aria-hidden="true">
      {BAR_WEIGHTS.map((weight, index) => (
        <span
          key={index}
          ref={(element) => {
            barsRef.current[index] = element
          }}
          className="w-[3px] origin-center rounded-full bg-gradient-to-b from-[#c9caff] to-accent shadow-[0_0_6px_rgba(90,90,240,0.35)]"
          style={{ height: `${BAR_HEIGHT}px`, transform: `scaleY(${MIN_SCALE + weight * 0.35})` }}
        />
      ))}
    </div>
  )
}
