# Jot

桌面语音输入工具：按住快捷键说话，松手后文字直接落到当前光标处。

Jot 不提供账号、订阅或云端中转。转写和润色都调用你自己配置的接口，密钥保存在系统钥匙串里。

## 它怎么工作

```text
全局快捷键
   ↓
麦克风采集（16 kHz / 单声道 / 16-bit）
   ↓
转写
  ├─ OpenAI 兼容：录音封装成 WAV，按段或整段上传到 /audio/transcriptions
  └─ 火山引擎：PCM 分片通过 WebSocket 持续发送，边录边识别
   ↓
可选：LLM 润色（OpenAI 兼容 /chat/completions）
   ↓
键入当前光标处；失败则回退到剪贴板
```

- **流式听写**：录音时实时显示文本。OpenAI 兼容路径按固定间隔把新增音频分段送去转写，松手后默认丢弃分段结果，对整段音频再转写一次；火山引擎路径由 WebSocket 服务端给出最终结果。
- **边说边打字**：打开后，文本在录音时就键入光标处；结束时只补上还没键入的部分，并跳过润色。
- **润色可选**：关掉后直接输出转写原文。开启后如果调用失败，仍会输出原文，同时在胶囊里提示润色失败。
- **输出有回退**：优先直接键入；失败后自动改为写入剪贴板并粘贴。自动粘贴失败时，文字通常会留在剪贴板里供手动粘贴。

## 特性

| 能力 | 说明 |
|---|---|
| 全局快捷键 | 默认 `Ctrl+/`，可录制组合键；支持「按住说话」和「按一下开始」 |
| 流式听写 | 录音时实时显示识别文本；可选边说边打字 |
| 可选润色 | 任意 OpenAI 兼容对话接口；可关闭；失败自动回退原文 |
| 两种转写接入 | OpenAI 兼容 HTTP 接口；火山引擎原生流式 WebSocket（可选） |
| 自定义词典 | 一行一个术语，支持 `术语 = 说明`；用于 OpenAI 兼容转写和润色 |
| 输出 | 直接键入光标处；失败回退剪贴板；可恢复原剪贴板 |
| 平台 | Windows / macOS / Linux |
| 托盘 | 显示主窗口、开始/停止录音、设置、开机启动、退出 |
| 隐私 | 密钥存系统钥匙串；音频只发往你选择的转写服务 |

## 快速开始

### 从源码构建

需要：

- Node.js 20 或更新版本
- Rust 1.82 或更新版本
- Tauri 2 对应的平台构建依赖

Windows 还需要 WebView2 运行时和 MSVC C++ 构建工具（Windows 10/11 通常已带 WebView2）；macOS 需要 Xcode Command Line Tools。

```bash
npm install
npm run tauri dev      # 开发模式
npm run tauri build    # 打包安装程序
```

打包 Windows 安装包时，Tauri 会先下载 WiX 工具链（约 40MB）。网络访问 GitHub 不稳定时会报
`timeout: global` 而失败，这时先预取一次工具链，再只打 MSI：

```powershell
pwsh -File scripts/fetch-wix-toolset.ps1
npm run tauri build -- --bundles msi
```

产物在 `src-tauri/target/release/bundle/msi/`。想同时出 NSIS（`.exe`）安装包，需要同样能被
Tauri 下载到 NSIS 工具链；只想要一个可用安装包时，MSI 就够了。

Linux（Debian / Ubuntu 22.04 及以上）：

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
                 patchelf libasound2-dev libssl-dev libxdo-dev
```

其他发行版请安装等价包。如果发行版把托盘依赖拆成了 ayatana 包，安装 `libayatana-appindicator3-dev`。

Linux 的键盘输出还依赖会话类型：

- X11：需要 `xdotool`
- Wayland（非 KDE）：需要 `wtype`
- KDE Wayland：需要 `kwtype`

这些工具缺失时，应用会回退到剪贴板，不会阻断录音和转写。

macOS 首次运行需要在「系统设置 → 隐私与安全性 → 辅助功能」里勾选 Jot。没有授权时无法把文字键入其他窗口，应用会回退到剪贴板。

## 发版

版本号写在 `src-tauri/tauri.conf.json`，`package.json` 保持一致。然后：

```powershell
pwsh -File scripts/release.ps1
```

脚本会校验版本号一致、工作区干净，打 `v<版本>` 标签并推送，GitHub Actions 随即在三个平台
打包并发布 Release。

流水线定义在 `.github/workflows/release.yml`：

- 先跑前端类型检查 + 构建 + 后端单测，失败就不发布
- Windows 出 `.msi`，macOS 出 universal `.dmg`，Linux 出 `.AppImage` / `.deb`
- 校验 tag 与版本号一致，避免打出错版本的包
- 安装包未签名：Windows 会弹 SmartScreen，macOS 首次打开需右键「打开」

前提：仓库已推到 GitHub 并启用 Actions。

### 首次配置

打开「识别」页，至少填前两项：

1. **接口地址**：例如 `https://api.openai.com/v1`
2. **模型名**：例如 `whisper-1`
3. **API Key**：失焦即保存到系统钥匙串；本地服务可以留空

如果改用火山引擎接入，`stt_base_url` 和 `stt_model` 不参与该路径，改填 `stt_volc_resource_id` 和对应的 API Key；详见下文。

默认配置开启了润色，并指向本机 Ollama 的 `http://127.0.0.1:11434/v1` 和 `qwen3:1.7b`。如果本机没有安装 Ollama，先到「润色」页关掉开关，避免每次听写都多一次失败的润色请求；也可以把地址和模型换成云端服务。

## 配置说明

配置文件由应用自动保存，字段名使用 snake_case。主要配置项和默认值如下：

| 配置项 | 默认值 | 说明 |
|---|---|---|
| `hotkey` | `Ctrl+/` | 全局快捷键 |
| `hotkey_mode` | `hold` | `hold` 按住说话，`toggle` 按一下开始 |
| `max_recording_seconds` | `30` | 单次录音上限，范围 10–600 秒 |
| `stt_provider` | `openai` | `openai` 为 OpenAI 兼容接口，`volcengine` 为火山引擎流式 |
| `stt_base_url` | 空 | 转写接口地址 |
| `stt_model` | `whisper-1` | 转写模型名 |
| `stt_language` | `auto` | 识别语言；`auto` 不发送 `language` 字段 |
| `stt_volc_resource_id` | `volc.seedasr.sauc.duration` | 火山引擎 Resource ID |
| `polish_enabled` | `true` | 是否调用 LLM 润色 |
| `llm_base_url` | `http://127.0.0.1:11434/v1` | 润色接口地址 |
| `llm_model` | `qwen3:1.7b` | 润色模型名 |
| `polish_style` | `clean` | `minimal` / `clean` / `structured` / `custom` |
| `polish_custom_prompt` | 空 | 自定义润色要求；在 `polish_style` 为 `custom` 时填写 |
| `llm_disable_thinking` | `true` | 发送 `reasoning_effort=none`，跳过推理模型的思考阶段 |
| `dictionary` | 空 | 词典条目数组 |
| `streaming_enabled` | `true` | 是否启用流式听写；OpenAI 兼容路径控制分段请求，火山引擎路径控制中间文本显示 |
| `streaming_insert` | `false` | 是否边说边把新增文字键入光标处 |
| `streaming_interval_ms` | `1200` | OpenAI 兼容路径的分段间隔，范围 800–5000 毫秒 |
| `output_mode` | `keyboard` | `keyboard` 直接键入，`clipboard` 复制到剪贴板 |
| `paste_shortcut` | `ctrlV` | macOS 默认 `cmdV`；也可选 `ctrlShiftV`、`shiftInsert` |
| `restore_clipboard_after_paste` | `true` | 粘贴成功后恢复原剪贴板文本 |
| `theme` | `system` | `light` / `dark` / `system` |
| `auto_start` | `true` | 登录系统后自动启动 |
| `close_to_tray` | `true` | 关闭主窗口时继续在托盘运行 |
| `start_minimized` | `false` | 启动时不显示主窗口 |
| `capsule_auto_hide` | `true` | 空闲时自动隐藏胶囊 |

### 语音识别

#### OpenAI 兼容接口

填任意兼容 `/v1/audio/transcriptions` 的服务：

- `stt_base_url`：填到 `/v1` 即可，例如 `https://api.openai.com/v1`
- `stt_model`：服务商提供的模型名，例如 `whisper-1`、`whisper-large-v3`
- `stt_language`：选「自动检测」时不发送 `language` 字段；也可以填语言代码
- API Key：非空时以 `Authorization: Bearer <key>` 发送；本地服务可以留空

接口地址只会自动补最后一段路径，**不会自动补 `/v1`**：

- 填 `https://api.openai.com/v1` → 请求 `https://api.openai.com/v1/audio/transcriptions`
- 填 `https://api.openai.com/v1/audio/transcriptions` → 原样使用，不会重复拼接
- 填 `https://api.openai.com` → 请求 `https://api.openai.com/audio/transcriptions`，通常不是想要的结果

地址必须以 `http://` 或 `https://` 开头，不能包含空格或其他空白字符。

转写请求使用 `multipart/form-data`，包含 `file`（`audio.wav`，`audio/wav`）和 `model`；词典非空时额外发送 `prompt`。音频格式为 16 kHz、单声道、16-bit PCM。

端点拼装规则、请求字段、鉴权方式和错误码的细节见 [接入自己的接口](<docs/接入自己的接口.md>)。

常见搭配：

| 服务 | 接口地址 | 模型示例 |
|---|---|---|
| OpenAI | `https://api.openai.com/v1` | `whisper-1` |
| Groq | `https://api.groq.com/openai/v1` | `whisper-large-v3` |
| 硅基流动 | `https://api.siliconflow.cn/v1` | `FunAudioLLM/SenseVoiceSmall` |
| 本地 OpenAI 兼容服务 | `http://localhost:8000/v1` | 以服务端实际模型名为准 |

本地部署与「术语库怎么才能真正生效」的取舍，见 [本地 ASR 选型](<docs/本地ASR选型.md>)。

#### 火山引擎流式识别（可选）

把 `stt_provider` 设为 `volcengine` 后，走原生 WebSocket 流式识别：

- `stt_volc_resource_id` 可选：
  - `volc.seedasr.sauc.duration`（默认，SeedASR 流式）
  - `volc.bigasr.sauc.duration`（BigASR 流式）
- `stt_base_url` 和 `stt_model` 不参与这条路径；切回 `openai` 时它们仍然保留
- API Key 不能为空，支持两种写法：
  - 火山引擎语音识别控制台的 UUID 形式，发送为 `X-Api-Key`
  - 旧版 `app_key:access_key`，分别发送为 `X-Api-App-Key` 和 `X-Api-Access-Key`
- 方舟的大模型 Key 不能用于语音识别
- 火山引擎的 Key 与 OpenAI 兼容接口的 Key 分开保存在系统钥匙串里

语言会自动映射：`auto` 不发送；`zh` → `zh-CN`，`en` → `en-US`，`ja` → `ja-JP`，`ko` → `ko-KR`；其他写法原样传递（例如 `yue`）。

> 词典的转写偏置目前只作用于 OpenAI 兼容接口。火山引擎协议没有对应的词典字段；在这条路径下，词典说明仍会在润色时使用。

### 智能润色

润色调用任意 OpenAI 兼容的 `/v1/chat/completions`，请求以 SSE 流式读取，`temperature` 为 `0.3`。服务需要支持 `stream: true`。

- 地址填到 `/v1` 即可；已经以 `/chat/completions` 结尾时不会重复拼接
- API Key 非空时以 `Authorization: Bearer <key>` 发送；本地服务可以留空
- 关闭开关后完全不调用 LLM，直接输出转写原文
- 调用失败、超时或返回空文本时，输出原文，并在胶囊里提示润色失败

**「关闭模型思考」为什么快**：qwen3 这类推理模型默认会先输出几百字的思考过程，再给最终答案。Jot 在 `llm_disable_thinking` 开启时发送 `reasoning_effort: "none"`，让模型直接输出结果。在 RTX 5060 Ti 热态、同一段中文听写文本、6 次取中位的实测里（均关闭思考），`qwen3:1.7b` 单次约 148 ms，`qwen3:8b` 约 434 ms；`qwen3:1.7b` 开启思考时会从 0.15 秒级变成 8 秒以上。具体耗时取决于硬件、模型是否已加载以及量化方式。如果服务不认识这个参数并返回 4xx，应用会去掉该字段重试一次，避免整次润色失败。

润色风格：

| 值 | 名称 | 行为 |
|---|---|---|
| `minimal` | 极简 | 只修明显的错别字和标点，尽量保留原句结构 |
| `clean` | 清爽 | 去口头语、重复词，整理成通顺的书面语（默认） |
| `structured` | 结构化 | 在保留原意的前提下分段、整理并列要点 |
| `custom` | 自定义 | 按 `polish_custom_prompt` 的要求处理 |

云端替代方案：

| 服务 | 接口地址 | 模型示例 |
|---|---|---|
| OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` |
| DeepSeek | `https://api.deepseek.com/v1` | `deepseek-chat` |
| 月之暗面 | `https://api.moonshot.cn/v1` | `moonshot-v1-8k` |
| 硅基流动 | `https://api.siliconflow.cn/v1` | `Qwen/Qwen2.5-7B-Instruct` |
| 阿里云百炼 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` |
| 本机 Ollama | `http://127.0.0.1:11434/v1` | `qwen3:1.7b` |

模型名会随服务商调整，上表只给常用组合；以服务商控制台为准。本机模型的安装、选型和延迟说明见 [本地模型](<docs/本地模型.md>)。

### 词典

一行一个条目，支持两种写法：

```text
联调
联调 = 接口联合调试，不是「连调」
SeedASR：火山引擎流式识别
```

- `术语`：OpenAI 兼容转写时作为提示词偏置；润色时也作为专有名词提示
- `术语 = 说明`：转写只使用「术语」本身；「说明」只会在润色时交给模型
- 分隔符支持 `=` 和全角 `：`；ASCII 冒号不切分，避免把 URL 或时间拆坏
- 同一个术语只保留第一条；最多 500 条，单条最多 64 个字符
- 词典部分的转写提示词最多 200 个字符，只取术语，用 `、` 连接；流式分段时还会附加最多 200 个字符的上文。重要术语放在前面
- 润色提示词最多带 100 条术语及说明

词典是提示，不是强制替换。模型仍可能写错生僻词；重要术语尽量放前面，并确认已经点「保存」。

### 通用与输出

- **快捷键**：支持 A–Z、0–9、F1–F12、Space、Enter、Tab、Escape 以及 `/ . , ; - = [ ]`；可以带 Ctrl / Alt / Shift / Super（Command）修饰键，也可以不带走修饰键
- **按住 / 按一下**：`hold` 在松开时结束；`toggle` 再按一次结束
- **录音上限**：到达 `max_recording_seconds` 后自动结束并开始转写
- **流式听写**：OpenAI 兼容路径关闭后不发送分段请求，松手后一次性转写；火山引擎路径仍会通过 WebSocket 流式识别，只是不显示中间文本
- **边说边打字**：需要同时开启流式听写；开启后跳过润色
- **输出方式**：`keyboard` 直接键入，失败自动回退剪贴板；`clipboard` 只走剪贴板
- **粘贴快捷键**：在终端等应用里，`Ctrl+V` 可能不生效，可以换成 `Ctrl+Shift+V` 或 `Shift+Insert`
- **恢复剪贴板**：仅当粘贴成功且剪贴板内容仍是刚插入的文本时，约 0.75 秒后恢复原文本

## 使用

1. 把光标放进要输入的地方：编辑器、聊天框、浏览器地址栏都可以
2. 按下全局快捷键，屏幕底部出现胶囊
3. 按住说话，或按一下开始；松开 / 再按一次结束
4. 胶囊显示转写、润色、完成或错误状态；文字会出现在光标处

托盘菜单：

- 显示主窗口
- 开始录音 / 停止录音
- 设置
- 开机启动
- 退出

左键单击托盘图标会显示主窗口。

### 录不上、没打进去怎么排查

先看胶囊里的提示，再按顺序检查：

1. **麦克风**：系统录音权限是否给了 Jot；输入设备是否被静音或被其他应用独占
2. **快捷键**：打开「通用」页，看快捷键是否注册成功；失败时通常会提示被其他程序占用
3. **转写接口**：在「识别」页点「测试连接」；确认接口地址是否需要在末尾带 `/v1`、模型名正确、Key 属于当前服务
4. **权限**：macOS 检查辅助功能；Linux 检查 `xdotool` / `wtype` / `kwtype` 是否存在
5. **目标窗口**：管理员权限窗口、部分游戏、远程桌面和虚拟机可能拦截模拟输入，改用「复制到剪贴板」
6. **Wayland**：全局快捷键和自动粘贴可能不可靠；可以先用托盘菜单开始录音，再手动粘贴

## 数据与隐私

- **API Key**：保存在操作系统钥匙串（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service）。不写入配置文件，也不随设置导出。
- **配置文件**：应用数据目录下的 `settings.json`，键为 `app_config`。包含接口地址、模型名、词典等，不包含任何密钥。词典里如果有敏感词，注意该文件的访问权限。
- **音频**：录音期间只存在于内存，不落盘。OpenAI 兼容路径开启流式听写时按段上传 WAV；结束时上传剩余音频（默认是整段，开启边说边打字时是尾段）。火山引擎路径在录音过程中持续发送 PCM 分片。两者都直接连接你选择的转写服务，不经过 Jot 的中转服务器。
- **润色**：开启润色时，转写文本、词典术语及其说明，以及（如果设置了）自定义要求会发送到你配置的对话接口。关闭润色后不会发送。
- **没有账号、订阅、埋点或云端中转**。如果选择云端转写或云端润色，数据会到达对应服务商。

## 常见问题

**按快捷键没反应？**

先看「通用」页的快捷键注册状态。注册失败通常是被其他程序占用，换一个组合键即可；应用会在后台重试，前 3 次每 5 秒，之后每 30 秒。Linux Wayland 下全局快捷键可能不可靠，可以先用托盘菜单的「开始录音」。

**文字没打进去？**

管理员权限窗口、部分游戏、远程桌面和虚拟机可能拦截模拟输入。把「输出方式」改成「复制到剪贴板」再手动粘贴。macOS 还需要辅助功能权限；Linux X11 需要 `xdotool`，Wayland 需要 `wtype` 或 `kwtype`。Wayland 下剪贴板通常只能复制，不能自动粘贴。

**转写很慢？**

使用 OpenAI 兼容转写时，默认在松手后会对整段音频再做一次完整转写，流式结果只用于预览，所以最终速度主要取决于接口的网络和算力。可以换更近的服务、缩短录音，或打开「边说边打字」；后者结束时只补还没键入的部分，不再整体重转。如果使用火山引擎，检查网络到 WebSocket 端点的连通性。

**润色结果不如原文？**

在「润色」页顶部关掉开关，直接输出转写原文。也可以把风格改成「极简」，减少模型改写。词典里的说明能帮助模型选对同音字，但不要指望它强制替换所有术语。

**润色很慢？**

先确认「关闭模型思考」是开启状态。本机推理模型开启思考时会先输出一段很长的内部推理；关闭后只输出结果。首次请求还可能包含模型加载时间。仍然慢的话，换更小的本机模型或云端接口。

**提示转写鉴权失败？**

检查 Key 是否属于当前接口。OpenAI 兼容接口和火山引擎的 Key 分开保存；切换接入方式后不要沿用另一边的 Key。火山引擎要填语音识别的 Key，不能填方舟的大模型 Key；旧版 `app_key:access_key` 也可以。

**提示没有检测到语音？**

检查麦克风是否静音、系统输入设备是否选错、说话是否太短或太轻。录到空音频、或者接口返回空文本时，都会显示这个提示。

**词典不生效？**

词典是提示词偏置，不是硬替换。确认已经保存；词典部分只取前 200 个字符，把最重要的术语放在前面。火山引擎路径不使用转写词典，但润色仍会使用词典说明。

**剪贴板被改掉了？**

默认开启「粘贴后恢复剪贴板」，从粘贴成功起约 0.75 秒后恢复。如果这期间其他程序改写了剪贴板，应用不会覆盖它。不想要这个行为，可以在「通用」页关闭。

## 许可

MIT。
