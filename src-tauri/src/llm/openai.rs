//! OpenAI-compatible chat completions client with SSE streaming.

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::json;

use crate::error::AppError;

use super::{
    chat_endpoint, prompt, ChunkCallback, LlmConfig, LlmProvider, PolishRequest, PolishResponse,
};

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const ERROR_BODY_MAX_CHARS: usize = 200;

pub struct OpenAiProvider {
    client: reqwest::Client,
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    fn build_body(config: &LlmConfig, request: &PolishRequest) -> serde_json::Value {
        let system_prompt = prompt::build_system_prompt(prompt::PolishPromptOptions {
            dictionary: &request.dictionary,
            polish_style: &request.polish_style,
            custom_prompt: &request.custom_prompt,
        });

        let mut body = json!({
            "model": config.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": prompt::build_user_message(&request.raw_text) }
            ],
            "stream": true,
            "temperature": config.temperature,
        });

        // 推理模型（qwen3 等）默认会先“思考”再输出，单次润色要好几秒。
        // reasoning_effort=none 让它们直接给结果；不支持该参数的云端服务会返回 4xx，
        // 届时 polish() 会去掉这个字段重试一次。
        if config.disable_thinking {
            body["reasoning_effort"] = json!("none");
        }

        body
    }

    async fn send_request(
        &self,
        config: &LlmConfig,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, AppError> {
        let endpoint = chat_endpoint(&config.base_url)?;
        let mut request = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .timeout(REQUEST_TIMEOUT);

        let api_key = config.api_key.trim();
        if !api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {api_key}"));
        }

        let response = request.json(body).send().await?;
        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let summary = truncate_chars(body.trim(), ERROR_BODY_MAX_CHARS);
        Err(AppError::Api {
            status: status.as_u16(),
            body: summary,
        })
    }

    /// Parse one SSE `data:` line, returning `(content, finished)`.
    fn parse_sse_data(data: &str) -> Result<(Option<String>, bool), AppError> {
        if data == "[DONE]" {
            return Ok((None, true));
        }
        let value: serde_json::Value = serde_json::from_str(data)
            .map_err(|error| AppError::Config(format!("无法解析润色响应片段：{error}")))?;

        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
        {
            return Err(AppError::Config(format!(
                "润色接口返回错误：{}",
                truncate_chars(message, ERROR_BODY_MAX_CHARS)
            )));
        }

        let content = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(serde_json::Value::as_str)
            .filter(|content| !content.is_empty())
            .map(str::to_string);

        Ok((content, false))
    }

    /// 部分推理模型（例如 qwen3 的旧构建）不认 `reasoning_effort=none`，
    /// 会把整段思考过程当正文吐出来，末尾再跟一个 `</think>`。
    /// 这里把 `</think>` 之前的内容整段丢弃，只保留真正的结果。
    fn answer_part(text: &str) -> &str {
        match text.rfind("</think>") {
            Some(index) => &text[index + "</think>".len()..],
            None => text,
        }
    }

    async fn consume_stream(
        &self,
        response: reqwest::Response,
        on_chunk: &ChunkCallback,
    ) -> Result<String, AppError> {
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut full_text = String::new();
        let mut emitted_chars = 0usize;
        let mut streaming_unlocked = false;

        // 输出很短，等到能判断有没有思考段再往前端推流，避免预览里出现推理废话。
        const UNLOCK_AFTER_CHARS: usize = 160;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let (content, finished) = Self::parse_sse_data(data.trim())?;
                if let Some(content) = content {
                    full_text.push_str(&content);

                    if !streaming_unlocked
                        && (full_text.contains("</think>")
                            || full_text.chars().count() > UNLOCK_AFTER_CHARS)
                    {
                        streaming_unlocked = true;
                    }
                    if streaming_unlocked {
                        let visible: Vec<char> = Self::answer_part(&full_text).chars().collect();
                        if visible.len() > emitted_chars {
                            let delta: String = visible[emitted_chars..].iter().collect();
                            on_chunk(&delta);
                            emitted_chars = visible.len();
                        }
                    }
                }
                if finished {
                    return Ok(Self::flush_tail(&full_text, &mut emitted_chars, on_chunk));
                }
            }
        }

        Ok(Self::flush_tail(&full_text, &mut emitted_chars, on_chunk))
    }

    fn flush_tail(full_text: &str, emitted_chars: &mut usize, on_chunk: &ChunkCallback) -> String {
        let visible: Vec<char> = Self::answer_part(full_text).chars().collect();
        if visible.len() > *emitted_chars {
            let delta: String = visible[*emitted_chars..].iter().collect();
            on_chunk(&delta);
            *emitted_chars = visible.len();
        }
        visible.into_iter().collect::<String>().trim().to_string()
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn polish(
        &self,
        config: &LlmConfig,
        request: &PolishRequest,
        on_chunk: Option<&ChunkCallback>,
    ) -> Result<PolishResponse, AppError> {
        if config.model.trim().is_empty() {
            return Err(AppError::Config("润色模型不能为空".to_string()));
        }

        let mut body = Self::build_body(config, request);
        let response = match self.send_request(config, &body).await {
            Ok(response) => response,
            Err(AppError::Api { status, .. })
                if body.get("reasoning_effort").is_some()
                    && (400..500).contains(&status) =>
            {
                // 该服务不认识 reasoning_effort：去掉它再试一次，避免因为一个
                // 可选优化让整次润色失败。
                body.as_object_mut()
                    .map(|object| object.remove("reasoning_effort"));
                self.send_request(config, &body).await?
            }
            Err(error) => return Err(error),
        };

        let polished_text = match on_chunk {
            Some(callback) => self.consume_stream(response, callback).await?,
            None => {
                // Defensive fallback: the contract always streams, but a
                // caller without a callback can still drain the SSE stream.
                let sink: ChunkCallback = Box::new(|_| {});
                self.consume_stream(response, &sink).await?
            }
        };

        if polished_text.trim().is_empty() {
            return Err(AppError::Config("润色接口没有返回任何文本".to_string()));
        }

        Ok(PolishResponse { polished_text })
    }

    fn name(&self) -> &str {
        "OpenAI Compatible"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_content_chunks_are_extracted() {
        let line = r#"{"choices":[{"delta":{"content":"你好"}}]}"#;
        let (content, finished) = OpenAiProvider::parse_sse_data(line).unwrap();
        assert_eq!(content.as_deref(), Some("你好"));
        assert!(!finished);
    }

    #[test]
    fn done_marker_finishes_the_stream() {
        let (content, finished) = OpenAiProvider::parse_sse_data("[DONE]").unwrap();
        assert_eq!(content, None);
        assert!(finished);
    }

    #[test]
    fn empty_deltas_are_ignored() {
        let line = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        let (content, finished) = OpenAiProvider::parse_sse_data(line).unwrap();
        assert_eq!(content, None);
        assert!(!finished);
    }

    #[test]
    fn error_payloads_surface_readable_messages() {
        let line = r#"{"error":{"message":"invalid api key"}}"#;
        let error = OpenAiProvider::parse_sse_data(line).unwrap_err();
        assert!(error.to_string().contains("invalid api key"));
    }

    #[test]
    fn body_matches_the_interface_contract() {
        let config = LlmConfig::new("https://api.openai.com/v1", "gpt-4o-mini", "sk-test");
        let request = PolishRequest {
            raw_text: "你好".to_string(),
            dictionary: vec!["Jot".to_string()],
            polish_style: "clean".to_string(),
            custom_prompt: String::new(),
        };

        let body = OpenAiProvider::build_body(&config, &request);

        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["stream"], true);
        assert_eq!(body["temperature"], 0.3);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Jot"));
    }
}
