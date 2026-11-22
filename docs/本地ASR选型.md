# 本地 ASR 选型（术语库场景）

本文记录在「本机跑、且术语库要真正生效」这个前提下，对当前开源中文语音识别方案的取舍。

结论先行：**Whisper 系做不到可靠的术语偏置；要术语生效，只能走 FunASR 系（Paraformer / Fun-ASR-Nano）的上下文热词，或 sherpa-onnx 的 hotwords。**

## 为什么 Whisper 的「提示词」不够

Jot 现在接入 OpenAI 兼容接口时，会把词典拼成 `prompt` 一起发给转写服务。这是**软偏置**：

- 它只是给模型一段上文，模型完全可以选择无视；
- 上下文窗口约 224 token，几十个术语就会被截断；
- 中文场景下 Whisper 本身的字错率就高于国产模型。

faster-whisper 1.0+ 额外提供了 `hotwords` 参数，效果比 `prompt` 稍好，但本质仍是提示而非解码约束。术语越多、越长，命中率越差。

## 候选对比

| 模型 | 参数量 | 中文 CER | 流式 | 术语/热词支持 | 部署方式 | 许可 |
|---|---|---|---|---|---|---|
| Fun-ASR-Nano（2025/12） | 800M | ~4.5% | 支持 | **FunASR 原生热词定制** | FunASR 服务 / Docker | Apache 2.0 |
| SeACo-Paraformer | 220M | ~1.7–2.0% | 支持 | **上下文热词（为此设计）** | FunASR / sherpa-onnx | MIT + 模型协议 |
| FireRedASR v2（2026/02） | 1.1B / 8.3B | **2.89%（最优）** | 不支持 | 无 | 需 GPU | Apache 2.0 |
| Qwen3-ASR（2026/01） | 0.6B / 1.7B | 3.76% | 支持 | 云端版支持 context 文本，开源权重需实测 | vLLM | Apache 2.0 |
| SenseVoice-Small | 234M | ~3.0% | 不支持 | 弱 | sherpa-onnx / FunASR | Apache 2.0 |
| Whisper large-v3 | 1550M | ~5.14% | 支持 | 软偏置（prompt / hotwords） | speaches / whisper.cpp | MIT |

数据来源：FireRedASR2S 仓库的统一基准对比表，以及各模型官方仓库；不同来源的口径不完全一致，仅用于量级判断。

几个要点：

- **精度最高的是 FireRedASR v2**，但它没有热词能力，也不支持流式，还要 GPU——和「术语库」这个需求冲突。
- **Qwen3-ASR** 是 2026 年的新选择，流式、方言覆盖广（22 种）、体积小，但开源权重是否接受上下文热词需要实测确认。
- **Paraformer / Fun-ASR-Nano** 精度不是最高，但热词是解码期约束，术语命中率最高，且都能在 CPU 上跑。

## 三条落地路径

### A. speaches + faster-whisper（零改动）

Jot 现有的 OpenAI 兼容路径直接可用：本地起 `speaches`，填 `http://localhost:8000/v1`，术语走 `prompt`。

- 优点：今天就能用，不写代码
- 缺点：中文精度一般，术语只是软偏置

### B. 新增 `local-funasr` 供应商（推荐）

本地起 FunASR 服务，Jot 增加一个供应商选项，把词典作为热词表发送。

- 优点：术语真正生效；中文精度优于 Whisper
- 代价：后端约 200–300 行；需要用户另起一个服务

### C. 内嵌 sherpa-onnx（全离线）

用 `sherpa-rs` 把模型打包进应用，装完即用，不需要 external server。

- 优点：完全离线、零配置
- 代价：安装包增加 200MB 以上；热词支持只对流式 transducer 模型完善，中文 Paraformer/SenseVoice 走这条路热词能力受限

## 当前建议

先用 A 验证链路，再决定是否投入 B。

如果术语命中是硬需求（人名、产品名、专业缩写经常被写错），直接做 B——这是唯一能让术语真正参与解码的路线。

## 参考

- [中文语音识别 6 个开源模型对比](https://cloud.tencent.cn/developer/article/2642961)
- [FireRedASR 论文](https://arxiv.org/html/2501.14350)
- [faster-whisper hotwords 参数](https://theneuralbase.com/faster-whisper/learn/intermediate/hotwords-parameter-v1-0/)
- [sherpa-onnx hotwords 文档](https://github.com/k2-fsa/sherpa/blob/f4cacb3710979d07c6c00f70df0d15fbfbad899d/docs/source/onnx/hotwords/index.rst)
- [speaches 的 OpenAI 兼容转写接口](https://deepwiki.com/speaches-ai/speaches/3.1-speech-to-text-api)
