import { useEffect, useState } from 'react'
import { AlertTriangle, Loader2, Mic, PlugZap } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import {
  formatError,
  testSttConnection,
  type SttProvider,
  type VolcResourceId,
} from '../../lib/tauri'
import { useCredential } from './shared/useCredential'
import { ApiKeyField } from './shared/ApiKeyField'
import { FormField } from './shared/FormField'
import { SegmentedControl } from './shared/SegmentedControl'
import {
  bannerClass,
  inputClass,
  primaryButtonClass,
  sectionClass,
  sectionHintClass,
  sectionTitleClass,
  selectClass,
} from './shared/ui'

const LANGUAGE_OPTIONS = [
  { value: 'auto', label: '自动检测' },
  { value: 'zh', label: '中文' },
  { value: 'en', label: '英文' },
  { value: 'ja', label: '日文' },
  { value: 'ko', label: '韩文' },
  { value: 'other', label: '其他' },
]

const KNOWN_LANGUAGES = ['auto', 'zh', 'en', 'ja', 'ko']

const PROVIDER_OPTIONS = [
  { value: 'openai', label: 'OpenAI 兼容' },
  { value: 'volcengine', label: '火山引擎' },
]

const RESOURCE_OPTIONS: { value: VolcResourceId; label: string }[] = [
  { value: 'volc.seedasr.sauc.duration', label: 'SeedASR 流式' },
  { value: 'volc.bigasr.sauc.duration', label: '大模型流式' },
]

type TestState =
  | { status: 'idle' }
  | { status: 'testing' }
  | { status: 'success'; ms: number }
  | { status: 'error'; message: string }

export function SpeechPane() {
  const config = useAppStore((state) => state.config)
  const updateConfig = useAppStore((state) => state.updateConfig)
  const credentialProvider = config.stt_provider === 'volcengine' ? 'volcengine' : 'custom'
  const { apiKey, setApiKey, saved, persist, loading, loadedProvider } = useCredential(
    'stt',
    credentialProvider,
  )
  const credentialReady = !loading && loadedProvider === credentialProvider

  const [testState, setTestState] = useState<TestState>({ status: 'idle' })

  useEffect(() => {
    setTestState({ status: 'idle' })
  }, [config.stt_provider])

  const isVolcengine = config.stt_provider === 'volcengine'
  const usingPresetLanguage = KNOWN_LANGUAGES.includes(config.stt_language)
  const selectedLanguage = usingPresetLanguage ? config.stt_language : 'other'
  const customLanguage = usingPresetLanguage ? '' : config.stt_language

  const handleProviderChange = (value: string) => {
    setTestState({ status: 'idle' })
    void updateConfig({ stt_provider: value as SttProvider })
  }

  const handleLanguageChange = (value: string) => {
    if (value === 'other') {
      if (usingPresetLanguage) void updateConfig({ stt_language: '' })
      return
    }
    void updateConfig({ stt_language: value })
  }

  const handleTest = async () => {
    setTestState({ status: 'testing' })
    try {
      const ms = await testSttConnection(
        apiKey.trim(),
        config.stt_base_url.trim(),
        config.stt_model.trim(),
        config.stt_language,
        config.stt_provider,
        config.stt_volc_resource_id,
      )
      setTestState({ status: 'success', ms })
    } catch (error) {
      setTestState({ status: 'error', message: formatError(error) })
    }
  }

  const canTest = isVolcengine
    ? config.stt_volc_resource_id.trim().length > 0 &&
      credentialReady &&
      (saved || apiKey.trim().length > 0) &&
      testState.status !== 'testing'
    : config.stt_base_url.trim().length > 0 &&
      config.stt_model.trim().length > 0 &&
      testState.status !== 'testing'

  const missingConfiguration = isVolcengine
    ? credentialReady && !saved && !apiKey.trim()
    : !config.stt_base_url.trim()

  const languageField = (
    <FormField
      label="识别语言"
      hint={
        isVolcengine
          ? '选择「自动检测」时不会向火山引擎发送 language 字段。'
          : '选择「自动检测」时不会向服务发送 language 字段。'
      }
    >
      <div className="flex items-center gap-2">
        <select
          value={selectedLanguage}
          onChange={(event) => handleLanguageChange(event.target.value)}
          className={`${selectClass} max-w-[200px]`}
        >
          {LANGUAGE_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        {selectedLanguage === 'other' && (
          <input
            type="text"
            value={customLanguage}
            spellCheck={false}
            autoComplete="off"
            placeholder="语言代码，如 pt"
            onChange={(event) => void updateConfig({ stt_language: event.target.value })}
            className={`${inputClass} max-w-[180px]`}
          />
        )}
      </div>
    </FormField>
  )

  return (
    <div>
      <section className={sectionClass}>
        <div className="mb-4 flex items-start gap-2">
          <Mic size={15} className="mt-0.5 text-accent" />
          <div>
            <h3 className={sectionTitleClass}>转写供应商</h3>
            <p className={sectionHintClass}>
              两种供应商的密钥分开保存在系统钥匙串，切换后互不影响。
            </p>
          </div>
        </div>
        <SegmentedControl
          options={PROVIDER_OPTIONS}
          value={config.stt_provider}
          onChange={handleProviderChange}
        />
      </section>

      {missingConfiguration && (
        <div className={`${bannerClass} mb-6`}>
          <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warning" />
          <div>
            {isVolcengine ? (
              <>
                <p className="font-medium text-text-primary">
                  还没有填写火山引擎 API Key，先填好下面的密钥才能开始使用。
                </p>
                <p className="mt-1 text-text-secondary">
                  可以直接填语音识别 API Key（UUID），或旧版控制台的 app_key:access_key。
                </p>
              </>
            ) : (
              <>
                <p className="font-medium text-text-primary">
                  还没有配置转写接口，先填好下面两项才能开始使用。
                </p>
                <p className="mt-1 text-text-secondary">
                  填好接口地址和模型名后，说一句话就能把文字输入到当前应用。
                </p>
              </>
            )}
          </div>
        </div>
      )}

      {isVolcengine ? (
        <section className={sectionClass}>
          <div className="mb-4 flex items-start gap-2">
            <Mic size={15} className="mt-0.5 text-accent" />
            <div>
              <h3 className={sectionTitleClass}>火山引擎流式识别</h3>
              <p className={sectionHintClass}>
                豆包 / SeedASR 原生流式识别：录音时边发音频边收文字，松手后直接使用最终稿。
              </p>
            </div>
          </div>

          <div className="space-y-4">
            <FormField
              label="Resource ID"
              hint="流式识别默认选 SeedASR；需要大模型识别时选「大模型流式」。"
            >
              <select
                value={config.stt_volc_resource_id}
                onChange={(event) =>
                  void updateConfig({
                    stt_volc_resource_id: event.target.value as VolcResourceId,
                  })
                }
                className={selectClass}
              >
                {RESOURCE_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}（{option.value}）
                  </option>
                ))}
              </select>
            </FormField>

            <FormField
              label="API Key"
              hint="支持两种写法：一段 UUID（使用 X-Api-Key），或 app_key:access_key（使用 App Key 鉴权）。方舟大模型密钥不能用于语音识别。"
            >
              <ApiKeyField
                value={apiKey}
                onChange={setApiKey}
                onBlur={() => void persist(apiKey)}
                saved={saved}
                disabled={!credentialReady}
                placeholder="UUID 或 app_key:access_key"
              />
            </FormField>

            {languageField}
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
      ) : (
        <section className={sectionClass}>
          <div className="mb-4 flex items-start gap-2">
            <Mic size={15} className="mt-0.5 text-accent" />
            <div>
              <h3 className={sectionTitleClass}>转写接口</h3>
              <p className={sectionHintClass}>使用你自己的 OpenAI 兼容语音转写服务。</p>
            </div>
          </div>

          <div className="space-y-4">
            <FormField
              label="接口地址"
              hint="填到 /v1 即可；也可以直接填完整的 /audio/transcriptions 地址。"
            >
              <input
                type="text"
                value={config.stt_base_url}
                spellCheck={false}
                autoComplete="off"
                placeholder="https://api.openai.com/v1"
                onChange={(event) => void updateConfig({ stt_base_url: event.target.value })}
                className={inputClass}
              />
            </FormField>

            <FormField label="模型名" hint="取决于服务提供方，例如 whisper-1、whisper-large-v3。">
              <input
                type="text"
                value={config.stt_model}
                spellCheck={false}
                autoComplete="off"
                placeholder="whisper-1"
                onChange={(event) => void updateConfig({ stt_model: event.target.value })}
                className={inputClass}
              />
            </FormField>

            <FormField label="API Key" hint="本地服务不需要密钥时可以留空。">
              <ApiKeyField
                value={apiKey}
                onChange={setApiKey}
                onBlur={() => void persist(apiKey)}
                saved={saved}
                disabled={!credentialReady}
                placeholder="sk-…（可留空）"
              />
            </FormField>

            {languageField}
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

      <div className="rounded-[10px] border border-border bg-bg-secondary/40 px-4 py-3 text-[12px] leading-relaxed text-text-secondary">
        {isVolcengine ? (
          <>
            火山引擎使用原生 WebSocket 流式识别，不会在松手后对整段音频重新转写。
            API Key 只保存在系统钥匙串，可填一段 UUID，或旧版控制台的{' '}
            <span className="font-mono text-[11px]">app_key:access_key</span>。
          </>
        ) : (
          <>
            填任意 OpenAI 兼容的{' '}
            <span className="font-mono text-[11px]">/v1/audio/transcriptions</span>{' '}
            服务即可，例如 OpenAI、Groq、硅基流动，或本地运行的 speaches / faster-whisper。
            接口地址通常填到 <span className="font-mono text-[11px]">/v1</span> 为止。
          </>
        )}
      </div>
    </div>
  )
}
