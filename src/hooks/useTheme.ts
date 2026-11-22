import { useEffect } from 'react'
import { useAppStore } from '../stores/appStore'
import type { Theme } from '../lib/tauri'

/** 应用三档主题（light / dark / system），返回当前配置值 */
export function useTheme(): Theme {
  const theme = useAppStore((state) => state.config.theme)

  useEffect(() => {
    const root = document.documentElement

    const applyTheme = (value: Theme) => {
      if (value === 'dark') {
        root.classList.add('dark')
      } else if (value === 'light') {
        root.classList.remove('dark')
      } else {
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
        root.classList.toggle('dark', prefersDark)
      }
    }

    applyTheme(theme)

    if (theme === 'system') {
      const query = window.matchMedia('(prefers-color-scheme: dark)')
      const handler = (event: MediaQueryListEvent) => {
        root.classList.toggle('dark', event.matches)
      }
      query.addEventListener('change', handler)
      return () => query.removeEventListener('change', handler)
    }
  }, [theme])

  return theme
}
