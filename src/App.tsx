import { useCallback, useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { useAppStore } from './stores/appStore'
import { useTauriEvents } from './hooks/useTauriEvents'
import { useTheme } from './hooks/useTheme'
import {
  checkAccessibilityPermission,
  formatError,
  getHotkeyRegistrationError,
  getPlatformCapabilities,
  loadConfig,
} from './lib/tauri'
import { Capsule } from './components/Capsule'
import { Settings } from './components/Settings'
import { ToastContainer } from './components/Toast'

type LoadStatus = 'loading' | 'ready' | 'error'

function CapsuleApp() {
  useTheme()
  useTauriEvents()

  const setConfig = useAppStore((state) => state.setConfig)

  useEffect(() => {
    let cancelled = false
    loadConfig()
      .then((config) => {
        if (!cancelled) setConfig(config)
      })
      .catch((error) => {
        console.error('Failed to load config in capsule window:', error)
      })
    return () => {
      cancelled = true
    }
  }, [setConfig])

  return <Capsule />
}

function MainApp() {
  useTheme()
  useTauriEvents()

  const setConfig = useAppStore((state) => state.setConfig)
  const setPlatformCapabilities = useAppStore((state) => state.setPlatformCapabilities)
  const setHotkeyRegistrationError = useAppStore((state) => state.setHotkeyRegistrationError)
  const setAccessibilityTrusted = useAppStore((state) => state.setAccessibilityTrusted)

  const [status, setStatus] = useState<LoadStatus>('loading')
  const [errorMessage, setErrorMessage] = useState('')

  const load = useCallback(async () => {
    setStatus('loading')
    setErrorMessage('')

    // 生产构建里前端是内嵌资源，可能比后端 setup 更早开始调用命令。
    // 后端会等 state 就绪，这里再兜一层自动重试，避免用户看到错误页。
    const attempts = 4
    let lastError: unknown = null

    for (let attempt = 0; attempt < attempts; attempt += 1) {
      try {
        const [config, capabilities, registrationError] = await Promise.all([
          loadConfig(),
          getPlatformCapabilities(),
          getHotkeyRegistrationError(),
        ])

        setConfig(config)
        setPlatformCapabilities(capabilities)
        setHotkeyRegistrationError(registrationError)

        // 辅助功能权限只对 macOS 有意义，其他平台完全不调用
        if (capabilities.os === 'macos') {
          checkAccessibilityPermission()
            .then(setAccessibilityTrusted)
            .catch(() => setAccessibilityTrusted(null))
        } else {
          setAccessibilityTrusted(null)
        }

        setStatus('ready')
        return
      } catch (error) {
        lastError = error
        if (attempt < attempts - 1) {
          await new Promise((resolve) => window.setTimeout(resolve, 250 * (attempt + 1)))
        }
      }
    }

    console.error('Failed to load the settings window:', lastError)
    setErrorMessage(formatError(lastError))
    setStatus('error')
  }, [setAccessibilityTrusted, setConfig, setHotkeyRegistrationError, setPlatformCapabilities])

  useEffect(() => {
    void load()
  }, [load])

  if (status === 'loading') {
    return (
      <div className="flex h-screen w-full flex-col items-center justify-center gap-3 bg-bg-primary text-text-secondary">
        <Loader2 size={18} className="animate-spin text-accent" />
        <span className="text-[13px]">加载中…</span>
      </div>
    )
  }

  if (status === 'error') {
    return (
      <div className="flex h-screen w-full flex-col items-center justify-center gap-3 bg-bg-primary px-8 text-center">
        <h1 className="text-[16px] font-medium text-text-primary">无法加载设置</h1>
        <p className="max-w-[420px] text-[12px] leading-relaxed text-text-secondary">
          请确认应用后端已经启动。如果刚刚更新过，请重新启动 Jot。
        </p>
        {errorMessage && (
          <p className="max-w-[480px] break-words text-[11px] leading-relaxed text-text-tertiary">
            {errorMessage}
          </p>
        )}
        <button
          type="button"
          onClick={() => void load()}
          className="mt-1 cursor-pointer rounded-[10px] bg-accent px-4 py-2 text-[13px] font-medium text-white transition-colors hover:bg-accent-hover"
        >
          重试
        </button>
      </div>
    )
  }

  return (
    <>
      <Settings />
      <ToastContainer />
    </>
  )
}

function App() {
  // 胶囊窗口通过 index.html#capsule 打开，同步判断避免挂载后再切换
  if (window.location.hash === '#capsule') return <CapsuleApp />
  return <MainApp />
}

export default App
