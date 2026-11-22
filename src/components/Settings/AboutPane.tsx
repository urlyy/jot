import { useEffect, useState } from 'react'
import { useAppStore } from '../../stores/appStore'
import { formatHotkey } from '../../lib/platform'
import { getAppInfo, type AppInfo, type PlatformOs } from '../../lib/tauri'

export function AboutPane() {
  const config = useAppStore((state) => state.config)
  const capabilities = useAppStore((state) => state.platformCapabilities)
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)

  const os: PlatformOs = capabilities?.os ?? 'unknown'

  useEffect(() => {
    let cancelled = false
    getAppInfo()
      .then((info) => {
        if (!cancelled) setAppInfo(info)
      })
      .catch((error) => {
        console.error('Failed to load app info:', error)
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <div>
      <div className="flex flex-col items-center py-8 text-center">
        <span className="flex h-14 w-14 items-center justify-center rounded-[16px] bg-accent text-[22px] font-semibold text-white shadow-md">
          E
        </span>
        <h2 className="mt-3 text-[20px] font-semibold tracking-tight text-text-primary">
          Jot
        </h2>
        <p className="mt-1 text-[12px] text-text-secondary">
          {appInfo ? `版本 ${appInfo.version}` : '版本读取中…'}
        </p>
        <p className="mt-1 text-[12px] text-text-tertiary">按住说话，文字落到光标处</p>
      </div>

      <div className="overflow-hidden rounded-[10px] border border-border">
        <InfoRow label="当前快捷键" value={formatHotkey(config.hotkey, os)} />
        <InfoRow label="版本号" value={appInfo?.version ?? '—'} />
        <InfoRow label="运行平台" value={os === 'unknown' ? '未知' : os} />
        <InfoRow label="许可证" value="MIT" />
      </div>

      <p className="mt-4 text-[12px] leading-relaxed text-text-secondary">
        音频只在录音期间留在内存，直接发往你自己配置的转写接口，不经过任何中转服务器；
        API Key 存放在系统钥匙串里，不写入配置文件。
      </p>
    </div>
  )
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-border bg-bg-elevated px-3.5 py-2.5 last:border-b-0">
      <span className="text-[12px] text-text-secondary">{label}</span>
      <span className="text-right text-[12px] text-text-primary">{value}</span>
    </div>
  )
}
