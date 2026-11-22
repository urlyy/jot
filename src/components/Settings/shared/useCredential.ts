import { useCallback, useEffect, useRef, useState } from 'react'
import {
  clearCredential,
  formatError,
  readCredential,
  setCredential,
  type CredentialNamespace,
  type CredentialProvider,
} from '../../../lib/tauri'
import { toast } from '../../../components/toast-service'

/**
 * 管理某个 namespace + provider 的 API Key：
 * 打开页面或切换供应商时从钥匙串读取，失焦时写回（空值即清除）。
 *
 * 转写供应商的密钥是分开存的：OpenAI 兼容用 `custom`，火山引擎用 `volcengine`。
 *
 * 兼容旧调用：第二个参数传 boolean 时表示 `enabled`，provider 固定为 `custom`。
 */
export function useCredential(
  namespace: CredentialNamespace,
  providerOrEnabled: CredentialProvider | boolean = 'custom',
  enabledArg = true,
) {
  const provider: CredentialProvider =
    typeof providerOrEnabled === 'boolean' ? 'custom' : providerOrEnabled
  const enabled =
    typeof providerOrEnabled === 'boolean' ? providerOrEnabled : enabledArg
  const [apiKey, setApiKey] = useState('')
  const [saved, setSaved] = useState(false)
  const [loading, setLoading] = useState(true)
  const [loadedProvider, setLoadedProvider] = useState<CredentialProvider | null>(null)
  const lastSavedRef = useRef<string | null>(null)
  const loadingRef = useRef(true)
  const providerRef = useRef(provider)

  // Block writes during the render where the provider just changed, before the
  // effect has had a chance to reload the corresponding secret.
  if (providerRef.current !== provider) {
    providerRef.current = provider
    loadingRef.current = true
  }

  useEffect(() => {
    if (!enabled) return
    let cancelled = false

    loadingRef.current = true
    setLoading(true)
    setLoadedProvider(null)
    setApiKey('')
    setSaved(false)
    lastSavedRef.current = null

    readCredential(namespace, provider)
      .then((secret) => {
        if (cancelled) return
        const value = secret ?? ''
        setApiKey(value)
        setSaved(Boolean(value.trim()))
        lastSavedRef.current = value
      })
      .catch((error) => {
        console.error(`Failed to load ${namespace}/${provider} credential:`, error)
      })
      .finally(() => {
        if (cancelled) return
        loadingRef.current = false
        setLoading(false)
        setLoadedProvider(provider)
      })

    return () => {
      cancelled = true
    }
  }, [namespace, provider, enabled])

  const persist = useCallback(
    async (value: string) => {
      if (loadingRef.current) return
      const trimmed = value.trim()
      if (trimmed === lastSavedRef.current) return
      try {
        if (trimmed) {
          await setCredential(namespace, trimmed, provider)
        } else {
          await clearCredential(namespace, provider)
        }
        lastSavedRef.current = trimmed
        setSaved(Boolean(trimmed))
      } catch (error) {
        toast.error(`保存密钥失败：${formatError(error)}`)
      }
    },
    [namespace, provider],
  )

  return { apiKey, setApiKey, saved, persist, loading, loadedProvider }
}
