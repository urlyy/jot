import { useCallback, useEffect, useState } from 'react'
import { Accessibility, AlertTriangle, Info } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { toast } from '../../components/toast-service'
import {
  applyHotkey,
  checkAccessibilityPermission,
  formatError,
  requestAccessibilityPermission,
  setAutoStart,
  setCapsuleAutoHide,
  type PasteShortcut,
  type PlatformOs,
  type Theme,
} from '../../lib/tauri'
import { pasteShortcutOptions } from '../../lib/platform'
import { FormField } from './shared/FormField'
import { HotkeyRecorder } from './shared/HotkeyRecorder'
import { SegmentedControl } from './shared/SegmentedControl'
import { Toggle } from './shared/Toggle'
import {
  bannerClass,
  rowClass,
  rowHintClass,
  rowLabelClass,
  sectionClass,
  sectionHintClass,
  sectionTitleClass,
  selectClass,
} from './shared/ui'

const THEME_OPTIONS = [
  { value: 'light', label: '浅色' },
  { value: 'dark', label: '深色' },
  { value: 'system', label: '跟随系统' },
]

const HOTKEY_MODE_OPTIONS = [
  { value: 'hold', label: '按住说话' },
  { value: 'toggle', label: '按一下开始' },
]

const OUTPUT_MODE_OPTIONS = [
  { value: 'keyboard', label: '直接键入' },
  { value: 'clipboard', label: '复制到剪贴板' },
]

export function GeneralPane() {
  const config = useAppStore((state) => state.config)
  const capabilities = useAppStore((state) => state.platformCapabilities)
  const accessibilityTrusted = useAppStore((state) => state.accessibilityTrusted)
  const setAccessibilityTrusted = useAppStore((state) => state.setAccessibilityTrusted)
  const hotkeyRegistrationError = useAppStore((state) => state.hotkeyRegistrationError)
  const updateConfig = useAppStore((state) => state.updateConfig)
  const applyConfig = useAppStore((state) => state.applyConfig)

  const os: PlatformOs = capabilities?.os ?? 'unknown'
  const isMac = os === 'macos'

  const [intervalDraft, setIntervalDraft] = useState(String(config.streaming_interval_ms))

  useEffect(() => {
    setIntervalDraft(String(config.streaming_interval_ms))
  }, [config.streaming_interval_ms])

  // macOS 上如果仍无辅助功能权限，进入设置时再确认一次
  useEffect(() => {
    if (!isMac) return
    let cancelled = false
    checkAccessibilityPermission()
      .then((trusted) => {
        if (!cancelled) setAccessibilityTrusted(trusted)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [isMac, setAccessibilityTrusted])

  const handleHotkeyChange = useCallback(
    async (nextHotkey: string) => {
      const previousHotkey = config.hotkey
      if (nextHotkey === previousHotkey) return

      try {
        await applyHotkey(nextHotkey)
      } catch (error) {
        toast.error(formatError(error))
        applyConfig({ hotkey: previousHotkey })
        try {
          await applyHotkey(previousHotkey)
        } catch {
          /* 旧快捷键恢复失败时保留后端当前的注册状态 */
        }
        return
      }

      const saved = await updateConfig({ hotkey: nextHotkey })
      if (!saved) {
        // update_config 已提示错误；按产品要求回滚显示与注册，避免界面显示一个不可用的快捷键
        applyConfig({ hotkey: previousHotkey })
        try {
          await applyHotkey(previousHotkey)
        } catch {
          /* ignore */
        }
      }
    },
    [applyConfig, config.hotkey, updateConfig],
  )

  const handleAutoStart = async (enabled: boolean) => {
    applyConfig({ auto_start: enabled })
    try {
      const updated = await setAutoStart(enabled)
      applyConfig(updated)
    } catch (error) {
      toast.error(`设置开机启动失败：${formatError(error)}`)
      applyConfig({ auto_start: !enabled })
    }
  }

  const handleCapsuleAutoHide = async (enabled: boolean) => {
    applyConfig({ capsule_auto_hide: enabled })
    try {
      const updated = await setCapsuleAutoHide(enabled)
      applyConfig(updated)
    } catch (error) {
      toast.error(`设置胶囊自动隐藏失败：${formatError(error)}`)
      applyConfig({ capsule_auto_hide: !enabled })
    }
  }

  const handleRequestAccessibility = async () => {
    try {
      const granted = await requestAccessibilityPermission()
      setAccessibilityTrusted(granted)
      if (!granted) {
        toast('请在当前系统的辅助功能设置中允许 Jot', 'info')
      }
    } catch (error) {
      toast.error(formatError(error))
    }
  }

  const handleIntervalChange = (value: string) => {
    setIntervalDraft(value)
    const parsed = Number(value)
    if (Number.isInteger(parsed) && parsed >= 800 && parsed <= 5000) {
      void updateConfig({ streaming_interval_ms: parsed })
    }
  }

  const handleIntervalBlur = () => {
    const parsed = Number(intervalDraft)
    const clamped = Number.isFinite(parsed)
      ? Math.min(Math.max(Math.round(parsed), 800), 5000)
      : config.streaming_interval_ms
    setIntervalDraft(String(clamped))
    if (clamped !== config.streaming_interval_ms) {
      void updateConfig({ streaming_interval_ms: clamped })
    }
  }

  const pasteOptions = pasteShortcutOptions(os)
  if (!pasteOptions.some((option) => option.value === config.paste_shortcut)) {
    pasteOptions.push({
      value: config.paste_shortcut,
      label: config.paste_shortcut === 'cmdV' ? '⌘V' : config.paste_shortcut,
    })
  }

  const outputFallbackHints: string[] = []
  if (capabilities && !capabilities.keyboardOutputReliable) {
    outputFallbackHints.push('当前系统的键盘输入不可靠，文字会自动回退为复制到剪贴板。')
  }
  if (capabilities && !capabilities.clipboardAutoPasteReliable) {
    outputFallbackHints.push('当前系统的自动粘贴不可靠，文字会留在剪贴板中供手动粘贴。')
  }

  return (
    <div>
      {isMac && accessibilityTrusted === false && (
        <div className={`${bannerClass} mb-6`}>
          <Accessibility size={16} className="mt-0.5 shrink-0 text-warning" />
          <div className="flex-1">
            <p className="font-medium text-text-primary">需要辅助功能权限</p>
            <p className="mt-1 text-text-secondary">
              没有授权时，Jot 无法把文字输入到其他应用。授权后请回到这里确认状态。
            </p>
          </div>
          <button
            type="button"
            onClick={handleRequestAccessibility}
            className="shrink-0 cursor-pointer rounded-[8px] border border-border bg-bg-elevated px-3 py-1.5 text-[12px] text-text-primary transition-colors hover:bg-bg-secondary"
          >
            去授权
          </button>
        </div>
      )}

      <section className={sectionClass}>
        <h3 className={sectionTitleClass}>快捷键</h3>
        <p className={sectionHintClass}>按住或按一下快捷键，随时开始语音输入。</p>

        <div className="mt-4">
          <FormField label="全局快捷键" hint="点击输入框后按下组合键即可录制；Esc 取消。">
            <HotkeyRecorder value={config.hotkey} os={os} onChange={handleHotkeyChange} />
          </FormField>
          {hotkeyRegistrationError && (
            <p className="mt-2 flex items-start gap-1.5 text-[11px] leading-relaxed text-error">
              <AlertTriangle size={12} className="mt-0.5 shrink-0" />
              快捷键注册失败：{hotkeyRegistrationError}
            </p>
          )}
        </div>

        <div className={`${rowClass} mt-3`}>
          <div>
            <div className={rowLabelClass}>触发方式</div>
            <p className={rowHintClass}>按住说话更接近对讲机；按一下开始适合长句。</p>
          </div>
          <div className="w-[220px] shrink-0">
            <SegmentedControl
              options={HOTKEY_MODE_OPTIONS}
              ariaLabel="触发方式"
              value={config.hotkey_mode}
              onChange={(value) =>
                void updateConfig({ hotkey_mode: value === 'toggle' ? 'toggle' : 'hold' })
              }
            />
          </div>
        </div>
      </section>

      <section className={sectionClass}>
        <h3 className={sectionTitleClass}>录音</h3>
        <p className={sectionHintClass}>控制单次录音的时长与流式听写行为。</p>

        <div className={rowClass}>
          <div className="flex-1">
            <div className={rowLabelClass}>录音最长秒数</div>
            <p className={rowHintClass}>到达上限后会自动结束并开始转写。</p>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <input
              type="range"
              aria-label="录音最长秒数"
              min={10}
              max={600}
              step={5}
              value={config.max_recording_seconds}
              onChange={(event) =>
                void updateConfig({ max_recording_seconds: Number(event.target.value) })
              }
              className="w-[170px] cursor-pointer accent-accent outline-none focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
            />
            <span className="w-[46px] text-right text-[12px] tabular-nums text-text-secondary">
              {config.max_recording_seconds} 秒
            </span>
          </div>
        </div>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>流式听写</div>
            <p className={rowHintClass}>录音时边录边显示识别结果。</p>
          </div>
          <Toggle
            checked={config.streaming_enabled}
            ariaLabel="流式听写"
            onChange={(checked) => void updateConfig({ streaming_enabled: checked })}
          />
        </div>

        <div className={rowClass}>
          <div className={config.streaming_enabled ? '' : 'opacity-50'}>
            <div className={rowLabelClass}>边说边打字</div>
            <p className={rowHintClass}>
              开启后文字会随说随打进光标处；关闭时松手后一次性插入，最终结果更准。
              {config.polish_enabled && ' 开启此选项时会跳过 LLM 润色。'}
            </p>
          </div>
          <Toggle
            checked={config.streaming_insert}
            ariaLabel="边说边打字"
            disabled={!config.streaming_enabled}
            onChange={(checked) => void updateConfig({ streaming_insert: checked })}
          />
        </div>

        <div className={rowClass}>
          <div className={config.streaming_enabled ? '' : 'opacity-50'}>
            <div className={rowLabelClass}>分段间隔</div>
            <p className={rowHintClass}>越小越灵敏，越大越省请求。</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <input
              type="number"
              aria-label="分段间隔（毫秒）"
              min={800}
              max={5000}
              step={100}
              value={intervalDraft}
              disabled={!config.streaming_enabled}
              onChange={(event) => handleIntervalChange(event.target.value)}
              onBlur={handleIntervalBlur}
              className="w-[104px] rounded-[10px] border border-border bg-bg-elevated px-3 py-2 text-right text-[13px] tabular-nums text-text-primary outline-none transition-colors hover:border-text-tertiary/40 focus:border-border-focus focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:border-border"
            />
            <span className="text-[12px] text-text-secondary">毫秒</span>
          </div>
        </div>
      </section>

      <section className={sectionClass}>
        <h3 className={sectionTitleClass}>输出</h3>
        <p className={sectionHintClass}>决定文字如何进入当前应用。</p>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>输出方式</div>
            <p className={rowHintClass}>直接键入失败时会自动回退到剪贴板。</p>
          </div>
          <div className="w-[240px] shrink-0">
            <SegmentedControl
              options={OUTPUT_MODE_OPTIONS}
              ariaLabel="输出方式"
              value={config.output_mode}
              onChange={(value) =>
                void updateConfig({ output_mode: value === 'clipboard' ? 'clipboard' : 'keyboard' })
              }
            />
          </div>
        </div>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>粘贴快捷键</div>
            <p className={rowHintClass}>剪贴板 / 回退粘贴时使用。</p>
          </div>
          <div className="w-[190px] shrink-0">
            <select
              aria-label="粘贴快捷键"
              value={config.paste_shortcut}
              onChange={(event) =>
                void updateConfig({ paste_shortcut: event.target.value as PasteShortcut })
              }
              className={selectClass}
            >
              {pasteOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>粘贴后恢复剪贴板</div>
            <p className={rowHintClass}>粘贴完成后把你原来的剪贴板内容放回去。</p>
          </div>
          <Toggle
            checked={config.restore_clipboard_after_paste}
            ariaLabel="粘贴后恢复剪贴板"
            onChange={(checked) => void updateConfig({ restore_clipboard_after_paste: checked })}
          />
        </div>

        {outputFallbackHints.length > 0 && (
          <p className="mt-3 flex items-start gap-1.5 text-[11px] leading-relaxed text-text-tertiary">
            <Info size={12} className="mt-0.5 shrink-0" />
            <span>{outputFallbackHints.join(' ')}</span>
          </p>
        )}
      </section>

      <section className={sectionClass}>
        <h3 className={sectionTitleClass}>外观</h3>
        <div className={`${rowClass} mt-3`}>
          <div>
            <div className={rowLabelClass}>主题</div>
            <p className={rowHintClass}>选择浅色、深色，或跟随系统外观。</p>
          </div>
          <div className="w-[240px] shrink-0">
            <SegmentedControl
              options={THEME_OPTIONS}
              ariaLabel="主题"
              value={config.theme}
              onChange={(value) => void updateConfig({ theme: value as Theme })}
            />
          </div>
        </div>
      </section>

      <section className={sectionClass}>
        <h3 className={sectionTitleClass}>启动与窗口</h3>
        <div className={`${rowClass} mt-3`}>
          <div>
            <div className={rowLabelClass}>开机启动</div>
            <p className={rowHintClass}>登录系统后自动在后台运行。</p>
          </div>
          <Toggle
            checked={config.auto_start}
            ariaLabel="开机启动"
            onChange={(checked) => void handleAutoStart(checked)}
          />
        </div>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>关闭到托盘</div>
            <p className={rowHintClass}>关闭主窗口时继续在托盘运行，快捷键仍然可用。</p>
          </div>
          <Toggle
            checked={config.close_to_tray}
            ariaLabel="关闭到托盘"
            onChange={(checked) => void updateConfig({ close_to_tray: checked })}
          />
        </div>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>启动时最小化</div>
            <p className={rowHintClass}>启动后不显示主窗口，直接从托盘或快捷键使用。</p>
          </div>
          <Toggle
            checked={config.start_minimized}
            ariaLabel="启动时最小化"
            onChange={(checked) => void updateConfig({ start_minimized: checked })}
          />
        </div>

        <div className={rowClass}>
          <div>
            <div className={rowLabelClass}>胶囊自动隐藏</div>
            <p className={rowHintClass}>关闭后，空闲时也会在屏幕底部保留一个小胶囊。</p>
          </div>
          <Toggle
            checked={config.capsule_auto_hide}
            ariaLabel="胶囊自动隐藏"
            onChange={(checked) => void handleCapsuleAutoHide(checked)}
          />
        </div>
      </section>
    </div>
  )
}
