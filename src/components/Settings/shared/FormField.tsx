import type { ReactNode } from 'react'

interface Props {
  label: string
  hint?: ReactNode
  children: ReactNode
}

export function FormField({ label, hint, children }: Props) {
  return (
    <div>
      <label className="mb-1.5 block text-[13px] font-medium text-text-primary">{label}</label>
      {hint && <p className="mb-2 text-[12px] leading-relaxed text-text-secondary">{hint}</p>}
      {children}
    </div>
  )
}
