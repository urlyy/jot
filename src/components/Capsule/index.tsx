import { useEffect, useRef, useState } from 'react'
import { Mic } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { useCapsuleResize, type CapsuleSize } from '../../hooks/useCapsuleResize'
import type { PipelineState } from '../../lib/tauri'
import { CapsuleRecording } from './CapsuleRecording'
import { CapsuleTranscribing } from './CapsuleTranscribing'
import { CapsulePolishing } from './CapsulePolishing'
import { CapsuleComplete } from './CapsuleComplete'
import { CapsuleError } from './CapsuleError'

type CapsulePhase =
  | 'hidden'
  | 'idle'
  | 'recording'
  | 'transcribing'
  | 'polishing'
  | 'complete'
  | 'error'

const COMPLETE_HIDE_MS = 1200
const ERROR_HIDE_MS = 3600

/** 胶囊窗口四周由 useCapsuleResize 统一留出 24px，这里的尺寸就是药丸本体尺寸 */
const BASE_SIZES: Record<Exclude<CapsulePhase, 'recording'>, CapsuleSize> = {
  hidden: { width: 40, height: 40 },
  idle: { width: 40, height: 40 },
  transcribing: { width: 320, height: 58 },
  polishing: { width: 320, height: 58 },
  complete: { width: 348, height: 46 },
  error: { width: 388, height: 48 },
}

const RECORDING_SIZE: CapsuleSize = { width: 344, height: 66 }
const RECORDING_COMPACT_SIZE: CapsuleSize = { width: 320, height: 46 }

export function Capsule() {
  const autoHide = useAppStore((state) => state.config.capsule_auto_hide)
  const streamingEnabled = useAppStore((state) => state.config.streaming_enabled)
  const pipelineState = useAppStore((state) => state.pipelineState)
  const pipelineError = useAppStore((state) => state.pipelineError)

  const [phase, setPhase] = useState<CapsulePhase>('hidden')
  const previousState = useRef<PipelineState>(pipelineState)

  // pipeline:state 是唯一的状态来源
  useEffect(() => {
    const previous = previousState.current
    previousState.current = pipelineState

    if (pipelineState === 'preparing' || pipelineState === 'recording') {
      setPhase('recording')
      return
    }
    if (pipelineState === 'transcribing') {
      setPhase('transcribing')
      return
    }
    if (pipelineState === 'polishing') {
      setPhase('polishing')
      return
    }
    if (pipelineState === 'outputting') {
      setPhase('complete')
      return
    }

    // idle：错误由单独的 effect 负责，不能在这里覆盖
    if (useAppStore.getState().pipelineError) return

    const store = useAppStore.getState()
    const hasOutput = Boolean(store.finalTranscript || store.polishedText || store.insertResult)
    if (previous !== 'idle' && hasOutput) {
      setPhase('complete')
    } else {
      setPhase((current) => (current === 'error' ? current : autoHide ? 'hidden' : 'idle'))
    }
  }, [pipelineState, autoHide])

  // 完成态停留 1.2s 后自动隐藏
  useEffect(() => {
    if (phase !== 'complete') return
    const timer = window.setTimeout(() => {
      setPhase(autoHide ? 'hidden' : 'idle')
    }, COMPLETE_HIDE_MS)
    return () => window.clearTimeout(timer)
  }, [phase, autoHide])

  // 错误态展示几秒后自动隐藏；点击胶囊可立即关闭
  useEffect(() => {
    if (!pipelineError) return
    setPhase('error')
    const timer = window.setTimeout(() => {
      const store = useAppStore.getState()
      if (store.pipelineError === pipelineError) store.setPipelineError(null)
    }, ERROR_HIDE_MS)
    return () => window.clearTimeout(timer)
  }, [pipelineError])

  useEffect(() => {
    if (pipelineError) return
    setPhase((current) => (current === 'error' ? (autoHide ? 'hidden' : 'idle') : current))
  }, [pipelineError, autoHide])

  useEffect(() => {
    if (autoHide && phase === 'idle') setPhase('hidden')
  }, [autoHide, phase])

  const effectivePhase: CapsulePhase = phase === 'hidden' && !autoHide ? 'idle' : phase
  const visible = effectivePhase !== 'hidden'

  const size: CapsuleSize =
    effectivePhase === 'recording'
      ? streamingEnabled
        ? RECORDING_SIZE
        : RECORDING_COMPACT_SIZE
      : BASE_SIZES[effectivePhase]

  useCapsuleResize(visible, size)

  if (!visible) {
    return <div className="h-full w-full" />
  }

  const isError = effectivePhase === 'error'
  const isIdle = effectivePhase === 'idle'

  return (
    <div className="flex h-full w-full select-none items-center justify-center p-3">
      <div
        key={effectivePhase}
        role={isError ? 'alert' : 'status'}
        aria-live={effectivePhase === 'recording' ? 'off' : 'polite'}
        className={`capsule-in relative flex shrink-0 items-center justify-center overflow-hidden rounded-full border text-white backdrop-blur-2xl ${
          isError
            ? 'border-[#f0a36b]/40 bg-[#201512]/95 shadow-[0_4px_14px_-6px_rgba(0,0,0,0.8)]'
            : 'border-white/[0.12] bg-capsule-base/95 shadow-[0_4px_14px_-6px_rgba(6,6,14,0.8)]'
        }`}
        style={{ width: size.width, height: size.height }}
      >
        {/* 内层高光 + 一圈极淡的内描边，让药丸像磨砂玻璃而不是纯色块 */}
        <span
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 top-0 h-1/2 bg-gradient-to-b from-white/[0.12] to-transparent"
        />
        <span
          aria-hidden="true"
          className={`pointer-events-none absolute inset-0 rounded-full ring-1 ring-inset ${
            isError ? 'ring-[#f0a36b]/15' : 'ring-white/[0.05]'
          }`}
        />

        {isIdle && (
          <div className="relative flex h-full w-full items-center justify-center text-white/75">
            <Mic size={16} strokeWidth={2.1} aria-hidden="true" />
          </div>
        )}

        {effectivePhase === 'recording' && <CapsuleRecording />}
        {effectivePhase === 'transcribing' && <CapsuleTranscribing />}
        {effectivePhase === 'polishing' && <CapsulePolishing />}
        {effectivePhase === 'complete' && <CapsuleComplete />}
        {isError && pipelineError && (
          <CapsuleError
            error={pipelineError}
            onClose={() => useAppStore.getState().setPipelineError(null)}
          />
        )}
      </div>
    </div>
  )
}
