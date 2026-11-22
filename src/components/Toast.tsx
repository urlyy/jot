import { useCallback, useEffect, useRef, useState } from 'react'
import { CheckCircle2, Info, X, XCircle } from 'lucide-react'
import { registerToastHandler, type ToastType } from './toast-service'

interface ToastMessage {
  id: number
  text: string
  type: ToastType
  leaving?: boolean
}

const icons: Record<ToastType, typeof Info> = {
  success: CheckCircle2,
  error: XCircle,
  info: Info,
}

const iconTint: Record<ToastType, string> = {
  success: 'bg-success/10 text-success',
  error: 'bg-error/10 text-error',
  info: 'bg-accent/10 text-accent-text',
}

const railTint: Record<ToastType, string> = {
  success: 'bg-success/70',
  error: 'bg-error/70',
  info: 'bg-accent/70',
}

const TOAST_LIFE_MS = 3200
const TOAST_LEAVE_MS = 150

let nextToastId = 1

export function ToastContainer() {
  const [toasts, setToasts] = useState<ToastMessage[]>([])
  const leavingIds = useRef(new Set<number>())
  const leaveTimers = useRef(new Set<number>())

  const dismiss = useCallback((id: number) => {
    if (leavingIds.current.has(id)) return
    leavingIds.current.add(id)
    setToasts((prev) =>
      prev.map((toast) => (toast.id === id ? { ...toast, leaving: true } : toast)),
    )

    const timer = window.setTimeout(() => {
      setToasts((prev) => prev.filter((toast) => toast.id !== id))
      leavingIds.current.delete(id)
      leaveTimers.current.delete(timer)
    }, TOAST_LEAVE_MS)
    leaveTimers.current.add(timer)
  }, [])

  useEffect(() => {
    const autoTimers = new Set<number>()
    const unregister = registerToastHandler((text: string, type: ToastType = 'info') => {
      const id = nextToastId++
      setToasts((prev) => [...prev.slice(-2), { id, text, type }])
      const timer = window.setTimeout(() => {
        autoTimers.delete(timer)
        dismiss(id)
      }, TOAST_LIFE_MS)
      autoTimers.add(timer)
    })

    return () => {
      unregister()
      autoTimers.forEach((timer) => window.clearTimeout(timer))
    }
  }, [dismiss])

  useEffect(() => {
    const timers = leaveTimers.current
    return () => {
      timers.forEach((timer) => window.clearTimeout(timer))
    }
  }, [])

  return (
    <div className="pointer-events-none fixed right-5 bottom-5 z-[9999] flex w-[360px] max-w-[calc(100vw_-_2.5rem)] flex-col items-end gap-2.5">
      {toasts.map((toast) => {
        const Icon = icons[toast.type]
        return (
          <div
            key={toast.id}
            role={toast.type === 'error' ? 'alert' : 'status'}
            className={`pointer-events-auto relative flex w-full items-start gap-2.5 overflow-hidden rounded-[12px] border border-border bg-bg-elevated/95 px-3.5 py-3 shadow-float backdrop-blur-xl ${
              toast.leaving ? 'toast-leave' : 'toast-enter'
            }`}
          >
            <span
              aria-hidden="true"
              className={`absolute inset-y-2.5 left-0 w-[2px] rounded-full ${railTint[toast.type]}`}
            />

            <span
              aria-hidden="true"
              className={`mt-px flex h-5 w-5 shrink-0 items-center justify-center rounded-full ${iconTint[toast.type]}`}
            >
              <Icon size={12} strokeWidth={2.6} />
            </span>

            <span className="min-w-0 flex-1 pt-px text-[12.5px] leading-relaxed break-words text-text-primary">
              {toast.text}
            </span>

            <button
              type="button"
              onClick={() => dismiss(toast.id)}
              aria-label="关闭提示"
              className="-mt-1 -mr-1 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-full border-none bg-transparent text-text-tertiary outline-none transition-colors duration-150 hover:bg-bg-secondary hover:text-text-primary focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent active:scale-95"
            >
              <X size={12} aria-hidden="true" />
            </button>
          </div>
        )
      })}
    </div>
  )
}
