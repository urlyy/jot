import { useState } from 'react'
import { Loader2, PlugZap, Sparkles } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { benchLlmConnection, formatError, type PolishStyle } from '../../lib/tauri'
import { useCredential } from './shared/useCredential'
import { ApiKeyField } from './shared/ApiKeyField'
import { FormField } from './shared/FormField'
import { SegmentedControl } from './shared/SegmentedControl'
import { Toggle } from './shared/Toggle'
import {
  inputClass,
  primaryButtonClass,
  sectionClass,
  sectionHintClass,
  sectionTitleClass,
  textareaClass,
} from './shared/ui'

const STYLE_OPTIONS = [
  { value: 'minimal', label: '极简' },
  { value: 'clean', label: '清爽' },
  { value: 'structured', label: '结构化' },
  { value: 'custom', label: '自定义' },
]

type TestState =
  | { status: 'idle' }
  | { status: 'testing' }
  | { status: 'success'; ms: number }
  | { status: 'error'; message: string }

export function PolishPane() {
  const config = useAppStore((state) => state.config)
  const updateConfig = useAppStore((state) => state.updateConfig)
  const { apiKey, setApiKey, saved, persist } = useCredential('llm', config.polish_enabled)

  const [testState, setTestState] = useState<TestState>({ status: 'idle' })

  const handleTest = async () => {
    setTestState({ status: 'testing' })
    try {
      const ms = await benchLlmConnection(
        apiKey.trim(),
        config.llm_base_url.trim(),
        config.llm_model.trim(),
        config.llm_disable_thinking,
      )
      setTestState({ status: 'success', ms })
    } catch (error) {
      setTestState({ status: 'error', message: formatError(error) })
    }
  }

  const canTest =
    config.llm_base_url.trim().length > 0 &&
    config.llm_model.trim().length > 0 &&
    testState.status !== 'testing'

  return (
    <div>
      <div className="flex items-start justify-between gap-5 rounded-[10px] border border-border bg-bg-elevated p-4 shadow-sm">
        <div className="flex items-start gap-3">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px] bg-accent/10 text-accent">
            <Sparkles size={16} />
          </span>
          <div>
            <h3 className="text-[14px] font-medium text-text-primary">用 LLM 润色转写结果</h3>
            <p className="mt-1 text-[12px] leading-relaxed text-text-secondary">
              把转写文字交给你的 OpenAI
              兼容对话接口改写，去掉口头语、补上标点，并按词典保留专有名词。
            </p>
          </div>
        </div>
        <Toggle
          checked={config.polish_enabled}
          onChange={(checked) => void updateConfig({ polish_enabled: checked })}
        />
      </div>

      {!config.polish_enabled ? (
        <div className="mt-4 rounded-[10px] border border-dashed border-border px-4 py-7 text-center text-[12px] text-text-tertiary">
          关闭后直接输出转写原文
        </div>
      ) : (
        <section className={`${sectionClass} mt-6`}>
          <div className="mb-4">
            <h3 className={sectionTitleClass}>润色接口</h3>
            <p className={sectionHintClass}>填任意 OpenAI 兼容的对话接口；地址填到 /v1 即可。</p>
          </div>

          <div className="space-y-4">
            <FormField label="接口地址">
              <input
                type="text"
                value={config.llm_base_url}
                spellCheck={false}
                autoComplete="off"
                placeholder="https://api.openai.com/v1"
                onChange={(event) => void updateConfig({ llm_base_url: event.target.value })}
                className={inputClass}
              />
            </FormField>

            <FormField label="模型名">
              <input
                type="text"
                value={config.llm_model}
                spellCheck={false}
                autoComplete="off"
                placeholder="gpt-4o-mini"
                onChange={(event) => void updateConfig({ llm_model: event.target.value })}
                className={inputClass}
              />
            </FormField>

            <FormField label="API Key">
              <ApiKeyField
                value={apiKey}
                onChange={setApiKey}
                onBlur={() => void persist(apiKey)}
                saved={saved}
                placeholder="sk-…（本地服务可留空）"
              />
            </FormField>

            <FormField label="润色风格">
              <SegmentedControl
                options={STYLE_OPTIONS}
                value={config.polish_style}
                onChange={(value) => void updateConfig({ polish_style: value as PolishStyle })}
              />
            </FormField>

            <div className="flex items-start justify-between gap-4 rounded-[10px] border border-border bg-bg-secondary/40 px-3 py-3">
              <div>
                <div className="text-[13px] text-text-primary">关闭模型思考（更快）</div>
                <p className="mt-0.5 text-[11px] leading-relaxed text-text-tertiary">
                  推理模型会先「思考」再输出，本地 qwen3 会因此慢到 8 秒以上；开启后直接出结果。
                  不支持该参数的云端服务会自动忽略。
                </p>
              </div>
              <Toggle
                checked={config.llm_disable_thinking}
                onChange={(checked) => void updateConfig({ llm_disable_thinking: checked })}
              />
            </div>

            {config.polish_style === 'custom' && (
              <FormField label="自定义要求" hint="会作为 system prompt 的一部分发送给润色模型。">
                <textarea
                  rows={5}
                  value={config.polish_custom_prompt}
                  spellCheck={false}
                  placeholder="例如：保留英文术语，不要改写数字，用简洁的书面语。"
                  onChange={(event) =>
                    void updateConfig({ polish_custom_prompt: event.target.value })
                  }
                  className={textareaClass}
                />
              </FormField>
            )}
          </div>

          <div className="mt-5 flex flex-wrap items-center gap-3">
            <button
              type="button"
              disabled={!canTest}
              onClick={() => void handleTest()}
              className={primaryButtonClass}
            >
              {testState.status === 'testing' ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <PlugZap size={14} />
              )}
              {testState.status === 'testing' ? '测试中…' : '测试连接'}
            </button>

            {testState.status === 'success' && (
              <span className="text-[12px] text-success">连接成功 · {testState.ms} ms</span>
            )}
            {testState.status === 'error' && (
              <span className="max-w-[360px] text-[12px] leading-relaxed text-error">
                {testState.message}
              </span>
            )}
          </div>
        </section>
      )}
    </div>
  )
}
