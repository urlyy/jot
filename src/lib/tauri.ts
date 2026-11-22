import { invoke } from '@tauri-apps/api/core'

/** 主题：浅色 / 深色 / 跟随系统 */
export type Theme = 'light' | 'dark' | 'system'

/** 快捷键模式：按住说话 / 按一下开始 */
export type HotkeyMode = 'hold' | 'toggle'

/** 输出方式：直接键入 / 复制到剪贴板 */
export type OutputMode = 'keyboard' | 'clipboard'

/** 粘贴快捷键（契约值；macOS 默认 cmdV，其余平台默认 ctrlV） */
export type PasteShortcut = 'ctrlV' | 'ctrlShiftV' | 'shiftInsert' | 'cmdV'

/** 润色风格 */
export type PolishStyle = 'minimal' | 'clean' | 'structured' | 'custom'

/** 转写供应商：OpenAI 兼容批式 / 火山引擎原生流式 */
export type SttProvider = 'openai' | 'volcengine'

/** 火山引擎流式识别 Resource ID */
export type VolcResourceId = 'volc.seedasr.sauc.duration' | 'volc.bigasr.sauc.duration'

/** 平台标识，来自 get_platform_capabilities */
export type PlatformOs = 'windows' | 'macos' | 'linux' | 'unknown'

/**
 * 精简版配置。字段顺序、名称、类型严格遵循 INTERFACE.md，
 * 任何 invoke 的 patch 也只能使用这里的字段名。
 */
export interface AppConfig {
  hotkey: string
  hotkey_mode: HotkeyMode
  max_recording_seconds: number

  stt_provider: SttProvider
  stt_volc_resource_id: VolcResourceId
  stt_base_url: string
  stt_model: string
  stt_language: string

  polish_enabled: boolean
  llm_base_url: string
  llm_model: string
  polish_style: PolishStyle
  polish_custom_prompt: string
  /** 关闭推理模型的思考阶段（reasoning_effort=none），本地模型能快一个数量级 */
  llm_disable_thinking: boolean

  dictionary: string[]

  streaming_enabled: boolean
  streaming_insert: boolean
  streaming_interval_ms: number

  output_mode: OutputMode
  paste_shortcut: PasteShortcut
  restore_clipboard_after_paste: boolean

  theme: Theme
  auto_start: boolean
  close_to_tray: boolean
  start_minimized: boolean
  ui_language: string
  capsule_auto_hide: boolean
}

export const DEFAULT_CONFIG: AppConfig = {
  hotkey: 'Ctrl+/',
  hotkey_mode: 'hold',
  max_recording_seconds: 30,

  stt_provider: 'openai',
  stt_volc_resource_id: 'volc.seedasr.sauc.duration',
  stt_base_url: '',
  stt_model: 'whisper-1',
  stt_language: 'auto',

  polish_enabled: true,
  llm_base_url: 'http://127.0.0.1:11434/v1',
  llm_model: 'qwen3:1.7b',
  polish_style: 'clean',
  polish_custom_prompt: '',
  llm_disable_thinking: true,

  dictionary: [],

  streaming_enabled: true,
  streaming_insert: false,
  streaming_interval_ms: 1200,

  output_mode: 'keyboard',
  paste_shortcut: 'ctrlV',
  restore_clipboard_after_paste: true,

  theme: 'system',
  auto_start: true,
  close_to_tray: true,
  start_minimized: false,
  ui_language: 'zh',
  capsule_auto_hide: true,
}

/** 管线状态，完全由后端 pipeline:state 事件驱动 */
export type PipelineState =
  | 'idle'
  | 'preparing'
  | 'recording'
  | 'transcribing'
  | 'polishing'
  | 'outputting'

export interface PipelineErrorPayload {
  code: string
  details?: string
}

export interface RecordingDeadline {
  sessionId: number
  startedAtUnixMs: number
  deadlineAtUnixMs: number
  effectiveMaxSeconds: number
}

export type InsertStatus = 'inserted' | 'copiedFallback' | 'failed' | 'partiallyInserted'

export interface InsertResult {
  status: InsertStatus
  strategyUsed: string
  charsInserted: number
  charsCopied: number
  warningCode: string | null
  message: string | null
}

export interface PlatformCapabilities {
  os: PlatformOs
  sessionType: string
  globalHotkeyReliable: boolean
  keyboardOutputReliable: boolean
  clipboardAutoPasteReliable: boolean
}

export interface AppInfo {
  version: string
  os: string
}

export interface CredentialStatus {
  stt: boolean
  llm: boolean
}

export type CredentialNamespace = 'stt' | 'llm'
export type CredentialProvider = 'custom' | 'volcengine'

/* ───────────────────────── 配置 ───────────────────────── */

export function loadConfig(): Promise<AppConfig> {
  return invoke<AppConfig>('get_config')
}

export function getConfig(): Promise<AppConfig> {
  return loadConfig()
}

/** 局部 patch，后端深合并后返回完整配置 */
export function saveConfigPatch(patch: Partial<AppConfig>): Promise<AppConfig> {
  return invoke<AppConfig>('update_config', { patch })
}

export function setAutoStart(enabled: boolean): Promise<AppConfig> {
  return invoke<AppConfig>('set_auto_start', { enabled })
}

export function setCapsuleAutoHide(enabled: boolean): Promise<AppConfig> {
  return invoke<AppConfig>('set_capsule_auto_hide', { enabled })
}

/* ───────────────────────── 应用信息 ───────────────────────── */

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>('get_app_info')
}

/* ───────────────────────── 钥匙串 ───────────────────────── */

export function readCredential(
  namespace: CredentialNamespace,
  provider: CredentialProvider = 'custom',
): Promise<string | null> {
  return invoke<string | null>('read_credential', { namespace, provider })
}

export function setCredential(
  namespace: CredentialNamespace,
  secret: string,
  provider: CredentialProvider = 'custom',
): Promise<void> {
  return invoke<void>('set_credential', { namespace, provider, secret })
}

export function clearCredential(
  namespace: CredentialNamespace,
  provider: CredentialProvider = 'custom',
): Promise<void> {
  return invoke<void>('clear_credential', { namespace, provider })
}

export function getCredentialStatus(): Promise<CredentialStatus> {
  return invoke<CredentialStatus>('get_credential_status')
}

/* ───────────────────────── 连接测试 ───────────────────────── */

export function testSttConnection(
  apiKey: string,
  baseUrl: string,
  model: string,
  language: string,
  provider: SttProvider = 'openai',
  resourceId = '',
): Promise<number> {
  return invoke<number>('test_stt_connection', {
    apiKey,
    baseUrl,
    model,
    language,
    provider,
    resourceId,
  })
}

export function benchLlmConnection(
  apiKey: string,
  baseUrl: string,
  model: string,
  disableThinking: boolean,
): Promise<number> {
  return invoke<number>('bench_llm_connection', { apiKey, baseUrl, model, disableThinking })
}

export function fetchLlmModels(apiKey: string, baseUrl: string): Promise<string[]> {
  return invoke<string[]>('fetch_llm_models', { apiKey, baseUrl })
}

/* ───────────────────────── 快捷键 ───────────────────────── */

/** 注册失败时 reject，错误文本由后端给出可读原因 */
export function applyHotkey(hotkey: string): Promise<void> {
  return invoke<void>('apply_hotkey', { hotkey })
}

export function pauseHotkey(): Promise<void> {
  return invoke<void>('pause_hotkey')
}

export function resumeHotkey(): Promise<void> {
  return invoke<void>('resume_hotkey')
}

export function getHotkeyRegistrationError(): Promise<string | null> {
  return invoke<string | null>('get_hotkey_registration_error')
}

/* ───────────────────────── 平台能力 ───────────────────────── */

export function getPlatformCapabilities(): Promise<PlatformCapabilities> {
  return invoke<PlatformCapabilities>('get_platform_capabilities')
}

export function checkAccessibilityPermission(): Promise<boolean> {
  return invoke<boolean>('check_accessibility_permission')
}

export function requestAccessibilityPermission(): Promise<boolean> {
  return invoke<boolean>('request_accessibility_permission')
}

/* ───────────────────────── 录音管线 ───────────────────────── */

export function startRecording(): Promise<void> {
  return invoke<void>('start_recording')
}

export function stopRecording(): Promise<void> {
  return invoke<void>('stop_recording')
}

export function abortRecording(): Promise<void> {
  return invoke<void>('abort_recording')
}

/* ───────────────────────── 工具 ───────────────────────── */

/** 把 Tauri 的 reject 值转成可直接展示的中文/原始错误文本 */
export function formatError(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  if (error && typeof error === 'object') {
    const record = error as Record<string, unknown>
    if (typeof record.message === 'string') return record.message
    if (typeof record.error === 'string') return record.error
  }
  try {
    return JSON.stringify(error)
  } catch {
    return String(error)
  }
}
