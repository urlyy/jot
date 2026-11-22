//! Optional LLM polishing for transcription results.
//!
//! Only the OpenAI-compatible chat completions protocol is supported; the user
//! brings their own base URL, model and API key.

pub mod openai;
pub mod prompt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Path appended to `llm_base_url` unless it is already present.
pub const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
/// Path used to list available models (diagnostics only).
pub const MODELS_PATH: &str = "/models";

/// Provider identifier stored in [`LlmConfig`]; there is exactly one backend.
pub const CUSTOM_LLM_PROVIDER: &str = "custom";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub max_tokens: u32,
    pub temperature: f64,
    /// 发送 `reasoning_effort=none`，跳过推理模型的思考阶段。
    pub disable_thinking: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: CUSTOM_LLM_PROVIDER.to_string(),
            api_key: String::new(),
            model: String::new(),
            base_url: String::new(),
            max_tokens: 1024,
            temperature: 0.3,
            disable_thinking: true,
        }
    }
}

impl LlmConfig {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            provider: CUSTOM_LLM_PROVIDER.to_string(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.into(),
            ..Self::default()
        }
    }
}

/// Everything the polishing prompt needs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolishRequest {
    pub raw_text: String,
    pub dictionary: Vec<String>,
    pub polish_style: String,
    pub custom_prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolishResponse {
    pub polished_text: String,
}

/// Callback invoked for every streamed chunk of polished text.
pub type ChunkCallback = Box<dyn Fn(&str) + Send + Sync>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn polish(
        &self,
        config: &LlmConfig,
        request: &PolishRequest,
        on_chunk: Option<&ChunkCallback>,
    ) -> Result<PolishResponse, AppError>;

    fn name(&self) -> &str;
}

/// Derive the chat completions endpoint from a base URL.
pub fn chat_endpoint(base_url: &str) -> Result<String, AppError> {
    normalize_endpoint(base_url, CHAT_COMPLETIONS_PATH, "对话接口地址")
}

/// Derive the models endpoint from a base URL (diagnostics only).
pub fn models_endpoint(base_url: &str) -> Result<String, AppError> {
    normalize_endpoint(base_url, MODELS_PATH, "对话接口地址")
}

fn normalize_endpoint(base_url: &str, suffix: &str, label: &str) -> Result<String, AppError> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(AppError::Config(format!(
            "{label}不能为空，请在设置中填写，例如 https://api.openai.com/v1"
        )));
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(AppError::Config(format!(
            "{label}必须以 http:// 或 https:// 开头"
        )));
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(AppError::Config(format!("{label}不能包含空格")));
    }

    let without_trailing_slash = trimmed.trim_end_matches('/');
    if without_trailing_slash.ends_with(suffix) {
        Ok(without_trailing_slash.to_string())
    } else {
        Ok(format!("{without_trailing_slash}{suffix}"))
    }
}

/// Create the only supported provider implementation.
pub fn create_provider(client: Option<reqwest::Client>) -> Box<dyn LlmProvider> {
    match client {
        Some(client) => Box::new(openai::OpenAiProvider::with_client(client)),
        None => Box::new(openai::OpenAiProvider::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_endpoint_appends_path() {
        assert_eq!(
            chat_endpoint("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_endpoint("https://api.openai.com/v1/chat/completions/").unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn models_endpoint_appends_path() {
        assert_eq!(
            models_endpoint("http://localhost:11434/v1").unwrap(),
            "http://localhost:11434/v1/models"
        );
    }

    #[test]
    fn endpoints_report_readable_config_errors() {
        assert!(chat_endpoint("").unwrap_err().to_string().contains("不能为空"));
        assert!(chat_endpoint("openai.com/v1")
            .unwrap_err()
            .to_string()
            .contains("http://"));
    }

    #[test]
    fn create_provider_returns_openai_compatible_provider() {
        let provider = create_provider(None);
        assert_eq!(provider.name(), "OpenAI Compatible");
    }
}
