import { Loader2 } from 'lucide-react'
import { CapsuleProgress } from './CapsuleProgress'

export function CapsuleTranscribing() {
  return (
    <CapsuleProgress
      icon={Loader2}
      iconClassName="animate-spin text-[#a5a5ff]"
      label="转写中"
      hint="正在识别语音"
      tag="transcribe"
    />
  )
}
