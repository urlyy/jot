//! Endpoint derivation and request-shaping helpers for the custom
//! OpenAI-compatible transcription provider.

use super::whisper_compat::WhisperCompatConfig;

/// Identifier used for the only supported provider (matches the credential
/// namespace/provider pair `stt` + `custom` from the contract).
pub const CUSTOM_WHISPER_PROVIDER: &str = "custom";

pub const TRANSCRIPTIONS_PATH: &str = "/audio/transcriptions";

/// Dictionary words are joined with the ideographic comma before being sent as
/// Whisper's `prompt` form field.
pub const DICTIONARY_SEPARATOR: &str = "、";
/// Hard limit from the interface contract for the dictionary hint.
pub const DICTIONARY_PROMPT_MAX_CHARS: usize = 200;

/// Normalize a user-supplied base URL into the full transcription endpoint.
///
/// Rules from the contract: strip the trailing `/`; if the path already ends
/// with `/audio/transcriptions` keep it, otherwise append it.
pub fn normalize_transcription_endpoint(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err("转写接口地址不能为空，请在设置中填写，例如 https://api.openai.com/v1".to_string());
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("转写接口地址必须以 http:// 或 https:// 开头".to_string());
    }
    if trimmed.contains(char::is_whitespace) {
        return Err("转写接口地址不能包含空格".to_string());
    }

    let without_trailing_slash = trimmed.trim_end_matches('/');
    if without_trailing_slash.ends_with(TRANSCRIPTIONS_PATH) {
        Ok(without_trailing_slash.to_string())
    } else {
        Ok(format!("{without_trailing_slash}{TRANSCRIPTIONS_PATH}"))
    }
}

/// Join dictionary words into a single `prompt` value, capped at 200 chars.
///
/// 只取术语本身：`术语 = 说明` 里的说明是写给润色模型看的，对声学模型没有意义，
/// 还会白白占用这 200 字符的额度。
pub fn dictionary_prompt(dictionary: &[String]) -> Option<String> {
    let words = crate::dictionary::terms(dictionary);
    if words.is_empty() {
        return None;
    }

    let joined = words.join(DICTIONARY_SEPARATOR);
    let truncated = truncate_chars(&joined, DICTIONARY_PROMPT_MAX_CHARS);
    if truncated.trim().is_empty() {
        None
    } else {
        Some(truncated)
    }
}

/// Keep the last `max_chars` characters — used for the streaming context hint.
pub fn tail_chars(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().skip(count - max_chars).collect()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Build the provider configuration from user settings.
///
/// `dictionary` is forwarded as the `prompt` form field (joined with `、`,
/// truncated to 200 chars) so the model is biased towards the user's terms.
pub fn build_custom_whisper_config(
    base_url: &str,
    model: &str,
    dictionary: &[String],
) -> Result<WhisperCompatConfig, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("转写模型不能为空，请在设置中填写，例如 whisper-1".to_string());
    }

    Ok(WhisperCompatConfig {
        provider_name: CUSTOM_WHISPER_PROVIDER.to_string(),
        endpoint: normalize_transcription_endpoint(base_url)?,
        model: model.to_string(),
        dictionary_prompt: dictionary_prompt(dictionary),
        api_key_required: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_appends_transcriptions_path() {
        assert_eq!(
            normalize_transcription_endpoint("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            normalize_transcription_endpoint("https://api.openai.com/v1/").unwrap(),
            "https://api.openai.com/v1/audio/transcriptions"
        );
    }

    #[test]
    fn endpoint_keeps_full_transcriptions_path() {
        assert_eq!(
            normalize_transcription_endpoint("http://localhost:8000/v1/audio/transcriptions/")
                .unwrap(),
            "http://localhost:8000/v1/audio/transcriptions"
        );
    }

    #[test]
    fn endpoint_rejects_empty_and_non_http_urls() {
        assert!(normalize_transcription_endpoint("   ")
            .unwrap_err()
            .contains("不能为空"));
        assert!(normalize_transcription_endpoint("file:///tmp/server")
            .unwrap_err()
            .contains("http://"));
        assert!(normalize_transcription_endpoint("https://example.com/a b")
            .unwrap_err()
            .contains("空格"));
    }

    #[test]
    fn dictionary_prompt_joins_words_and_is_capped() {
        let dictionary = vec![
            "Jot".to_string(),
            "  Whisper  ".to_string(),
            String::new(),
            "Jot".to_string(),
        ];
        assert_eq!(
            dictionary_prompt(&dictionary).as_deref(),
            Some("Jot、Whisper")
        );

        let long: Vec<String> = (0..50).map(|index| format!("词{index:02}")).collect();
        let prompt = dictionary_prompt(&long).unwrap();
        assert!(prompt.chars().count() <= DICTIONARY_PROMPT_MAX_CHARS);
    }

    #[test]
    fn empty_dictionary_has_no_prompt() {
        assert_eq!(dictionary_prompt(&[]), None);
        assert_eq!(dictionary_prompt(&["  ".to_string()]), None);
    }

    #[test]
    fn tail_chars_returns_the_end_of_the_text() {
        assert_eq!(tail_chars("abcdef", 2), "ef");
        assert_eq!(tail_chars("abc", 10), "abc");
        assert_eq!(tail_chars("  abc  ", 10), "abc");
    }

    #[test]
    fn config_requires_a_model() {
        let error = build_custom_whisper_config("http://localhost:8000/v1", "  ", &[]).unwrap_err();
        assert!(error.contains("转写模型不能为空"));
    }

    #[test]
    fn config_carries_dictionary_prompt() {
        let config = build_custom_whisper_config(
            "http://localhost:8000/v1",
            "whisper-1",
            &["Jot".to_string()],
        )
        .unwrap();

        assert_eq!(
            config.endpoint,
            "http://localhost:8000/v1/audio/transcriptions"
        );
        assert_eq!(
            config.dictionary_prompt.as_deref(),
            Some("Jot")
        );
        assert!(!config.api_key_required);
    }
}
