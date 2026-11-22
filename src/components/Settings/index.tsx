import { useEffect, useRef, useState } from 'react'
import { SettingsTabs } from './SettingsTabs'
import { PANES, type PaneId } from './panes'
import { GeneralPane } from './GeneralPane'
import { SpeechPane } from './SpeechPane'
import { PolishPane } from './PolishPane'
import { DictionaryPane } from './DictionaryPane'
import { AboutPane } from './AboutPane'

const PANE_META: Record<PaneId, { title: string; description: string }> = {
  general: { title: '通用', description: '快捷键、录音与文字输出的偏好设置。' },
  speech: { title: '语音识别', description: '接入你自己的 OpenAI 兼容转写服务。' },
  polish: { title: '智能润色', description: '让模型把口语整理成干净的书面表达。' },
  dictionary: { title: '词典', description: '告诉模型哪些专有名词不能被写错。' },
  about: { title: '关于', description: '版本、运行环境与隐私说明。' },
}

function paneFromHash(): PaneId | null {
  const query = window.location.hash.split('?')[1]
  if (!query) return null
  const requested = new URLSearchParams(query).get('pane')
  return requested && requested in PANE_META ? (requested as PaneId) : null
}

/** 品牌标记：声音的三道波 + 一道文字光标，呼应「语音 → 文字」 */
function BrandMark() {
  return (
    <span className="relative flex h-7 w-7 shrink-0 items-center justify-center overflow-hidden rounded-[9px] bg-accent shadow-[0_2px_8px_-1px_rgba(90,90,240,0.5)]">
      <span
        aria-hidden="true"
        className="absolute inset-x-0 top-0 h-1/2 bg-gradient-to-b from-white/25 to-transparent"
      />
      <svg
        viewBox="0 0 24 24"
        fill="none"
        aria-hidden="true"
        className="relative h-[15px] w-[15px]"
      >
        <path
          d="M4.5 9.5v5M8.5 6.5v11M12.5 8.5v7M17.5 5.5v13"
          stroke="white"
          strokeWidth="1.7"
          strokeLinecap="round"
        />
      </svg>
    </span>
  )
}

export function Settings() {
  const [activePane, setActivePane] = useState<PaneId>(() => paneFromHash() ?? 'general')
  const contentRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    const syncRequestedPane = () => {
      const requested = paneFromHash()
      if (requested) {
        setActivePane(requested)
      } else if (window.location.hash.startsWith('#/settings')) {
        setActivePane('general')
      }
    }
    window.addEventListener('hashchange', syncRequestedPane)
    return () => window.removeEventListener('hashchange', syncRequestedPane)
  }, [])

  useEffect(() => {
    contentRef.current?.scrollTo?.({ top: 0 })
  }, [activePane])

  const activeMeta = PANE_META[activePane]
  const activeIndex = PANES.findIndex((pane) => pane.id === activePane)
  const paneNumber = String(activeIndex + 1).padStart(2, '0')
  const paneTotal = String(PANES.length).padStart(2, '0')

  return (
    <div className="flex h-screen w-full flex-col overflow-hidden bg-bg-primary text-text-primary">
      <header className="shrink-0 border-b border-border bg-bg-elevated/80 backdrop-blur-xl">
        <div className="mx-auto flex h-[60px] w-full max-w-[720px] items-center gap-4 px-6">
          <div className="flex min-w-0 shrink-0 items-center gap-2.5">
            <BrandMark />
            <div className="min-w-0">
              <p className="text-[13.5px] font-semibold leading-none tracking-[-0.02em] text-text-primary">
                Jot
              </p>
              <p className="mt-[4px] font-mono text-[8.5px] uppercase leading-none tracking-[0.16em] text-text-tertiary">
                voice → text
              </p>
            </div>
          </div>

          <span aria-hidden="true" className="h-7 w-px shrink-0 bg-border" />

          <div className="ml-auto min-w-0">
            <SettingsTabs activePane={activePane} onSelect={setActivePane} />
          </div>
        </div>
      </header>

      <main ref={contentRef} className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-6 py-8">
        <div className="mx-auto w-full max-w-[680px]">
          <div className="mb-6 flex items-end justify-between gap-4 border-b border-border pb-5">
            <div className="min-w-0">
              <span className="flex items-center gap-1.5 font-mono text-[9.5px] uppercase leading-none tracking-[0.2em] text-accent-text">
                <span aria-hidden="true" className="h-px w-3.5 bg-accent-text/70" />
                settings
              </span>
              <h1 className="mt-2 text-[21px] font-semibold leading-none tracking-[-0.02em] text-text-primary">
                {activeMeta.title}
              </h1>
              <p className="mt-2 text-[12.5px] leading-relaxed text-text-secondary">
                {activeMeta.description}
              </p>
            </div>
            <span
              aria-hidden="true"
              className="shrink-0 pb-0.5 font-mono text-[10px] tabular-nums tracking-[0.14em] text-text-tertiary"
            >
              {paneNumber} / {paneTotal}
            </span>
          </div>

          <div
            key={activePane}
            id={`settings-panel-${activePane}`}
            role="tabpanel"
            aria-labelledby={`settings-tab-${activePane}`}
            className="settings-pane outline-none"
          >
            {activePane === 'general' && <GeneralPane />}
            {activePane === 'speech' && <SpeechPane />}
            {activePane === 'polish' && <PolishPane />}
            {activePane === 'dictionary' && <DictionaryPane />}
            {activePane === 'about' && <AboutPane />}
          </div>
        </div>
      </main>
    </div>
  )
}
