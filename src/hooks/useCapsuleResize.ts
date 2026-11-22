import { useEffect, useRef } from 'react'

export interface CapsuleSize {
  width: number
  height: number
}

/** 胶囊内容四周留白，用于投影 */
const WINDOW_PADDING = 24
/** 距屏幕工作区底部的间距（逻辑像素） */
const BOTTOM_MARGIN = 80

type WindowApi = typeof import('@tauri-apps/api/window')

interface MonitorLike {
  position: { x: number; y: number }
  size: { width: number; height: number }
  scaleFactor: number
  workArea: {
    position: { x: number; y: number }
    size: { width: number; height: number }
  }
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max))
}

function bottomCenterPosition(
  monitor: MonitorLike,
  logicalWidth: number,
  logicalHeight: number,
): { x: number; y: number } {
  const scale = monitor.scaleFactor || 1
  const width = Math.round(logicalWidth * scale)
  const height = Math.round(logicalHeight * scale)
  const margin = Math.round(BOTTOM_MARGIN * scale)
  const area = monitor.workArea ?? {
    position: monitor.position,
    size: monitor.size,
  }
  const centeredX = area.position.x + Math.round((area.size.width - width) / 2)
  const bottomY = area.position.y + area.size.height - height - margin

  return {
    x: clamp(centeredX, area.position.x, area.position.x + area.size.width - width),
    y: clamp(bottomY, area.position.y, area.position.y + area.size.height - height),
  }
}

/** 优先跟随主窗口所在显示器，取不到时退回主显示器/当前显示器 */
async function resolveMonitor(api: WindowApi): Promise<MonitorLike | null> {
  try {
    const main = await api.Window.getByLabel('main')
    if (main) {
      const [position, size] = await Promise.all([main.outerPosition(), main.outerSize()])
      const monitor = await api.monitorFromPoint(
        position.x + size.width / 2,
        position.y + size.height / 2,
      )
      if (monitor) return monitor
    }
  } catch {
    /* 主窗口不可用时继续用默认显示器 */
  }

  const primary = await api.primaryMonitor().catch(() => null)
  if (primary) return primary
  return api.currentMonitor().catch(() => null)
}

/**
 * 胶囊窗口按内容自适应大小，并停靠在当前屏幕底部居中。
 * 窗口只在 visible 为 true 时显示。
 */
export function useCapsuleResize(visible: boolean, size: CapsuleSize) {
  const initialized = useRef(false)
  const generation = useRef(0)

  useEffect(() => {
    const current = ++generation.current
    const isCurrent = () => generation.current === current
    const windowWidth = size.width + WINDOW_PADDING
    const windowHeight = size.height + WINDOW_PADDING

    void (async () => {
      try {
        const api = await import('@tauri-apps/api/window')
        if (!isCurrent()) return

        const win = api.getCurrentWindow()

        if (!initialized.current) {
          initialized.current = true
          await win.setFocusable(false).catch(() => {})
          if (!isCurrent()) return
        }

        await win.setSize(new api.LogicalSize(windowWidth, windowHeight)).catch(() => {})
        if (!isCurrent()) return

        const monitor = await resolveMonitor(api)
        if (!isCurrent()) return

        if (monitor) {
          const position = bottomCenterPosition(monitor, windowWidth, windowHeight)
          await win.setPosition(new api.PhysicalPosition(position.x, position.y)).catch(() => {})
          if (!isCurrent()) return
        }

        if (visible) {
          await win.show().catch(() => {})
        } else {
          await win.hide().catch(() => {})
        }
      } catch (error) {
        console.error('Failed to resize capsule window:', error)
      }
    })()

    return () => {
      generation.current += 1
    }
  }, [visible, size.width, size.height])
}
