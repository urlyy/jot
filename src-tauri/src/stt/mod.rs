//! Speech-to-text abstraction.
//!
//! Two providers coexist behind [`SttProvider`]:
//!
//! * [`whisper_compat::WhisperCompatProvider`] — a user-supplied
//!   OpenAI-compatible batch transcription endpoint (the default).
//! * [`volcengine::VolcengineProvider`] — Volcengine (Doubao / SeedASR) native
//!   WebSocket streaming ASR, with concurrent audio upload and transcript
//!   download.

pub mod config;
pub mod volcengine;
pub mod whisper_compat;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

use whisper_compat::{WhisperCompatConfig, WhisperCompatProvider};

/// Sample rate used for the WAV files we upload (PCM 16 kHz mono).
pub const SAMPLE_RATE: u32 = 16_000;

/// Configuration handed to an STT provider for one dictation session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SttConfig {
    /// API key (may be empty for keyless local servers).
    pub api_key: String,
    /// `None` means "auto" — the `language` form field is not sent.
    pub language: Option<String>,
    pub sample_rate: u32,
    /// Volcengine only: `X-Api-Resource-Id` value. `None` uses SeedASR.
    pub resource_id: Option<String>,
    /// Volcengine only: 术语表，作为 `request.corpus.context` 的热词直传。
    pub hotwords: Vec<String>,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            language: None,
            sample_rate: SAMPLE_RATE,
            resource_id: None,
            hotwords: Vec::new(),
        }
    }
}

impl SttConfig {
    /// Build a config from user settings, mapping `"auto"`/empty to `None`.
    pub fn from_language(api_key: String, language: &str) -> Self {
        let language = language.trim();
        let language = if language.is_empty() || language.eq_ignore_ascii_case("auto") {
            None
        } else {
            Some(language.to_string())
        };
        Self {
            api_key,
            language,
            sample_rate: SAMPLE_RATE,
            resource_id: None,
            hotwords: Vec::new(),
        }
    }

    /// Attach a Volcengine resource id (`volc.seedasr.sauc.duration` etc.).
    pub fn with_resource_id(mut self, resource_id: &str) -> Self {
        let resource_id = resource_id.trim();
        self.resource_id = if resource_id.is_empty() {
            None
        } else {
            Some(resource_id.to_string())
        };
        self
    }

    /// Attach the user's terminology list as Volcengine hotwords.
    ///
    /// 火山引擎的 `corpus.context` 是「提示」而不是强约束，且双向流式只有
    /// 100 token 的额度（从前往后保留）。所以这里做两件事：去重、按顺序截断。
    pub fn with_hotwords(mut self, hotwords: Vec<&str>) -> Self {
        const MAX_HOTWORDS: usize = 40;
        const MAX_TOTAL_CHARS: usize = 180;

        let mut total = 0usize;
        let mut kept: Vec<String> = Vec::new();
        for word in hotwords {
            let word = word.trim();
            if word.is_empty() || kept.iter().any(|existing| existing == word) {
                continue;
            }
            // 中文约 1 字 1 token，留出余量；超额的词直接放弃，避免被服务端截断。
            let cost = word.chars().count() + 2;
            if kept.len() >= MAX_HOTWORDS || total + cost > MAX_TOTAL_CHARS {
                break;
            }
            total += cost;
            kept.push(word.to_string());
        }

        self.hotwords = kept;
        self
    }
}

/// Events a provider can yield. Batch providers only ever produce [`Final`]
/// (from `disconnect`); streaming providers also produce [`Partial`].
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEvent {
    Final {
        text: String,
        confidence: f32,
    },
    Partial {
        text: String,
    },
    Error {
        message: String,
    },
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError>;
    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError>;
    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError>;
    /// Finish the session and return the final transcript for batch providers.
    async fn disconnect(&mut self) -> Result<Option<String>, AppError>;
    fn name(&self) -> &str;
}

/// Create the only supported batch provider: the custom OpenAI-compatible one.
pub fn create_provider(
    provider_name: &str,
    provider_config: WhisperCompatConfig,
    client: Option<reqwest::Client>,
) -> Result<Box<dyn SttProvider>, AppError> {
    match provider_name {
        config::CUSTOM_WHISPER_PROVIDER => Ok(match client {
            Some(client) => Box::new(WhisperCompatProvider::with_client(
                provider_config,
                client,
            )),
            None => Box::new(WhisperCompatProvider::new(provider_config)),
        }),
        other => Err(AppError::Config(format!(
            "不支持的转写供应商：{other}（仅支持自定义 OpenAI 兼容接口）"
        ))),
    }
}

/// Create the native Volcengine streaming provider.
pub fn create_volcengine_provider() -> Box<dyn SttProvider> {
    Box::new(volcengine::VolcengineProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_config() -> WhisperCompatConfig {
        config::build_custom_whisper_config("http://localhost:8000/v1", "whisper-1", &[])
            .expect("test config must be valid")
    }

    #[test]
    fn custom_provider_is_created() {
        let provider =
            create_provider(config::CUSTOM_WHISPER_PROVIDER, provider_config(), None).unwrap();
        assert_eq!(provider.name(), config::CUSTOM_WHISPER_PROVIDER);
    }

    #[test]
    fn unknown_provider_is_rejected_with_a_readable_message() {
        let error = create_provider("deepgram", provider_config(), None)
            .err()
            .expect("unknown provider must fail");
        assert!(error.to_string().contains("deepgram"));
    }

    #[test]
    fn auto_language_is_not_forwarded() {
        assert_eq!(SttConfig::from_language("key".to_string(), "auto").language, None);
        assert_eq!(SttConfig::from_language("key".to_string(), "").language, None);
        assert_eq!(
            SttConfig::from_language("key".to_string(), "zh").language.as_deref(),
            Some("zh")
        );
    }

    #[test]
    fn resource_id_is_optional_and_trimmed() {
        assert_eq!(SttConfig::default().resource_id, None);
        assert_eq!(
            SttConfig::from_language("key".to_string(), "auto")
                .with_resource_id("  volc.seedasr.sauc.duration ")
                .resource_id
                .as_deref(),
            Some("volc.seedasr.sauc.duration")
        );
        assert_eq!(
            SttConfig::default().with_resource_id("   ").resource_id,
            None
        );
    }

    #[test]
    fn volcengine_provider_factory_builds_the_streaming_provider() {
        let provider = create_volcengine_provider();
        assert_eq!(provider.name(), crate::storage::STT_PROVIDER_VOLCENGINE);
    }
}
