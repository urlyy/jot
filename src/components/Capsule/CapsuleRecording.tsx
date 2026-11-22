import { useAppStore } from '../../stores/appStore'
import { tailText } from '../../lib/platform'
import { Waveform } from './Waveform'
import { DurationTimer } from './DurationTimer'

const WIDE_CHAR = /[\u2e80-\u9fff\uf900-\ufaff\uff00-\uffef]/

/** 粗估一行文本的显示宽度（全角按 1、半角按 0.55 个字符） */
function visualWidth(text: string): number {
  let width = 0
  for (const char of text) width += WIDE_CHAR.test(char) ? 1 : 0.55
  return width
}

export function CapsuleRecording() {
  const config = useAppStore((state) => state.config)
  const pipelineState = useAppStore((state) => state.pipelineState)
  const partialTranscript = useAppStore((state) => state.partialTranscript)

  const preparing = pipelineState === 'preparing'
  const partialTail = tailText(partialTranscript, 40)
  const streamingInsert = config.streaming_enabled && config.streaming_insert
  const showPreview = config.streaming_enabled

  const hint = preparing
    ? '正在准备麦克风…'
    : streamingInsert
      ? '文字会实时输入'
      : config.hotkey_mode === 'hold'
        ? '松手后插入'
        : '再按一次结束'

  // 短句从左边开始读；长句只显示末尾，避免新说的字先被裁掉
  const tailMode = visualWidth(partialTail) > 26

  return (
    <div
      className={`relative flex h-full w-full flex-col justify-center px-4 ${
        showPreview ? 'gap-1.5' : ''
      }`}
    >
      <div className="relative flex h-5 items-center">
        <span className="flex shrink-0 items-center gap-2.5">
          <span className="relative flex h-2.5 w-2.5 shrink-0 items-center justify-center">
            <span
              className="capsule-halo absolute h-2.5 w-2.5 rounded-full bg-rec/30"
              aria-hidden="true"
            />
            <span
              className="capsule-dot h-2 w-2 rounded-full bg-rec shadow-[0_0_10px_rgba(255,77,79,0.75)]"
              aria-hidden="true"
            />
          </span>

          <span className="shrink-0 font-mono text-[9px] font-medium uppercase leading-none tracking-[0.16em] text-rec/90">
            REC
          </span>
        </span>

        {/* 波形固定在药丸水平中心，左右信息各让出空间 */}
        <span className="pointer-events-none absolute left-1/2 -translate-x-1/2">
          <Waveform />
        </span>

        <span className="ml-auto flex shrink-0 items-center gap-2.5">
          {!showPreview && (
            <span className="shrink-0 whitespace-nowrap text-[10.5px] leading-none text-white/55">
              {hint}
            </span>
          )}

          <DurationTimer />
        </span>
      </div>

      {showPreview && (
        <div className="flex h-[14px] items-center gap-2">
          <span className="flex h-3 w-3 shrink-0 items-center justify-center">
            {tailMode ? (
              <span className="text-[11px] leading-none text-white/35" aria-hidden="true">
                …
              </span>
            ) : (
              <span
                className="capsule-caret h-3 w-[2px] rounded-full bg-[#a5a5ff]"
                aria-hidden="true"
              />
            )}
          </span>
          <div
            className={`capsule-tail flex min-w-0 flex-1 items-center overflow-hidden ${
              tailMode ? 'justify-end' : 'justify-start'
            }`}
          >
            <span
              className={`whitespace-nowrap text-[11px] leading-[14px] ${
                partialTail ? 'text-white/60' : 'text-white/45'
              }`}
            >
              {partialTail || hint}
            </span>
          </div>
        </div>
      )}
    </div>
  )
}
