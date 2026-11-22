import { useEffect, useState } from 'react'
import { Keyboard, RotateCcw } from 'lucide-react'
import { formatHotkey } from '../../../lib/platform'
import { pauseHotkey, resumeHotkey, type PlatformOs } from '../../../lib/tauri'

interface Props {
  value: string
  os: PlatformOs
  disabled?: boolean
  /** 返回 Promise 时，录制器会等注册完成后再恢复全局快捷键 */
  onChange: (hotkey: string) => Promise<void> | void
}

const KEY_BY_CODE: Record<string, string> = {
  Space: 'Space',
  Enter: 'Enter',
  NumpadEnter: 'Enter',
  Tab: 'Tab',
  Escape: 'Escape',
  Slash: '/',
  Period: '.',
  Comma: ',',
  Semicolon: ';',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
}

function modifiersFromEvent(event: KeyboardEvent): string[] {
  const modifiers: string[] = []
  if (event.ctrlKey) modifiers.push('Ctrl')
  if (event.altKey) modifiers.push('Alt')
  if (event.shiftKey) modifiers.push('Shift')
  if (event.metaKey) modifiers.push('Super')
  return modifiers
}

/** 把按键归一成契约允许的主键；只按修饰键时返回 null */
function mainKeyFromEvent(event: KeyboardEvent): string | null {
  const code = event.code
  if (/^Key[A-Z]$/.test(code)) return code.slice(3)
  if (/^Digit[0-9]$/.test(code)) return code.slice(5)
  if (/^F([1-9]|1[0-2])$/.test(code)) return code
  if (KEY_BY_CODE[code]) return KEY_BY_CODE[code]

  const key = event.key
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase()
  if (/^[0-9]$/.test(key)) return key
  if (/^F([1-9]|1[0-2])$/.test(key)) return key
  if (key === ' ') return 'Space'
  if (key === 'Enter') return 'Enter'
  if (key === 'Tab') return 'Tab'
  if (key === 'Escape') return 'Escape'
  if (['/', '.', ',', ';', '-', '=', '[', ']'].includes(key)) return key
  return null
}

export function HotkeyRecorder({ value, os, disabled = false, onChange }: Props) {
  const [recording, setRecording] = useState(false)
  const [preview, setPreview] = useState('')
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!recording) return

    let paused = false
    let disposed = false
    pauseHotkey()
      .then(() => {
        paused = true
        if (disposed) {
          paused = false
          void resumeHotkey().catch(() => {})
        }
      })
      .catch(() => {})

    const finish = () => {
      setRecording(false)
      setPreview('')
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault()
      event.stopPropagation()
      if (event.repeat) return

      const modifiers = modifiersFromEvent(event)
      const mainKey = mainKeyFromEvent(event)

      // 无修饰键的 Esc = 取消录制
      if (event.key === 'Escape' && modifiers.length === 0) {
        finish()
        return
      }

      if (!mainKey) {
        setPreview(modifiers.length > 0 ? `${modifiers.join('+')}+…` : '')
        return
      }

      if (modifiers.length === 0) {
        setError('至少需要一个修饰键，例如 Ctrl+A')
        return
      }

      const hotkey = [...modifiers, mainKey].join('+')
      setError(null)
      finish()
      void Promise.resolve(onChange(hotkey)).catch(() => {})
    }

    window.addEventListener('keydown', handleKeyDown, true)

    return () => {
      disposed = true
      window.removeEventListener('keydown', handleKeyDown, true)
      if (paused) void resumeHotkey().catch(() => {})
    }
  }, [recording, onChange])

  const startRecording = () => {
    if (disabled) return
    setError(null)
    setPreview('')
    setRecording(true)
  }

  const display = recording
    ? preview
      ? formatHotkey(preview, os)
      : '请按下组合键…'
    : formatHotkey(value, os)

  return (
    <div>
      <div className="flex items-center gap-2">
        <button
          type="button"
          disabled={disabled}
          onClick={() => (recording ? setRecording(false) : startRecording())}
          aria-label={recording ? '取消录制快捷键' : '录制快捷键'}
          className={`flex h-[38px] min-w-[210px] items-center gap-2 rounded-[10px] border px-3 text-left text-[13px] outline-none transition-[border-color,background-color,box-shadow,transform] duration-150 focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent active:translate-y-px disabled:cursor-not-allowed disabled:opacity-50 disabled:active:translate-y-0 ${
            recording
              ? 'border-accent/60 bg-accent-light/60 text-accent-text shadow-[0_0_0_3px_rgba(90,90,240,0.12)]'
              : 'border-border bg-bg-elevated text-text-primary hover:border-text-tertiary/50'
          }`}
        >
          {recording ? (
            <span className="relative flex h-2 w-2 shrink-0" aria-hidden="true">
              <span className="absolute inset-0 animate-ping rounded-full bg-rec/50" />
              <span className="relative h-2 w-2 rounded-full bg-rec" />
            </span>
          ) : (
            <Keyboard size={14} className="shrink-0 text-text-tertiary" aria-hidden="true" />
          )}
          <span className="truncate" aria-live="polite">
            {display}
          </span>
        </button>
        <button
          type="button"
          disabled={disabled}
          title="清除并重新录制"
          aria-label="清除并重新录制快捷键"
          onClick={startRecording}
          className="flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-[10px] border border-border bg-bg-elevated text-text-secondary outline-none transition-[color,background-color,border-color] duration-150 hover:bg-bg-secondary hover:text-text-primary focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent active:translate-y-px disabled:cursor-not-allowed disabled:opacity-50 disabled:active:translate-y-0"
        >
          <RotateCcw size={14} />
        </button>
      </div>
      {error && (
        <p role="alert" className="mt-1.5 text-[11px] leading-relaxed text-error">
          {error}
        </p>
      )}
      {recording && (
        <p className="mt-1.5 text-[11px] text-text-tertiary">
          按 Esc 取消 · 组合键需包含至少一个修饰键
        </p>
      )}
    </div>
  )
}
