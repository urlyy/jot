# Jot

按住快捷键说话，文字直接落进光标处。

跨平台桌面语音输入工具：录音 → 转写 →（可选）润色 → 键入。不经过任何中转服务器，
转写和润色都调用你自己配置的接口，密钥存在系统钥匙串里。

- 没有账号、订阅、历史库、埋点，也不做云端中转
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

从 [Releases](https://github.com/urlyy/jot/releases) 下载：

- Windows：`.msi`
- macOS：`.dmg`（未签名，首次打开需右键「打开」）
- Linux：`.AppImage` 或 `.deb`

## 从源码构建

需要 Node 20+ 与 Rust 1.82+。

```bash
npm install
npm run tauri dev      # 开发模式
npm run tauri build    # 打包
```

Linux 额外依赖：

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
                 patchelf libasound2-dev libssl-dev libxdo-dev
```

Windows 打包若卡在下载 WiX 工具链（报 `timeout: global`），先预取一次再只打 MSI：

```powershell
pwsh -File scripts/fetch-wix-toolset.ps1
npm run tauri build -- --bundles msi
```

## 第一次配置

打开「识别」页，至少填两项：

1. **接口地址**：例如 `https://api.openai.com/v1`
2. **模型名**：例如 `whisper-1`
3. **API Key**：失焦即存进系统钥匙串；本地服务可留空

地址填到 `/v1` 即可，程序会自动拼 `/audio/transcriptions`；已经填了完整端点也不会重复拼接。
**不会自动补 `/v1`**，填 `https://api.openai.com` 会请求到错误路径。

默认开启了润色，指向本机 Ollama 的 `http://127.0.0.1:11434/v1` + `qwen3:1.7b`。
**没装 Ollama 的话先去「润色」页关掉开关**，否则每次听写都会多一次失败的请求。

## 配置要点

**润色**：调用 `/chat/completions`，以 SSE 流式读取。关闭开关后完全不调用模型。
调用失败、超时或返回空文本时输出原文，并在胶囊里提示润色失败。

**「关闭模型思考」**：qwen3 这类推理模型默认先输出几百字思考再给答案。开启后发送
`reasoning_effort: "none"`，实测 `qwen3:1.7b` 从 8 秒以上降到约 0.15 秒（RTX 5060 Ti 热态）。
服务若不认这个参数会返回 4xx，应用会去掉该字段重试一次。

**词典**：

```text
联调
联调 = 接口联合调试，不是「连调」
SeedASR：火山引擎流式识别
```

- 转写只使用术语本身；说明只在润色时交给模型
- 分隔符支持 `=` 和全角 `：`，ASCII 冒号不切分（避免拆坏 URL 和时间）
- OpenAI 兼容路径把术语拼成 `prompt` 发送；火山引擎路径走 `corpus.context.hotwords`（上限 100 token）
- 词典是提示不是强制替换，重要术语放前面

**快捷键**：支持 A–Z、0–9、F1–F12、Space、Enter、Tab、Escape 以及 `/ . , ; - = [ ]`，
可带 Ctrl / Alt / Shift / Super。

**录音上限**：默认 360 秒，到达后自动结束并开始转写。

## 使用

1. 把光标放进要输入的地方
2. 按快捷键，屏幕底部出现胶囊
3. 说话；松开或再按一次结束
4. 文字出现在光标处

托盘菜单可以开始/停止录音、打开设置、切换开机启动。

## 常见问题

**按快捷键没反应**：看「通用」页的注册状态，失败通常是被其他程序占用，换一个组合键。
Linux Wayland 下全局快捷键可能不可靠，可以先用托盘菜单开始录音。

**文字没打进去**：管理员权限窗口、部分游戏、远程桌面会拦截模拟输入，把输出方式改成
「复制到剪贴板」。macOS 需要辅助功能权限；Linux X11 需要 `xdotool`，Wayland 需要 `wtype` 或 `kwtype`。

**录不上**：检查系统麦克风权限，以及输入设备是否被静音或被其他应用独占。

**转写很慢**：主要取决于转写服务的网络与算力；开启流式听写可以边录边看到结果。

**润色比原文还差**：直接关掉润色开关，或把风格调成「极简」。

## 数据与隐私

- **API Key** 存在操作系统钥匙串，不写入配置文件
- **配置文件** 是应用数据目录下的 `settings.json`，只含接口地址、模型名、词典等
- **音频** 只在录音期间留在内存，直接发往你选择的转写服务
- **润色** 开启时，转写文本与词典会发往你配置的对话接口；关闭后不发送

## 发版

```powershell
pwsh -File scripts/release.ps1
```

校验版本号与工作区后打 `v*` 标签并推送，GitHub Actions 三平台打包并发布 Release。

## 许可

MIT
