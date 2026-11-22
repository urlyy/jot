interface Props {
  checked: boolean
  onChange: (checked: boolean) => void
  label?: string
  /** 只给读屏用的名称（视觉上已有同行的标题时使用） */
  ariaLabel?: string
  disabled?: boolean
}

export function Toggle({ checked, onChange, label, ariaLabel, disabled = false }: Props) {
  return (
    <label
      className={`flex items-center gap-2.5 ${disabled ? 'cursor-not-allowed' : 'cursor-pointer'}`}
    >
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={ariaLabel ?? label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative h-[24px] w-[42px] shrink-0 rounded-full outline-none transition-[background-color,box-shadow,transform] duration-200 focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-50 disabled:active:scale-100 ${
          checked
            ? 'bg-accent shadow-[inset_0_1px_1px_rgba(0,0,0,0.14)]'
            : 'bg-bg-tertiary shadow-[inset_0_1px_2px_rgba(18,18,30,0.12)]'
        }`}
      >
        <span
          aria-hidden="true"
          className={`absolute top-[2px] h-[20px] w-[20px] rounded-full bg-white shadow-[0_1px_3px_rgba(18,18,30,0.28)] transition-transform duration-200 ${
            checked ? 'translate-x-[20px]' : 'translate-x-[2px]'
          }`}
        />
      </button>
      {label && (
        <span
          className={`text-[13px] transition-colors ${
            disabled ? 'text-text-tertiary' : 'text-text-primary'
          }`}
        >
          {label}
        </span>
      )}
    </label>
  )
}
