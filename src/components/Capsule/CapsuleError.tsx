import { AlertTriangle, X } from 'lucide-react'
import type { PipelineErrorPayload } from '../../lib/tauri'

const ERROR_MESSAGES: Record<string, string> = {
  mic_unavailable: '麦克风不可用，请检查系统权限',
  stt_not_configured: '还没有配置转写接口',
  stt_auth: '转写接口鉴权失败，请检查 API Key',
  stt_failed: '转写失败，请稍后重试',
  no_speech: '没有检测到语音',
  llm_not_configured: '还没有配置润色接口',
  llm_failed: '润色失败，请稍后重试',
  output_failed: '文字输出失败',
  accessibility_required: '需要辅助功能权限才能输入文字',
  internal: '出现内部错误',
}

function pipelineErrorMessage(error: PipelineErrorPayload): string {
  return ERROR_MESSAGES[error.code] ?? `操作失败（${error.code}）`
}

interface Props {
  error: PipelineErrorPayload
  onClose: () => void
}

export function CapsuleError({ error, onClose }: Props) {
  const message = pipelineErrorMessage(error)

  return (
    <button
      type="button"
      onClick={onClose}
      title="点击关闭"
      className="group relative flex h-full w-full cursor-pointer items-center gap-2.5 border-none bg-transparent px-4 text-left outline-none transition-colors duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[#f0a36b]/60"
    >
      <span className="flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full bg-[#f0a36b]/15 text-[#f5b27a]">
        <AlertTriangle size={12} strokeWidth={2.4} aria-hidden="true" />
      </span>

      <span className="min-w-0 flex-1 truncate text-[11.5px] leading-none text-white/90">
        {message}
      </span>

      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-white/40 transition-colors duration-150 group-hover:bg-white/10 group-hover:text-white/80">
        <X size={12} aria-hidden="true" />
      </span>
    </button>
  )
}
