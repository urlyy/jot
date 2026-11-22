import type { PasteShortcut, PlatformOs } from './tauri'

export function isMac(os: PlatformOs): boolean {
  return os === 'macos'
}

const MAC_MODIFIERS: Record<string, string> = {
  ctrl: '⌃',
  control: '⌃',
  alt: '⌥',
  option: '⌥',
  shift: '⇧',
  super: '⌘',
  meta: '⌘',
  cmd: '⌘',
  command: '⌘',
}

const OTHER_MODIFIERS: Record<string, string> = {
  ctrl: 'Ctrl',
  control: 'Ctrl',
  alt: 'Alt',
  option: 'Alt',
  shift: 'Shift',
  super: 'Win',
  meta: 'Win',
  cmd: 'Win',
  command: 'Win',
}

function modifierLabel(part: string, os: PlatformOs): string {
  const table = isMac(os) ? MAC_MODIFIERS : OTHER_MODIFIERS
  return table[part.toLowerCase()] ?? part
}

/**
 * 把契约里的 accelerator（如 `Super+Shift+Space`）转成平台友好的展示文本：
 * macOS 用 ⌃⌥⇧⌘，其他平台用 Ctrl/Alt/Shift/Win。
 */
export function formatHotkey(hotkey: string, os: PlatformOs): string {
  const trimmed = hotkey.trim()
  if (!trimmed) return '未设置'
  const parts = trimmed
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)
  if (parts.length === 0) return '未设置'
  const labels = parts.map((part) => modifierLabel(part, os))
  return isMac(os) ? labels.join(' ') : labels.join('+')
}

export function defaultPasteShortcut(os: PlatformOs): PasteShortcut {
  return isMac(os) ? 'cmdV' : 'ctrlV'
}

export interface PasteShortcutOption {
  value: PasteShortcut
  label: string
}

/** 按平台给出粘贴快捷键选项与文案 */
export function pasteShortcutOptions(os: PlatformOs): PasteShortcutOption[] {
  if (isMac(os)) {
    return [
      { value: 'cmdV', label: '⌘V' },
      { value: 'ctrlV', label: '⌃V' },
      { value: 'ctrlShiftV', label: '⌃⇧V' },
      { value: 'shiftInsert', label: '⇧Insert' },
    ]
  }
  return [
    { value: 'ctrlV', label: 'Ctrl+V' },
    { value: 'ctrlShiftV', label: 'Ctrl+Shift+V' },
    { value: 'shiftInsert', label: 'Shift+Insert' },
  ]
}

export function pasteShortcutLabel(value: PasteShortcut, os: PlatformOs): string {
  return pasteShortcutOptions(os).find((option) => option.value === value)?.label ?? value
}

/** 截取文本末尾一段，用于胶囊里避免整行抖动 */
export function tailText(text: string, max = 40): string {
  const clean = text.replace(/\s+/g, ' ').trim()
  if (clean.length <= max) return clean
  return `…${clean.slice(-max)}`
}
