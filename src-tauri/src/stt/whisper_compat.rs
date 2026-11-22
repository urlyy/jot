use async_trait::async_trait;

use crate::error::AppError;

use super::{SttConfig, SttProvider, TranscriptEvent};

/// Configuration for the OpenAI-compatible HTTP file-upload STT provider.
#[derive(Debug, Clone)]
pub struct WhisperCompatConfig {
    pub provider_name: String,
    pub endpoint: String,
    pub model: String,
    /// Dictionary bias sent as Whisper's `prompt` form field.
    pub dictionary_prompt: Option<String>,
    /// Local OpenAI-compatible servers often do not require authentication.
    pub api_key_required: bool,
}

/// Max audio buffer: ~24 MB PCM ≈ 12.5 min at 16kHz 16-bit mono.
/// Keeps the resulting WAV under 25 MB (OpenAI/Groq limit).
const MAX_AUDIO_BYTES: usize = 24 * 1024 * 1024;
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
// Three HTTP attempts, two backoffs (1s + 2s), and a small finalization margin.
pub const FINALIZE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(198);
/// Max characters of context forwarded as the `prompt` for streaming segments.
const CONTEXT_PROMPT_MAX_CHARS: usize = 200;

fn upload_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::Timeout(REQUEST_TIMEOUT)
    } else {
        error.into()
    }
}

fn truncate_body(body: &str) -> String {
    body.char_indices()
        .take_while(|&(index, _)| index < 200)
        .last()
        .map(|(index, character)| &body[..index + character.len_utf8()])
        .unwrap_or(body)
        .to_string()
}

/// Generic provider for any OpenAI Whisper-compatible transcription API.
pub struct WhisperCompatProvider {
    provider_config: WhisperCompatConfig,
    stt_config: Option<SttConfig>,
    audio_buffer: Vec<u8>,
    client: reqwest::Client,
}

impl WhisperCompatProvider {
    pub fn new(provider_config: WhisperCompatConfig) -> Self {
        Self::with_client(provider_config, reqwest::Client::new())
    }

    pub fn with_client(provider_config: WhisperCompatConfig, client: reqwest::Client) -> Self {
        Self {
            provider_config,
            stt_config: None,
            audio_buffer: Vec::new(),
            client,
        }
    }

    /// Build a WAV file from raw PCM 16-bit mono audio.
    pub fn build_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
        let data_len = pcm.len() as u32;
        let channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
        let block_align = channels * bits_per_sample / 8;
        let file_size = 36 + data_len;

        let mut wav = Vec::with_capacity(44 + pcm.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(pcm);
        wav
    }

    /// Compose the `prompt` form field: the dictionary bias plus optional
    /// per-request context (used by live dictation to keep terminology stable).
    fn request_prompt(&self, context: Option<&str>) -> Option<String> {
        let dictionary = self
            .provider_config
            .dictionary_prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let context = context
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .chars()
                    .rev()
                    .take(CONTEXT_PROMPT_MAX_CHARS)
                    .collect::<Vec<char>>()
                    .into_iter()
                    .rev()
                    .collect::<String>()
            });

        match (dictionary, context) {
            (None, None) => None,
            (Some(dictionary), None) => Some(dictionary.to_string()),
            (None, Some(context)) => Some(context),
            (Some(dictionary), Some(context)) => Some(format!("{dictionary} {context}")),
        }
    }

    /// Single-shot transcription of already-rendered WAV bytes.
    ///
    /// Shared by the batch path (`disconnect`) and live dictation segments so
    /// the multipart/retry logic exists exactly once.
    pub async fn transcribe_wav(
        &self,
        wav_data: Vec<u8>,
        config: &SttConfig,
        context_prompt: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        if self.provider_config.api_key_required && config.api_key.trim().is_empty() {
            return Err(AppError::Auth(format!(
                "{} API key is empty",
                self.provider_config.provider_name
            )));
        }

        let prompt = self.request_prompt(context_prompt);

        let mut attempt = 0u32;
        loop {
            let file_part = reqwest::multipart::Part::bytes(wav_data.clone())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|error| AppError::Config(format!("构建音频上传表单失败：{error}")))?;

            let mut form = reqwest::multipart::Form::new()
                .text("model", self.provider_config.model.clone())
                .part("file", file_part);

            if let Some(language) = config
                .language
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("auto"))
            {
                form = form.text("language", language.to_string());
            }

            if let Some(prompt) = prompt.as_ref() {
                form = form.text("prompt", prompt.clone());
            }

            let mut request = self
                .client
                .post(&self.provider_config.endpoint)
                .multipart(form)
                .timeout(REQUEST_TIMEOUT);

            if !config.api_key.trim().is_empty() {
                request = request.header("Authorization", format!("Bearer {}", config.api_key));
            }

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.map_err(upload_error)?;

                    if status.is_success() {
                        let value: serde_json::Value = serde_json::from_str(&body)
                            .map_err(|error| AppError::Config(format!("无法解析转写响应：{error}")))?;
                        let text = value["text"].as_str().unwrap_or("").trim().to_string();
                        return Ok(if text.is_empty() { None } else { Some(text) });
                    }

                    if status.as_u16() >= 500 && attempt < 2 {
                        attempt += 1;
                        tracing::warn!(
                            "{} server error {} (attempt {}/3): {}",
                            self.provider_config.provider_name,
                            status,
                            attempt,
                            truncate_body(&body)
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            1000 * 2u64.pow(attempt - 1),
                        ))
                        .await;
                        continue;
                    }

                    let summary = truncate_body(&body);
                    tracing::error!(
                        "{} HTTP {}: {}",
                        self.provider_config.provider_name,
                        status,
                        summary
                    );
                    return Err(AppError::Api {
                        status: status.as_u16(),
                        body: summary,
                    });
                }
                Err(error) if error.is_timeout() && attempt < 2 => {
                    attempt += 1;
                    tracing::warn!(
                        "{} timeout (attempt {}/3)",
                        self.provider_config.provider_name,
                        attempt
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(
                        1000 * 2u64.pow(attempt - 1),
                    ))
                    .await;
                    continue;
                }
                Err(error) => return Err(upload_error(error)),
            }
        }
    }

    /// Render raw PCM to WAV and transcribe it in one shot.
    pub async fn transcribe_pcm(
        &self,
        pcm: &[u8],
        config: &SttConfig,
        context_prompt: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let wav = Self::build_wav(pcm, config.sample_rate);
        self.transcribe_wav(wav, config, context_prompt).await
    }
}

#[async_trait]
impl SttProvider for WhisperCompatProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError> {
        if self.provider_config.api_key_required && config.api_key.trim().is_empty() {
            return Err(AppError::Auth(format!(
                "{} API key is empty",
                self.provider_config.provider_name
            )));
        }
        self.stt_config = Some(config.clone());
        self.audio_buffer.clear();
        tracing::info!(
            "{} provider ready (buffering mode)",
            self.provider_config.provider_name
        );
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError> {
        if self.audio_buffer.len() + chunk.len() > MAX_AUDIO_BYTES {
            return Err(AppError::Config(format!(
                "{}: audio exceeds maximum length (~12 min)",
                self.provider_config.provider_name
            )));
        }
        self.audio_buffer.extend_from_slice(chunk);
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError> {
        // File-based providers transcribe in disconnect(); keep this future
        // pending so callers do not busy-spin while recording.
        std::future::pending().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>, AppError> {
        let config = match &self.stt_config {
            Some(config) => config.clone(),
            None => return Ok(None),
        };

        if self.audio_buffer.is_empty() {
            tracing::info!(
                "{}: no audio buffered, skipping",
                self.provider_config.provider_name
            );
            return Ok(None);
        }

        let audio_len_secs = self.audio_buffer.len() as f64 / (config.sample_rate as f64 * 2.0);
        let wav_data = Self::build_wav(&self.audio_buffer, config.sample_rate);
        self.audio_buffer.clear();
        tracing::info!(
            "{}: sending {:.1}s of audio for transcription",
            self.provider_config.provider_name,
            audio_len_secs
        );

        self.transcribe_wav(wav_data, &config, None).await
    }

    fn name(&self) -> &str {
        &self.provider_config.provider_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WhisperCompatConfig {
        WhisperCompatConfig {
            provider_name: "custom".to_string(),
            endpoint: "http://localhost:8000/v1/audio/transcriptions".to_string(),
            model: "whisper-1".to_string(),
            dictionary_prompt: Some("Jot".to_string()),
            api_key_required: false,
        }
    }

    fn test_provider(dictionary_prompt: Option<&str>) -> WhisperCompatProvider {
        WhisperCompatProvider::new(WhisperCompatConfig {
            dictionary_prompt: dictionary_prompt.map(str::to_string),
            ..test_config()
        })
    }

    #[tokio::test]
    async fn incomplete_response_body_is_reported_as_a_transport_error() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 128\r\nConnection: close\r\n\r\n{\"text\":\"")
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        let mut provider = WhisperCompatProvider::new(WhisperCompatConfig {
            endpoint: format!("http://{address}/transcriptions"),
            ..test_config()
        });
        provider.connect(&SttConfig::default()).await.unwrap();
        provider.send_audio(&[0, 0]).await.unwrap();
        let error = provider.disconnect().await.unwrap_err();
        assert!(matches!(error, AppError::Network(_)), "{error:?}");
        server.join().unwrap();
    }

    #[tokio::test]
    async fn connect_allows_empty_api_key_when_not_required() {
        let mut provider = test_provider(None);

        let result = provider.connect(&SttConfig::default()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn recv_transcript_waits_for_file_based_provider() {
        let mut provider = test_provider(None);

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(20),
            provider.recv_transcript(),
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn request_prompt_merges_dictionary_and_context() {
        let provider = test_provider(Some("Jot"));
        assert_eq!(
            provider.request_prompt(Some("今天讨论接口")).as_deref(),
            Some("Jot 今天讨论接口")
        );
        assert_eq!(
            provider.request_prompt(None).as_deref(),
            Some("Jot")
        );

        let provider = test_provider(None);
        assert_eq!(provider.request_prompt(None), None);
        assert_eq!(
            provider.request_prompt(Some("  ")).as_deref(),
            None
        );
    }

    #[test]
    fn request_prompt_keeps_the_tail_of_long_context() {
        let provider = test_provider(None);
        let context = "a".repeat(500);
        let prompt = provider.request_prompt(Some(&context)).unwrap();
        assert_eq!(prompt.chars().count(), CONTEXT_PROMPT_MAX_CHARS);
    }

    #[test]
    fn wav_header_describes_pcm_16k_mono() {
        let wav = WhisperCompatProvider::build_wav(&[0, 0, 1, 0], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + 4);
        assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 16_000);
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1);
    }
}
