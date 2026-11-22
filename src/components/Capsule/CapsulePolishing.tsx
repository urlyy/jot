import { Sparkles } from 'lucide-react'
import { CapsuleProgress } from './CapsuleProgress'

export function CapsulePolishing() {
  return (
    <CapsuleProgress
      icon={Sparkles}
      iconClassName="animate-pulse text-[#a5a5ff]"
      label="润色中"
      hint="正在整理文字"
      tag="polish"
    />
  )
}
