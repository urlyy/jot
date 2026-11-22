import { useRef, type KeyboardEvent } from 'react'
import { PANES, type PaneId } from './panes'

interface Props {
  activePane: PaneId
  onSelect: (pane: PaneId) => void
}

export function SettingsTabs({ activePane, onSelect }: Props) {
  const tabsRef = useRef<Array<HTMLButtonElement | null>>([])

  // 方向键在标签之间移动，Home / End 跳到首尾；符合 WAI-ARIA tabs 的键盘习惯
  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const lastIndex = PANES.length - 1
    let nextIndex: number | null = null

    if (event.key === 'ArrowLeft') nextIndex = index === 0 ? lastIndex : index - 1
    else if (event.key === 'ArrowRight') nextIndex = index === lastIndex ? 0 : index + 1
    else if (event.key === 'Home') nextIndex = 0
    else if (event.key === 'End') nextIndex = lastIndex

    if (nextIndex === null) return
    event.preventDefault()
    onSelect(PANES[nextIndex].id)
    tabsRef.current[nextIndex]?.focus()
  }

  return (
    <nav aria-label="设置分类" className="shrink-0">
      <div
        role="tablist"
        aria-label="设置分类"
        className="flex flex-nowrap items-center gap-1 rounded-full border border-border/80 bg-bg-secondary/60 p-1"
      >
        {PANES.map((pane, index) => {
          const Icon = pane.icon
          const active = activePane === pane.id
          return (
            <button
              key={pane.id}
              ref={(element) => {
                tabsRef.current[index] = element
              }}
              type="button"
              role="tab"
              id={`settings-tab-${pane.id}`}
              aria-selected={active}
              aria-controls={`settings-panel-${pane.id}`}
              tabIndex={active ? 0 : -1}
              onClick={() => onSelect(pane.id)}
              onKeyDown={(event) => handleKeyDown(event, index)}
              className={`group inline-flex shrink-0 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-full px-3 py-[7px] text-[12.5px] outline-none transition-[background-color,color,box-shadow] duration-150 focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
                active
                  ? 'bg-accent font-medium text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.16),0_3px_10px_-3px_rgba(90,90,240,0.65)]'
                  : 'text-text-secondary hover:bg-bg-elevated/80 hover:text-text-primary'
              }`}
            >
              <Icon
                size={13}
                strokeWidth={active ? 2.2 : 2}
                aria-hidden="true"
                className={
                  active
                    ? 'text-white'
                    : 'text-text-tertiary transition-colors group-hover:text-text-secondary'
                }
              />
              {pane.label}
            </button>
          )
        })}
      </div>
    </nav>
  )
}
