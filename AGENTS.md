# AGENTS.md

给在此仓库工作的 AI 代理与贡献者的工程说明。产品说明见 [README](README.md)。

## 这是什么

Jot —— 跨平台桌面语音输入工具。Tauri 2（Rust 后端 + React 19 前端）。

核心链路：全局快捷键 → 录音 → 转写 →（可选）润色 → 键入光标处。
没有账号、云服务、历史库；密钥进系统钥匙串。

## 目录

```
src/                     前端（React + TypeScript + Tailwind 4）
  App.tsx                两个窗口入口：#capsule / 主窗口
  components/Capsule/    录音浮层（录音/转写/润色/完成/错误）
  components/Settings/   设置页：通用 / 识别 / 润色 / 词典 / 关于
  stores/appStore.ts     zustand：配置 + 管线状态
  lib/tauri.ts           所有 invoke 封装与类型（命令名的唯一来源）
  lib/platform.ts        按平台格式化修饰键、粘贴快捷键
  hooks/                 事件订阅、主题、胶囊自适应

src-tauri/               后端（Rust）
  src/pipeline.rs        管线状态机：录音 → 转写 → 润色 → 输出
  src/stt/               SttProvider 抽象
    whisper_compat.rs    OpenAI 兼容批式（整段 WAV 上传）
    volcengine.rs        火山引擎原生流式（WebSocket + 自定义二进制帧）
  src/llm/               OpenAI 兼容对话客户端 + 中文提示词
  src/output/            键入 / 剪贴板 / Windows SendInput
  src/hotkey.rs          全局快捷键注册与重试
  src/storage/mod.rs     AppConfig + ConfigManager（tauri-plugin-store）
  src/credentials.rs     系统钥匙串读写
  src/dictionary.rs      词典条目解析（术语 / 说明）

.github/workflows/       ci.yml（三平台检查）、release.yml（打标签发布）
scripts/                 release.ps1、fetch-wix-toolset.ps1
```

## 常用命令

```bash
npm run tauri dev                 # 开发
npm run tauri build -- --bundles msi   # 只出 MSI（Windows）
npx tsc --noEmit                  # 前端类型检查
npm run build                     # 前端构建
npm run lint                      # eslint
cd src-tauri && cargo test --lib  # 后端单测
cd src-tauri && cargo check --all-targets
```

**改完必须跑**：`cargo check --all-targets`、`cargo test --lib`、`npx tsc --noEmit`、
`npm run build`、`npm run lint`。提交前保持全绿。

## 架构要点

**管线**（`pipeline.rs`）状态：`idle → preparing → recording → transcribing → polishing → outputting → idle`，
通过 `pipeline:state` 事件驱动前端。录音与停止不可重入（`run_lock`）。

**两种转写**

- `openai`：录音期间可选分段上传（`streaming_enabled`），松手后对整段音频重转一次保证质量
- `volcengine`：connect 时建立 WebSocket，边发 PCM 边收结果；松手发结束帧拿终稿，不再重转

**润色**：`polish_enabled=false` 时完全跳过；调用失败回退原文并 emit `pipeline:error`。
`llm_disable_thinking` 为真时请求体带 `reasoning_effort: "none"`，遇到 4xx 自动去掉重试一次。

**配置与密钥**：`AppConfig` 存在 `tauri-plugin-store` 的 `settings.json`；API Key 只进钥匙串，
命名空间 `stt`/`llm`，provider 为 `custom` 或 `volcengine`。**永远不要把密钥写进配置文件。**

## 硬性约定

1. **前后端契约不能单向漂移**：新增/修改 invoke 命令、事件名、配置字段时，三处同时改——
   `src/lib/tauri.ts`、`src-tauri/src/commands/`、`src-tauri/src/storage/mod.rs`。
2. **配置字段用 snake_case**，缺失字段要有 `#[serde(default)]`，未知字段忽略。
3. **错误要可读**：面向用户的中文原因，不要 `unwrap()`/`expect()` 处理用户输入或网络结果。
4. **跨平台**：平台相关代码必须 `#[cfg(target_os = ...)]`；不要在通用路径调用 Windows API。
   无法本地验证的平台，至少保证 `cfg` 分支语法正确。
5. **密钥不进 git**：`.env` 已在 `.gitignore`；运行时也不读 `.env`。
6. **不要引入新依赖**除非确有必要，并说明原因。

## 发版

1. 改版本号：`src-tauri/tauri.conf.json`、`package.json`、`src-tauri/Cargo.toml` 保持一致
2. 提交后执行 `pwsh -File scripts/release.ps1`
3. 脚本打 `v<版本>` 标签并推送，`.github/workflows/release.yml` 会先跑测试再三平台打包发布

## 已知限制

- macOS 未签名：首次打开需右键「打开」，钥匙串条目可能因签名变化重新授权
- Linux 无桌面会话时 Secret Service 不可用，密钥可能存不进去
- 录音与人声相关的端到端行为无法在无声卡环境自动测试
