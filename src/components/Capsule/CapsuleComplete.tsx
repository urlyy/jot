import { AlertCircle, Check } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'

/** 只取结果开头，用于完成态的一行摘要 */
function headText(text: string, max = 20): string {
  const clean = text.replace(/\s+/g, ' ').trim()
  return clean.length > max ? `${clean.slice(0, max)}…` : clean
}

export function CapsuleComplete() {
  const insertResult = useAppStore((state) => state.insertResult)
  const polishedText = useAppStore((state) => state.polishedText)
  const finalTranscript = useAppStore((state) => state.finalTranscript)

  const text = headText(polishedText || finalTranscript, 20)
  const failed = insertResult?.status === 'failed'

  let label = '已输入'
  let detail = text
  if (failed) {
    label = '输出失败'
    detail = insertResult?.message ?? '请检查系统的输入权限'
  } else if (insertResult?.status === 'copiedFallback') {
    label = '已复制'
    detail = '可直接粘贴'
  } else if (insertResult?.status === 'partiallyInserted') {
    label = '部分已输入'
    detail = text || '其余内容在剪贴板中'
  } else if (!text) {
    label = '完成'
    detail = ''
  }

  const Icon = failed ? AlertCircle : Check

  return (
    <div className="flex h-full w-full items-center gap-2.5 px-4">
      <span
        className={`capsule-pop flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full ${
          failed ? 'bg-[#f0a36b]/15 text-[#f5b27a]' : 'bg-[#4ade80]/15 text-[#7be8a5]'
        }`}
      >
        <Icon size={13} strokeWidth={2.8} aria-hidden="true" />
      </span>

      <span className="shrink-0 whitespace-nowrap text-[11.5px] font-medium text-white/90">
        {label}
      </span>

      {detail && (
        <>
          <span aria-hidden="true" className="h-3 w-px shrink-0 bg-white/10" />
          <span className="min-w-0 flex-1 truncate text-[11px] text-white/60">{detail}</span>
        </>
      )}
    </div>
  )
}
