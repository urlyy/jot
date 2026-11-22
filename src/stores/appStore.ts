import { create } from 'zustand'
import {
  DEFAULT_CONFIG,
  formatError,
  loadConfig,
  saveConfigPatch,
  type AppConfig,
  type InsertResult,
  type PipelineErrorPayload,
  type PipelineState,
  type PlatformCapabilities,
  type RecordingDeadline,
} from '../lib/tauri'
import { toast } from '../components/toast-service'

interface AppState {
  config: AppConfig

  /** 整体替换（初次加载 / 重试） */
  setConfig: (config: AppConfig) => void
  /** 只合并局部字段（config:patch 事件、后端返回值、失败回滚） */
  applyConfig: (patch: Partial<AppConfig>) => void
  /** 即时保存：先乐观更新本地，再以 update_config 的完整返回为准 */
  updateConfig: (patch: Partial<AppConfig>) => Promise<boolean>

  pipelineState: PipelineState
  setPipelineState: (state: PipelineState) => void

  audioVolume: number
  setAudioVolume: (volume: number) => void

  recordingDeadline: RecordingDeadline | null
  setRecordingDeadline: (deadline: RecordingDeadline | null) => void

  partialTranscript: string
  setPartialTranscript: (text: string) => void

  finalTranscript: string
  setFinalTranscript: (text: string) => void

  polishedText: string
  setPolishedText: (text: string) => void
  appendPolishedChunk: (chunk: string) => void

  pipelineError: PipelineErrorPayload | null
  setPipelineError: (error: PipelineErrorPayload | null) => void

  insertResult: InsertResult | null
  setInsertResult: (result: InsertResult | null) => void

  hotkeyRegistrationError: string | null
  setHotkeyRegistrationError: (error: string | null) => void

  platformCapabilities: PlatformCapabilities | null
  setPlatformCapabilities: (capabilities: PlatformCapabilities | null) => void

  accessibilityTrusted: boolean | null
  setAccessibilityTrusted: (trusted: boolean | null) => void

  savedNotice: string | null
  showSavedNotice: (message: string) => void

  /** 新一轮录音开始时清理上一轮的展示数据 */
  resetPipeline: () => void
}

let configSaveSeq = 0
let savedNoticeTimer: number | undefined

function normalizeConfig(config: AppConfig): AppConfig {
  const merged: AppConfig = { ...DEFAULT_CONFIG, ...config }
  return {
    ...merged,
    dictionary: Array.isArray(merged.dictionary) ? merged.dictionary : [],
    streaming_interval_ms: clampNumber(merged.streaming_interval_ms, 800, 5000, 1200),
    max_recording_seconds: clampNumber(merged.max_recording_seconds, 10, 600, 30),
  }
}

function clampNumber(value: number, min: number, max: number, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback
  return Math.min(Math.max(Math.round(value), min), max)
}

export const useAppStore = create<AppState>((set, get) => ({
  config: DEFAULT_CONFIG,

  setConfig: (config) => {
    configSaveSeq += 1
    set({ config: normalizeConfig(config) })
  },

  applyConfig: (patch) => {
    // 后端推送的补丁优先于尚未返回的 update_config 响应
    configSaveSeq += 1
    set((state) => ({ config: normalizeConfig({ ...state.config, ...patch }) }))
  },

  updateConfig: async (patch) => {
    const seq = ++configSaveSeq
    set((state) => ({ config: normalizeConfig({ ...state.config, ...patch }) }))

    try {
      const full = normalizeConfig(await saveConfigPatch(patch))
      if (seq === configSaveSeq) set({ config: full })
      return true
    } catch (error) {
      if (seq === configSaveSeq) {
        toast.error(`保存失败：${formatError(error)}`)
        // 后端可能修正/拒绝了值，重新拉取一次以回到真实状态
        try {
          const current = normalizeConfig(await loadConfig())
          if (seq === configSaveSeq) set({ config: current })
        } catch {
          /* 保持乐观值，至少界面仍可用 */
        }
      }
      return false
    }
  },

  pipelineState: 'idle',
  setPipelineState: (pipelineState) => set({ pipelineState }),

  audioVolume: 0,
  setAudioVolume: (audioVolume) => set({ audioVolume }),

  recordingDeadline: null,
  setRecordingDeadline: (recordingDeadline) => set({ recordingDeadline }),

  partialTranscript: '',
  setPartialTranscript: (partialTranscript) => set({ partialTranscript }),

  finalTranscript: '',
  setFinalTranscript: (finalTranscript) => set({ finalTranscript, polishedText: '' }),

  polishedText: '',
  setPolishedText: (polishedText) => set({ polishedText }),
  appendPolishedChunk: (chunk) =>
    set((state) => {
      const next = state.polishedText + chunk
      return { polishedText: next.length > 20000 ? next.slice(-20000) : next }
    }),

  pipelineError: null,
  setPipelineError: (pipelineError) => set({ pipelineError }),

  insertResult: null,
  setInsertResult: (insertResult) => set({ insertResult }),

  hotkeyRegistrationError: null,
  setHotkeyRegistrationError: (hotkeyRegistrationError) => set({ hotkeyRegistrationError }),

  platformCapabilities: null,
  setPlatformCapabilities: (platformCapabilities) => set({ platformCapabilities }),

  accessibilityTrusted: null,
  setAccessibilityTrusted: (accessibilityTrusted) => set({ accessibilityTrusted }),

  savedNotice: null,
  showSavedNotice: (message) => {
    if (savedNoticeTimer !== undefined) window.clearTimeout(savedNoticeTimer)
    set({ savedNotice: message })
    savedNoticeTimer = window.setTimeout(() => {
      if (get().savedNotice === message) set({ savedNotice: null })
    }, 2000)
  },

  resetPipeline: () =>
    set({
      audioVolume: 0,
      recordingDeadline: null,
      partialTranscript: '',
      finalTranscript: '',
      polishedText: '',
      pipelineError: null,
      insertResult: null,
    }),
}))
