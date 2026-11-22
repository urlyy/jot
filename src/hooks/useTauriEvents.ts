import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useAppStore } from '../stores/appStore'
import type {
  AppConfig,
  InsertResult,
  PipelineErrorPayload,
  PipelineState,
  RecordingDeadline,
} from '../lib/tauri'

type Unlisten = () => void | Promise<void>

function safeUnlisten(unlisten: Unlisten) {
  try {
    Promise.resolve(unlisten()).catch(() => {})
  } catch {
    /* Dev HMR 可能留下失效的 listener 句柄 */
  }
}

/**
 * 按 INTERFACE.md 订阅全部后端事件。
 * 录音中托盘/快捷键触发的状态变化只来自 pipeline:state，前端不做任何猜测。
 */
export function useTauriEvents() {
  const applyConfig = useAppStore((state) => state.applyConfig)
  const setAudioVolume = useAppStore((state) => state.setAudioVolume)
  const setPipelineState = useAppStore((state) => state.setPipelineState)
  const setRecordingDeadline = useAppStore((state) => state.setRecordingDeadline)
  const setPartialTranscript = useAppStore((state) => state.setPartialTranscript)
  const setFinalTranscript = useAppStore((state) => state.setFinalTranscript)
  const appendPolishedChunk = useAppStore((state) => state.appendPolishedChunk)
  const setPipelineError = useAppStore((state) => state.setPipelineError)
  const setInsertResult = useAppStore((state) => state.setInsertResult)
  const setHotkeyRegistrationError = useAppStore((state) => state.setHotkeyRegistrationError)
  const setAccessibilityTrusted = useAppStore((state) => state.setAccessibilityTrusted)
  const resetPipeline = useAppStore((state) => state.resetPipeline)

  useEffect(() => {
    let cancelled = false
    const unlisteners: Unlisten[] = []

    function addListener<T>(event: string, handler: (payload: T) => void) {
      listen<T>(event, (message) => handler(message.payload))
        .then((unlisten) => {
          if (cancelled) {
            safeUnlisten(unlisten)
          } else {
            unlisteners.push(unlisten)
          }
        })
        .catch((error) => {
          console.error(`Failed to register listener for "${event}":`, error)
        })
    }

    addListener<number>('audio:volume', setAudioVolume)

    addListener<PipelineState>('pipeline:state', (state) => {
      setPipelineState(state)
      if (state === 'preparing') {
        resetPipeline()
      }
      if (state === 'preparing' || state === 'idle') {
        setRecordingDeadline(null)
      }
    })

    addListener<PipelineErrorPayload>('pipeline:error', (payload) => {
      setPipelineError(payload)
      if (payload?.code === 'accessibility_required') {
        setAccessibilityTrusted(false)
      }
    })

    addListener<string>('stt:partial', setPartialTranscript)
    addListener<string>('stt:final', setFinalTranscript)
    addListener<string>('llm:chunk', appendPolishedChunk)

    addListener<RecordingDeadline>('recording:deadline', setRecordingDeadline)
    addListener<InsertResult>('pipeline:insert_result', setInsertResult)

    addListener<string>('hotkey:registration-failed', setHotkeyRegistrationError)
    addListener<null>('hotkey:registration-recovered', () => setHotkeyRegistrationError(null))

    addListener<Partial<AppConfig>>('config:patch', (patch) => {
      if (patch && typeof patch === 'object') applyConfig(patch)
    })

    addListener<string>('tray:navigate', (hash) => {
      if (typeof hash === 'string' && hash.startsWith('#')) {
        window.location.hash = hash
      }
    })

    return () => {
      cancelled = true
      unlisteners.forEach(safeUnlisten)
    }
  }, [
    applyConfig,
    setAudioVolume,
    setPipelineState,
    setRecordingDeadline,
    setPartialTranscript,
    setFinalTranscript,
    appendPolishedChunk,
    setPipelineError,
    setInsertResult,
    setHotkeyRegistrationError,
    setAccessibilityTrusted,
    resetPipeline,
  ])
}
