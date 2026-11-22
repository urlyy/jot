# Jot

按住快捷键说话，文字直接落进光标处。

Jot 是桌面语音输入工具，支持 Windows、macOS 和 Linux。录音和转写文本直接发给你配置的服务，Jot 不做云端中转。

- 没有账号、订阅、历史库或埋点
- 转写支持任意 OpenAI 兼容接口，或火山引擎 SeedASR 原生流式（带术语热词）
- 润色可以一键关掉，关掉后直接输出转写原文

## 特性

| | |
|---|---|
| 全局快捷键 | 单键触发，按住说话 / 按一下开始两种模式 |
| 两种转写 | OpenAI 兼容批式接口；火山引擎原生流式 WebSocket |
| 流式听写 | 录音时实时出字；可选「边说边打字」 |
| 可选润色 | 任意 OpenAI 兼容对话接口，失败自动回退原文 |
| 术语库 | 一行一个术语，支持 `术语 = 说明`；转写偏置 + 润色提示 |
| 输出 | 键入光标处，失败回退剪贴板；可恢复原剪贴板 |
| 平台 | Windows / macOS / Linux |

## 安装

在 [Releases](https://github.com/urlyy/jot/releases) 的 Assets 中下载对应的安装包。GitHub 也会列出源码压缩包。

- Windows：`.msi`
- macOS：`.dmg`（未签名，首次打开需右键「打开」）
- Linux：`.AppImage` 或 `.deb`

## 从源码构建

使用 Node.js 22.13 或更新的 22.x 版本，以及稳定版 Rust。Windows 需要 MSVC C++ 构建工具和 WebView2，macOS 需要 Xcode 命令行工具。

```bash
npm ci
npm run tauri dev      # 开发模式
```

Ubuntu/Debian 构建前先安装系统依赖。

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
                 patchelf libasound2-dev libssl-dev libxdo-dev
```

运行 `npm run tauri build`。Windows 生成 MSI，macOS 生成 DMG，Linux 生成 DEB 和 AppImage。

默认产物目录是 `src-tauri/target/release/bundle/`。如果仓库移动过，且 Cargo 报错引用旧路径，可将 `CARGO_TARGET_DIR` 指向新的绝对路径再构建。

Windows 下载 WiX 超时（`timeout: global`）时，可先运行预取脚本再打包。

```powershell
pwsh -File scripts/fetch-wix-toolset.ps1
npm run tauri build -- --bundles msi
```

## 第一次配置

使用 OpenAI 兼容服务时，在「识别」页填写接口地址和模型名。

1. 接口地址：例如 `https://api.openai.com/v1`
2. 模型名：例如 `whisper-1`
3. 服务需要 API Key 时，填入后离开输入框，Jot 会把密钥存进系统钥匙串。本地服务允许匿名调用时可留空。

地址填到 `/v1` 即可，程序会自动拼 `/audio/transcriptions`；已经填了完整端点也不会重复拼接。
Jot 不会自动补 `/v1`，使用 OpenAI 官方服务时需填到 `https://api.openai.com/v1`。

默认开启了润色，指向本机 Ollama 的 `http://127.0.0.1:11434/v1` + `qwen3:1.7b`。
如果本机没有启动 Ollama，可以关闭润色，或填入可用的对话接口。

## 配置要点

润色：调用 `/chat/completions`，以 SSE 流式读取。关闭开关后完全不调用模型。
调用失败、超时或返回空文本时输出原文，并在胶囊里提示润色失败。

「关闭模型思考」：qwen3 这类推理模型默认先输出几百字思考再给答案。开启后发送
`reasoning_effort: "none"`，实测 `qwen3:1.7b` 从 8 秒以上降到约 0.15 秒（RTX 5060 Ti 热态）。
服务若不认这个参数会返回 4xx，应用会去掉该字段重试一次。

词典：

```text
联调
联调 = 接口联合调试，不是「连调」
SeedASR：火山引擎流式识别
```

- 转写只使用术语本身；说明只在润色时交给模型
- 分隔符支持 `=` 和全角 `：`，ASCII 冒号不切分（避免拆坏 URL 和时间）
- OpenAI 兼容路径把术语拼成 `prompt` 发送；火山引擎路径走 `corpus.context.hotwords`（上限 100 token）
- 词典是提示不是强制替换，重要术语放前面

快捷键：支持 A–Z、0–9、F1–F12、Space、Enter、Tab、Escape 以及 `/ . , ; - = [ ]`，
可带 Ctrl / Alt / Shift / Super。

录音上限：默认 360 秒，到达后自动结束并开始转写。

## 使用

1. 把光标放进要输入的地方
2. 按快捷键，屏幕底部出现胶囊
3. 说话；松开或再按一次结束
4. 文字出现在光标处

托盘菜单可以开始/停止录音、打开设置、切换开机启动。

## 常见问题

按快捷键没反应：看「通用」页的注册状态，失败通常是被其他程序占用，换一个组合键。
Linux Wayland 下全局快捷键可能不可靠，可以先用托盘菜单开始录音。

文字没打进去：管理员权限窗口、部分游戏、远程桌面会拦截模拟输入，把输出方式改成
「复制到剪贴板」。macOS 需要辅助功能权限；Linux X11 需要 `xdotool`，Wayland 需要 `wtype` 或 `kwtype`。

录不上：检查系统麦克风权限、输入设备的静音状态，以及是否有其他应用独占麦克风。

转写很慢：主要取决于转写服务的网络与算力；开启流式听写可以边录边看到结果。

润色比原文还差：直接关掉润色开关，或把风格调成「极简」。

## 数据与隐私

- API Key 存在操作系统钥匙串，不写入配置文件
- 配置文件 是应用数据目录下的 `settings.json`，只含接口地址、模型名、词典等
- 录音和转写期间，音频保存在内存中，并直接发给你选择的转写服务
- 润色 开启时，转写文本与词典会发往你配置的对话接口；关闭后不发送

## 本地转写

想先试离线听写，可以启动 [whisper-server](https://github.com/ggml-org/whisper.cpp/tree/master/examples/server)，用多语言 Whisper 模型，并传入 `--host 127.0.0.1 --inference-path /v1/audio/transcriptions`。Jot 的「识别」页选 OpenAI 兼容接口，地址填 `http://127.0.0.1:8080/v1`，模型名填已加载模型的名称，API Key 留空。关闭流式听写后，松开快捷键才提交整段录音。

如果需要像 SeedASR 一样边说边出字，可以评估 [sherpa-onnx 的中英流式 Zipformer](https://github.com/k2-fsa/sherpa-onnx/blob/master/rust-api-examples/README.md)。它需要接入 Jot 的转写后端。想评估中文及粤语的整段识别，可以用同一运行时试 SenseVoiceSmall。

## 发版

```powershell
pwsh -File scripts/release.ps1
```

提交前同步 `package.json`、`package-lock.json`、`src-tauri/tauri.conf.json` 和 Cargo 包版本。脚本推送 `v<版本>` 标签后，GitHub Actions 从该标签构建。三平台的安装包齐全后，发布作业会把它们上传到同一个 Release。
