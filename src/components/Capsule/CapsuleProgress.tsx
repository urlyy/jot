import type { LucideIcon } from 'lucide-react'

interface Props {
  icon: LucideIcon
  iconClassName?: string
  label: string
  hint: string
  tag: string
}

/** 转写 / 润色共用的“细扫光 + 简短文案”处理态 */
export function CapsuleProgress({ icon: Icon, iconClassName, label, hint, tag }: Props) {
  return (
    <div className="relative flex h-full w-full flex-col justify-center gap-2.5 px-4">
      <div className="flex h-5 items-center gap-2">
        <Icon
          size={12}
          className={`shrink-0 ${iconClassName ?? 'text-[#a5a5ff]'}`}
          aria-hidden="true"
        />
        <span className="shrink-0 text-[12px] font-medium tracking-[-0.01em] text-white/90">
          {label}
        </span>
        <span aria-hidden="true" className="h-3 w-px shrink-0 bg-white/10" />
        <span className="truncate text-[11px] leading-none text-white/50">{hint}</span>
        <span className="ml-auto shrink-0 font-mono text-[9px] uppercase leading-none tracking-[0.16em] text-white/45">
          {tag}
        </span>
      </div>

      <div className="relative h-[3px] overflow-hidden rounded-full bg-white/[0.14]">
        <span className="capsule-sweep absolute inset-y-0 w-[38%] rounded-full bg-gradient-to-r from-transparent via-[#c3c4ff] to-transparent shadow-[0_0_10px_rgba(165,165,255,0.6)]" />
      </div>
    </div>
  )
}
