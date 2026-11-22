import { useState } from 'react'
import { BookOpen, Check, Loader2 } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { primaryButtonClass, sectionHintClass, sectionTitleClass, textareaClass } from './shared/ui'

/** 取一行的术语部分（`术语 = 说明` 里的术语），与后端 dictionary::parse_entry 保持一致。 */
function termOf(line: string): string {
  const indexes = [line.indexOf('='), line.indexOf('：')].filter((index) => index >= 0)
  if (indexes.length === 0) return line.trim()
  return line.slice(0, Math.min(...indexes)).trim()
}

function parseWords(text: string): string[] {
  const seen = new Set<string>()
  const words: string[] = []
  for (const line of text.split(/\r?\n/)) {
    const entry = line.trim()
    if (!entry) continue
    const term = termOf(entry) || entry
    // 同一个术语只保留第一条，避免重复喂给模型
    if (seen.has(term)) continue
    seen.add(term)
    words.push(entry)
  }
  return words
}

export function DictionaryPane() {
  const config = useAppStore((state) => state.config)
  const updateConfig = useAppStore((state) => state.updateConfig)
  const savedNotice = useAppStore((state) => state.savedNotice)
  const showSavedNotice = useAppStore((state) => state.showSavedNotice)

  const [draft, setDraft] = useState(() => config.dictionary.join('\n'))
  const [saving, setSaving] = useState(false)

  const draftWords = parseWords(draft)
  const dirty =
    draftWords.length !== config.dictionary.length ||
    draftWords.some((word, index) => word !== config.dictionary[index])

  const handleSave = async () => {
    if (!dirty || saving) return
    setSaving(true)
    const words = parseWords(draft)
    const ok = await updateConfig({ dictionary: words })
    setSaving(false)
    if (ok) {
      setDraft(words.join('\n'))
      showSavedNotice('已保存')
    }
  }

  return (
    <div>
      <div className="mb-4 flex items-start gap-2">
        <BookOpen size={15} className="mt-0.5 text-accent" />
        <div>
          <h3 className={sectionTitleClass}>自定义词典</h3>
          <p className={sectionHintClass}>
            一行一个术语。转写时只把术语本身作为提示词偏置；如果写成「术语 = 说明」，
            说明会在润色时一起交给模型，帮它选对同音字、理解上下文。
          </p>
        </div>
      </div>

      <textarea
        rows={14}
        value={draft}
        spellCheck={false}
        placeholder={
          '一行一个术语，可以用 = 补充说明，例如：\nJot\n联调 = 接口联合调试，不是「连调」\nSeedASR：火山引擎流式识别\n张伟'
        }
        onChange={(event) => setDraft(event.target.value)}
        className={`${textareaClass} min-h-[260px]`}
      />

      <div className="mt-3 flex items-center justify-between gap-4">
        <span className="text-[11px] text-text-tertiary">
          共 {draftWords.length} 个术语 · 空行会自动忽略
        </span>
        <div className="flex items-center gap-3">
          {savedNotice === '已保存' && (
            <span className="flex items-center gap-1 text-[12px] text-success">
              <Check size={13} />
              {savedNotice}
            </span>
          )}
          <button
            type="button"
            disabled={!dirty || saving}
            onClick={() => void handleSave()}
            className={primaryButtonClass}
          >
            {saving && <Loader2 size={14} className="animate-spin" />}
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
  )
}
