# Jot 接口契约

前端（`src/`）与后端（`src-tauri/`）之间的唯一约定：配置字段、invoke 命令、事件名、
端点推导与管线行为。两侧都必须按此实现，任何一方不得自行改名、增删字段。

## 产品定义

桌面语音输入工具：

1. 按全局快捷键开始录音，再按一次（或松开）结束
2. 把音频交给用户选择的转写供应商：
   - `openai`（默认）：用户自己的 OpenAI 兼容批式转写接口
   - `volcengine`：火山引擎（豆包 / SeedASR）原生 WebSocket 流式识别
3. 如果开启「LLM 润色」，把转写结果交给用户自己的 OpenAI 兼容对话接口改写
4. 把最终文本键入到当前光标处（失败/配置为剪贴板时改为粘贴，并保留剪贴板内容）

**支持流式听写**：录音过程中按固定间隔把新增音频分段送去转写，胶囊里实时出字
（事件 `stt:partial`）。松手后默认对整段音频再做一次完整转写作为最终稿，
保证最终结果质量；若用户开启「边说边打字」，则改为把增量实时键入光标处，
结束时只补转写最后一段，不再整体重转。

`stt_provider == "volcengine"` 时走**原生流式**路径：录音开始即建立 WebSocket，
边发音频边收识别结果；松手后发送结束帧并等待最终稿，**不再对整段音频重转**。
该路径下 `streaming_enabled` 只控制是否实时显示/实时键入，不影响是否使用流式协议。

没有账号、没有云服务、没有订阅、没有历史库、没有场景、没有翻译、没有语音指令。

## 跨平台要求

目标平台：Windows / macOS / Linux，三者都必须能编译通过。

- 平台相关代码一律用 `#[cfg(target_os = "...")]` 隔离，不得在通用路径里调用 Windows 专有 API
- 保留并修复 `src/output/windows_sendinput.rs`（仅 Windows）、`src/native_hotkey.rs`（仅 macOS）、
  `src/linux_x11.rs`（仅 Linux）等既有平台模块
- 粘贴快捷键按平台给默认值：macOS `cmdV`，其余 `ctrlV`
- 钥匙串依赖按平台选择 `keyring` feature（Cargo.toml 里已有，不要删）
- 磁盘路径、换行符、括号/符号键位映射不得硬编码为 Windows 习惯
- 无法在本机验证的平台，至少保证 `cargo check --target` 级别的语法与 cfg 正确性，
  并且不留 `#[cfg(windows)]` 之外对 `windows-sys` 的引用

## 配置

持久化方式：`tauri-plugin-store`，文件名 `settings.json`，键 `app_config`
（即 `app.store("settings.json")`）。

```jsonc
{
  "hotkey": "Ctrl+/",                 // 全局快捷键，accelerator 语法
  "hotkey_mode": "hold",              // "hold" 按住说话 | "toggle" 按一下开始
  "max_recording_seconds": 30,        // 10..600

  "stt_provider": "openai",           // "openai"（默认，OpenAI 兼容）| "volcengine"
  "stt_volc_resource_id": "volc.seedasr.sauc.duration",
                                       // 仅 volcengine：volc.seedasr.sauc.duration
                                       // | volc.bigasr.sauc.duration
  "stt_base_url": "",                 // 仅 openai，例 "https://api.openai.com/v1"
  "stt_model": "whisper-1",           // 仅 openai
  "stt_language": "auto",             // 两个供应商共用；"auto" 时不发送 language 字段

  "polish_enabled": true,             // false 时完全跳过 LLM，直接输出原始转写
  "llm_base_url": "http://127.0.0.1:11434/v1", // 默认本机 Ollama；可改成任意 OpenAI 兼容服务
  "llm_model": "qwen3:1.7b",
  "polish_style": "clean",            // "minimal" | "clean" | "structured" | "custom"
  "polish_custom_prompt": "",
  "llm_disable_thinking": true,       // 发送 reasoning_effort=none，跳过推理模型的思考阶段

  "dictionary": [],                   // 一行一个条目，可用「术语 = 说明」补充解释

  "streaming_enabled": true,          // 录音时边录边转写，胶囊实时出字
  "streaming_insert": false,          // 边说边把新增文字键入光标处（关闭时结束时一次性插入）
  "streaming_interval_ms": 1200,      // 分段间隔，800..5000

  "output_mode": "keyboard",          // "keyboard" | "clipboard"
  "paste_shortcut": "ctrlV",          // "ctrlV" | "ctrlShiftV" | "shiftInsert" | "cmdV"（macOS 默认 cmdV）
  "restore_clipboard_after_paste": true,

  "theme": "system",                  // "light" | "dark" | "system"
  "auto_start": true,
  "close_to_tray": true,
  "start_minimized": false,
  "ui_language": "zh",                // 仅 "zh" 保留字段，界面固定中文
  "capsule_auto_hide": true
}
```

字段名、类型、默认值必须与此处完全一致（snake_case）。未知字段读入时忽略，缺失字段用默认值。
`update_config` 接收的是**局部 patch**，后端做深合并后持久化并返回完整配置。

### API Key 存储

用系统钥匙串（`credentials.rs` 已有的 keyring 封装），不写进 settings.json：

- 转写密钥（OpenAI 兼容）：namespace `"stt"`, provider `"custom"`
- 转写密钥（火山引擎）：namespace `"stt"`, provider `"volcengine"`
- 润色密钥：namespace `"llm"`, provider `"custom"`

两个转写供应商的密钥分开存储，切换供应商不会覆盖对方。

`get_credential_status().stt` 仍只反映 OpenAI 兼容密钥；火山引擎密钥用
`read_credential { namespace: "stt", provider: "volcengine" }` 读取/判断。

转写密钥（钥匙串中的 `stt`/`custom`）、`stt_base_url`、`stt_model` 只在
`stt_provider == "openai"` 时使用；`stt_volc_resource_id` 只在
`stt_provider == "volcengine"` 时使用。

### 词典条目

`dictionary` 是字符串数组，每行一个条目，支持两种写法：

```text
联调
联调 = 接口联合调试，不是「连调」
```

解析规则（见 `src-tauri/src/dictionary.rs`）：以第一个 `=` 或 `：` 分隔「术语」与「说明」，
ASCII 冒号不参与分隔（避免切坏 URL 和时间）。转写偏置只用术语部分；润色提示词里
写成 `术语（说明）`。转写偏置仅对 OpenAI 兼容供应商生效，火山引擎协议没有对应字段。

## 端点推导（仅 OpenAI 兼容供应商）

- 转写：`{stt_base_url}` 去掉末尾 `/`；若已以 `/audio/transcriptions` 结尾则直接用，
  否则拼 ` + "/audio/transcriptions"`。
- 对话：`{llm_base_url}` 同样处理；已以 `/chat/completions` 结尾则直接用，
  否则拼 ` + "/chat/completions"`。
- 认证：密钥非空时带 `Authorization: Bearer <key>`；为空不带。
- 转写请求：`multipart/form-data`，字段 `file`（`audio.wav`, `audio/wav`）、`model`；
  `stt_language` 不为 `auto`/空时加 `language`；`dictionary` 非空时加 `prompt`
  （词用「、」连接，截断到 200 字符以内）。
- 转写响应：JSON，取 `text` 字段。
- 对话请求：JSON `{ "model", "messages": [{"role":"system","content":...},{"role":"user","content":...}], "stream": true, "temperature": 0.3 }`，
  按 SSE 解析 `data:` 行，取 `choices[0].delta.content` 增量。

## 火山引擎流式供应商

- 端点固定为 `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async`。
- 请求头：`X-Api-Resource-Id` 取 `stt_volc_resource_id`，`X-Api-Connect-Id` 每次连接唯一。
- 鉴权二选一：
  - 密钥是一段 UUID（不含 `:`）→ `X-Api-Key: <key>`
  - 密钥是 `app_key:access_key`（两段都非空）→ `X-Api-App-Key` + `X-Api-Access-Key`
  - 方舟（Ark）大模型密钥不能用于语音识别。
- 音频格式：PCM 16 kHz、16-bit、单声道；请求 JSON 的 `audio.language` 使用
  `stt_language`，`auto` 时**省略该字段**；`enable_itn`/`enable_punc` 固定开启。
- 并发模型：握手后把 WebSocket 拆成 sink / stream 两半，读任务把服务端帧解析成
  `Partial` / `Final` / `Error` 推进 mpsc 通道，写任务从另一条 mpsc 通道取音频帧写出。
  `send_audio` 只入队、不等待网络；`recv_transcript` 只用 `try_recv`，无事件返回
  `Ok(None)`，因此 30 ms 轮询里可以同时收发。
- 结束：发送负序号音频帧（结束帧），最多等待 10 秒最终结果；超时返回已有部分文本
  或空，不会永久挂住。
- 错误映射：
  - 握手 401/403 或服务端鉴权类错误码/文案 → `stt_auth`
  - 连接、协议、解析、超时 → `stt_failed`
  - 未配置 Resource ID 或密钥 → `stt_not_configured`

## 流式听写行为（OpenAI 兼容供应商）

以下规则仅适用于 `stt_provider == "openai"`；火山引擎的原生流式路径见下一节。

`streaming_enabled == true` 时，录音期间启动分段转写任务：

- 每 `streaming_interval_ms` 取「自上一个切点以来的新增 PCM」；累积不足 400ms 则跳过本轮
- 每轮请求都带上 `prompt`：已确认文本的末尾 200 字符（保持上下文与术语一致）
- 返回文本与上一轮临时结果做**去重拼接**（剥离重复的公共前后缀），得到累积全文后
  emit `stt:partial`（全文，不是增量）
- 分段请求失败**不中断录音**，记 warning 后继续下一轮
- 停止录音时：
  - `streaming_insert == false`（默认）：丢弃分段结果，对**整段音频**做一次完整转写作为最终稿
    —— 准确率优先，emit `stt:final`
  - `streaming_insert == true`：只补转写最后一段尾部音频，与已键入内容拼接，
    emit `stt:final` 为拼接后的全文，结束时不再整体插入
- `streaming_insert == true` 时，每段新增文本在录制过程中就立刻走输出管线键入
- `streaming_enabled == false`：完全不发分段请求，退回一次性批式转写

分段任务必须能被 `stop()` / `abort()` 取消，且不得与其竞争同一份音频缓冲
（用取消令牌 + 互斥锁，audio 缓冲按切点移交所有权）。

## 流式听写行为（火山引擎）

- `start()`：打开麦克风的同时建立 WebSocket；录音期间约 30 ms 轮询一次：
  把「自上一个切点以来的新增 PCM」`send_audio` 出去，再 `recv_transcript` 取事件。
- 收到 `Partial`：更新累积文本，emit `stt:partial`（全文，不是增量）。
- 收到 `Final`：更新累积文本，不重新 emit `stt:partial`。
- `streaming_enabled == false`：不 emit `stt:partial`、不实时键入，但仍然走流式协议。
- `streaming_enabled == true && streaming_insert == true`：把相对已键入文本的新增后缀
  在录音过程中实时键入光标；结束时只补最后一段差异。
- `stop()`：停止采集、把剩余音频喂完、发送结束帧、`disconnect()` 拿最终稿，
  emit `stt:final`，然后照常润色/插入；**不对整段音频重转**。
- `abort()`：立即取消轮询并断开 WebSocket，不等待最终结果。
- `streaming_interval_ms` 仅用于 OpenAI 兼容供应商的分段节奏；火山引擎路径固定
  约 30 ms 轮询一次。

## Tauri command（invoke 名 / 参数 / 返回）

参数一律用 camelCase 的默认（Tauri 会把 JS 的 `apiKey` 映射到 Rust 的 `api_key`）。

| command | 参数 | 返回 |
|---|---|---|
| `get_config` | – | `AppConfig` |
| `update_config` | `{ patch: object }` | `AppConfig`（合并后完整配置） |
| `set_auto_start` | `{ enabled: boolean }` | `AppConfig` |
| `set_capsule_auto_hide` | `{ enabled: boolean }` | `AppConfig` |
| `get_app_info` | – | `{ version: string, os: string }` |
| `read_credential` | `{ namespace: string, provider: string }` | `string \| null` |
| `set_credential` | `{ namespace: string, provider: string, secret: string }` | `null` |
| `clear_credential` | `{ namespace: string, provider: string }` | `null` |
| `get_credential_status` | – | `{ stt: boolean, llm: boolean }` |
| `test_stt_connection` | `{ apiKey: string, baseUrl: string, model: string, language: string, provider: "openai"\|"volcengine", resourceId: string }` | `number`（毫秒） |
| `bench_llm_connection` | `{ apiKey: string, baseUrl: string, model: string, disableThinking: boolean }` | `number`（毫秒） |
| `fetch_llm_models` | `{ apiKey: string, baseUrl: string }` | `string[]` |
| `apply_hotkey` | `{ hotkey: string }` | `null`（注册失败时 reject，错误含可读原因） |
| `pause_hotkey` | – | `null` |
| `resume_hotkey` | – | `null` |
| `get_hotkey_registration_error` | – | `string \| null` |
| `get_platform_capabilities` | – | `{ os: "windows"\|"macos"\|"linux"\|"unknown", sessionType: string, globalHotkeyReliable: boolean, keyboardOutputReliable: boolean, clipboardAutoPasteReliable: boolean }` |
| `check_accessibility_permission` | – | `boolean` |
| `request_accessibility_permission` | – | `boolean` |
| `start_recording` | – | `null` |
| `stop_recording` | – | `null` |
| `abort_recording` | – | `null` |

`update_config` 发现 `hotkey` 变化时必须重新注册全局快捷键；失败时保留旧注册，并 reject
错误，但配置里的 `hotkey` 字段仍写入（下次启动会重试）。

`test_stt_connection` 按 `provider` 分派：

- `openai`：上传 100 ms 静音 WAV 到 `baseUrl`/`model`，沿用原有行为。
- `volcengine`：用 `apiKey`（为空时读钥匙串中的 `stt`/`volcengine`）与 `resourceId`
  建立一次真实 WebSocket 连接，握手成功后发送 200 ms 静音 PCM 和结束帧，然后断开。
  连接/握手成功即返回耗时（识别结果为空也算成功）；鉴权失败返回可读错误。
  总超时 15 秒。

## 事件（后端 emit → 前端 listen）

| 事件名 | payload |
|---|---|
| `audio:volume` | `number` 0..1 |
| `pipeline:state` | `"idle" \| "preparing" \| "recording" \| "transcribing" \| "polishing" \| "outputting"` |
| `pipeline:error` | `{ code: string, details?: string }` |
| `stt:final` | `string`（原始转写） |
| `stt:partial` | `string`（录音过程中累积的临时转写全文，不是增量） |
| `llm:chunk` | `string`（润色增量，仅用于胶囊实时预览） |
| `recording:deadline` | `{ sessionId: number, startedAtUnixMs: number, deadlineAtUnixMs: number, effectiveMaxSeconds: number }` |
| `pipeline:insert_result` | `{ status: "inserted"\|"copiedFallback"\|"failed"\|"partiallyInserted", strategyUsed: string, charsInserted: number, charsCopied: number, warningCode: string\|null, message: string\|null }` |
| `hotkey:registration-failed` | `string` |
| `hotkey:registration-recovered` | `null` |
| `config:patch` | 局部 `AppConfig` |
| `tray:navigate` | `string`（如 `"#/settings"`） |

`pipeline:error.code` 取值（前端据此显示中文）：

`mic_unavailable` `stt_not_configured` `stt_auth` `stt_failed` `no_speech`
`llm_not_configured` `llm_failed` `output_failed` `accessibility_required` `internal`

## 窗口

- `main`：设置主窗口（900×700，可缩放，居中，默认不显示，由 setup 显示）
- `capsule`：录音浮层（`index.html#capsule`，透明、无边框、置顶、不占任务栏、默认隐藏）
- 不再有 `ask` 窗口

## 托盘菜单（中文）

`显示主窗口` / `开始录音`（录音中变成 `停止录音`）/ `设置` / `开机启动`（勾选态）/ `退出`
左键单击托盘图标 = 显示主窗口。

## 视觉规范

沿用现有 Tailwind 主题令牌：`bg-bg-primary` `bg-bg-secondary` `bg-bg-elevated` `text-text-primary`
`text-text-secondary` `text-text-tertiary` `border-border` `text-accent` `bg-accent` `text-error`
`text-success` `shadow-matrix`（用 `shadow-md`/`shadow-lg`/`shadow-float`）。圆角用 `rounded-[10px]`
一类任意值。界面文案固定中文，不引入 i18n。
