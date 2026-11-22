import { useState } from 'react'
import { Eye, EyeOff, KeyRound } from 'lucide-react'
import { inputClass } from './ui'

interface Props {
  value: string
  onChange: (value: string) => void
  onBlur: () => void
  saved: boolean
  disabled?: boolean
  placeholder?: string
}

export function ApiKeyField({
  value,
  onChange,
  onBlur,
  saved,
  disabled = false,
  placeholder = 'sk-…',
}: Props) {
  const [visible, setVisible] = useState(false)

  return (
    <div>
      <div className="relative">
        <KeyRound
          size={13}
          className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary"
          aria-hidden="true"
        />
        <input
          type={visible ? 'text' : 'password'}
          value={value}
          disabled={disabled}
          placeholder={placeholder}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
          onBlur={onBlur}
          className={`${inputClass} pl-8 pr-9 disabled:cursor-not-allowed disabled:opacity-50`}
        />
        <button
          type="button"
          tabIndex={-1}
          aria-label={visible ? '隐藏密钥' : '显示密钥'}
          onClick={() => setVisible((current) => !current)}
          className="absolute right-2 top-1/2 -translate-y-1/2 rounded-[6px] border-none bg-transparent p-1 text-text-tertiary transition-colors hover:text-text-primary"
        >
          {visible ? <EyeOff size={14} /> : <Eye size={14} />}
        </button>
      </div>
      <p className="mt-1.5 text-[11px] text-text-tertiary">
        {saved ? '已保存到系统钥匙串' : '输入后失焦即保存到系统钥匙串'}
      </p>
    </div>
  )
}
