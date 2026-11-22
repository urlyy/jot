/** 设置页共用的控件样式，保持所有页签视觉一致 */

const focusRing =
  'outline-none focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent'

const interactiveTransition = 'transition-colors duration-150'

export const inputClass = `w-full px-3 py-2 bg-bg-elevated border border-border rounded-[10px] text-[13px] text-text-primary placeholder:text-text-tertiary ${focusRing} ${interactiveTransition} hover:border-text-tertiary/40 focus:border-border-focus disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:border-border`

export const textareaClass = `${inputClass} resize-none leading-relaxed`

export const selectClass = `${inputClass} cursor-pointer pr-8`

export const primaryButtonClass = `inline-flex items-center justify-center gap-1.5 px-3.5 py-2 bg-accent text-white rounded-[10px] text-[13px] font-medium border-none cursor-pointer ${focusRing} ${interactiveTransition} hover:bg-accent-hover active:translate-y-px disabled:opacity-50 disabled:cursor-not-allowed disabled:active:translate-y-0`

export const secondaryButtonClass = `inline-flex items-center justify-center gap-1.5 px-3.5 py-2 bg-bg-secondary text-text-primary rounded-[10px] text-[13px] border border-border cursor-pointer ${focusRing} ${interactiveTransition} hover:bg-bg-tertiary active:translate-y-px disabled:opacity-50 disabled:cursor-not-allowed disabled:active:translate-y-0`

/** 一个分组 = 一张安静的白/深色卡片；页签之间靠它建立节奏 */
export const sectionClass =
  'rounded-[12px] border border-border bg-bg-elevated p-5 shadow-[0_1px_2px_rgba(18,18,30,0.04)] mb-4 last:mb-0'

export const sectionTitleClass = 'text-[12.5px] font-semibold tracking-[-0.01em] text-text-primary'

export const sectionHintClass = 'mt-1 text-[12px] leading-[1.6] text-text-secondary'

export const rowClass =
  'flex items-center justify-between gap-6 py-3 border-b border-border last:border-b-0'

export const rowLabelClass = 'text-[13px] text-text-primary'

export const rowHintClass = 'mt-0.5 text-[12px] leading-[1.55] text-text-secondary'

export const bannerClass =
  'flex items-start gap-2.5 rounded-[12px] border border-warning/30 bg-warning/10 px-3.5 py-3 text-[12px] leading-relaxed text-text-primary'

export const dangerBannerClass =
  'flex items-start gap-2.5 rounded-[12px] border border-error/30 bg-error/10 px-3.5 py-3 text-[12px] leading-relaxed text-text-primary'
