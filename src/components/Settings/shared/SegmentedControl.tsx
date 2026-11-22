interface Option {
  value: string
  label: string
}

interface Props {
  options: Option[]
  value: string
  onChange: (value: string) => void
  disabled?: boolean
  /** 控件组的读屏名称 */
  ariaLabel?: string
}

export function SegmentedControl({ options, value, onChange, disabled = false, ariaLabel }: Props) {
  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className={`flex gap-0.5 rounded-[10px] border border-border/70 bg-bg-secondary/70 p-0.5 ${
        disabled ? 'opacity-50' : ''
      }`}
    >
      {options.map((option) => {
        const active = value === option.value
        return (
          <button
            key={option.value}
            type="button"
            disabled={disabled}
            aria-pressed={active}
            onClick={() => onChange(option.value)}
            className={`flex-1 whitespace-nowrap rounded-[8px] border-none px-3 py-1.5 text-[12px] outline-none transition-[background-color,color,box-shadow] duration-150 focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent ${
              active
                ? 'bg-bg-elevated font-medium text-text-primary shadow-sm'
                : 'bg-transparent text-text-secondary hover:bg-bg-elevated/50 hover:text-text-primary'
            } ${
              disabled
                ? 'cursor-not-allowed'
                : active
                  ? 'cursor-default'
                  : 'cursor-pointer active:translate-y-px'
            }`}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
