//! Connection diagnostics used by the settings screen.

use std::time::Duration;

use tauri::State;

use crate::credentials::{
    self, SystemCredentialVault, CUSTOM_PROVIDER, VOLCENGINE_PROVIDER,
};
use crate::error::AppError;
use crate::llm::{self, models_endpoint};
use crate::pipeline::{llm_error_details, stt_error_details};
use crate::storage::STT_PROVIDER_VOLCENGINE;
use crate::stt::config::build_custom_whisper_config;
use crate::stt::volcengine;
use crate::stt::SttConfig;
use crate::stt::whisper_compat::WhisperCompatProvider;

/// Upper bound for the Volcengine connectivity probe.
const VOLCENGINE_TEST_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the probe waits for the final transcript before reporting success.
const VOLCENGINE_TEST_FINAL_TIMEOUT: Duration = Duration::from_secs(3);

/// 0.1 s of silence at 16 kHz / 16-bit mono — enough to validate auth and model.
fn silent_pcm() -> Vec<u8> {
    vec![0u8; 3_200]
}

/// 0.2 s of silence, used to exercise the streaming WebSocket end-to-end.
fn silent_pcm_200ms() -> Vec<u8> {
    vec![0u8; 6_400]
}

fn resolve_api_key(provided: &str, namespace: &str, provider: &str) -> Result<String, String> {
    let provided = provided.trim();
    if !provided.is_empty() {
        return Ok(provided.to_string());
    }

    let stored = if namespace == credentials::STT_NAMESPACE {
        credentials::read_stt_secret_for_provider(&SystemCredentialVault, provider)
    } else {
        credentials::read_llm_secret(&SystemCredentialVault)
    };
    stored.map_err(|error| format!("无法读取系统钥匙串中的密钥：{error}"))
}

#[tauri::command]
pub async fn test_stt_connection(
    api_key: String,
    base_url: String,
    model: String,
    language: String,
    provider: String,
    resource_id: String,
    client: State<'_, reqwest::Client>,
) -> Result<u32, String> {
    if provider.trim().eq_ignore_ascii_case(STT_PROVIDER_VOLCENGINE) {
        return test_volcengine_connection(api_key, language, resource_id).await;
    }

    let api_key = resolve_api_key(&api_key, credentials::STT_NAMESPACE, CUSTOM_PROVIDER)?;
    let provider_config =
        build_custom_whisper_config(&base_url, &model, &[]).map_err(|error| error.to_string())?;
    let stt_config = SttConfig::from_language(api_key, &language);

    let provider = WhisperCompatProvider::with_client(provider_config, client.inner().clone());
    let wav = WhisperCompatProvider::build_wav(&silent_pcm(), stt_config.sample_rate);

    let started = std::time::Instant::now();
    match provider.transcribe_wav(wav, &stt_config, None).await {
        Ok(_) => Ok(started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32),
        Err(error) => Err(stt_error_details(&error)),
    }
}

/// Probe the native Volcengine streaming endpoint: real WebSocket handshake,
/// 200 ms of silence plus the end frame. A readable transcript is not required;
/// only connectivity and authentication are verified.
async fn test_volcengine_connection(
    api_key: String,
    language: String,
    resource_id: String,
) -> Result<u32, String> {
    let api_key = resolve_api_key(
        &api_key,
        credentials::STT_NAMESPACE,
        VOLCENGINE_PROVIDER,
    )?;
    if api_key.trim().is_empty() {
        return Err("请先在设置中填写火山引擎 API Key".to_string());
    }
    if resource_id.trim().is_empty() {
        return Err("请先在设置中选择火山引擎 Resource ID".to_string());
    }

    let stt_config = SttConfig::from_language(api_key, &language)
        .with_resource_id(&volcengine::normalize_resource_id(&resource_id));

    let started = std::time::Instant::now();
    let probe = tokio::time::timeout(VOLCENGINE_TEST_TIMEOUT, async {
        let mut provider = crate::stt::create_volcengine_provider();
        provider.connect(&stt_config).await?;
        provider.send_audio(&silent_pcm_200ms()).await?;

        // The handshake is what we report on. Still flush the end frame and give
        // the server a short window to surface an auth error frame.
        match tokio::time::timeout(
            VOLCENGINE_TEST_FINAL_TIMEOUT,
            provider.disconnect(),
        )
        .await
        {
            Ok(Err(error)) if matches!(error, AppError::Auth(_)) => Err(error),
            _ => Ok(()),
        }
    })
    .await;

    match probe {
        Ok(Ok(())) => Ok(started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32),
        Ok(Err(error)) => Err(stt_error_details(&error)),
        Err(_) => Err(format!(
            "连接火山引擎超时（{} 秒）",
            VOLCENGINE_TEST_TIMEOUT.as_secs()
        )),
    }
}

#[tauri::command]
pub async fn bench_llm_connection(
    api_key: String,
    base_url: String,
    model: String,
    disable_thinking: bool,
    client: State<'_, reqwest::Client>,
) -> Result<u32, String> {
    let api_key = resolve_api_key(&api_key, credentials::LLM_NAMESPACE, CUSTOM_PROVIDER)?;
    let model = model.trim();
    if model.is_empty() {
        return Err("请先填写润色模型".to_string());
    }

    let endpoint = llm::chat_endpoint(&base_url).map_err(|error| llm_error_details(&error))?;

    let mut body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "stream": false,
    });
    // 与真实润色路径保持一致：关掉推理模型的思考阶段，否则探测要等好几秒。
    if disable_thinking {
        body["reasoning_effort"] = serde_json::json!("none");
    }

    let build_request = |body: &serde_json::Value| {
        let mut request = client
            .post(endpoint.clone())
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(30));
        if !api_key.trim().is_empty() {
            request = request.header("Authorization", format!("Bearer {api_key}"));
        }
        request.json(body)
    };

    let started = std::time::Instant::now();
    let mut response = build_request(&body).send().await.map_err(|error| {
        let app_error: AppError = error.into();
        llm_error_details(&app_error)
    })?;

    // 该服务不认识 reasoning_effort：去掉再试一次
    if !response.status().is_success()
        && (400..500).contains(&response.status().as_u16())
        && body.get("reasoning_effort").is_some()
    {
        if let Some(object) = body.as_object_mut() {
            object.remove("reasoning_effort");
        }
        response = build_request(&body).send().await.map_err(|error| {
            let app_error: AppError = error.into();
            llm_error_details(&app_error)
        })?;
    }

    let elapsed = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;

    if response.status().is_success() {
        return Ok(elapsed);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let summary: String = body.trim().chars().take(200).collect();
    Err(if summary.is_empty() {
        format!("润色接口返回 {status}")
    } else {
        format!("润色接口返回 {status}：{summary}")
    })
}

#[tauri::command]
pub async fn fetch_llm_models(
    api_key: String,
    base_url: String,
    client: State<'_, reqwest::Client>,
) -> Result<Vec<String>, String> {
    let api_key = resolve_api_key(&api_key, credentials::LLM_NAMESPACE, CUSTOM_PROVIDER)?;
    let endpoint = models_endpoint(&base_url).map_err(|error| llm_error_details(&error))?;

    let mut request = client
        .get(endpoint)
        .timeout(std::time::Duration::from_secs(15));
    if !api_key.trim().is_empty() {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }

    let response = request.send().await.map_err(|error| {
        let app_error: AppError = error.into();
        llm_error_details(&app_error)
    })?;
    if !response.status().is_success() {
        return Err(format!("获取模型列表失败：接口返回 {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("无法解析模型列表响应：{error}"))?;

    let mut models: Vec<String> = Vec::new();
    if let Some(data) = body.get("data").and_then(|value| value.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|value| value.as_str()) {
                models.push(id.to_string());
            }
        }
    } else if let Some(data) = body.get("models").and_then(|value| value.as_array()) {
        for item in data {
            if let Some(name) = item.get("name").and_then(|value| value.as_str()) {
                models.push(name.to_string());
            }
        }
    }

    models.sort();
    models.dedup();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_audio_is_one_tenth_of_a_second() {
        assert_eq!(silent_pcm().len(), 3_200);
        assert_eq!(silent_pcm_200ms().len(), 6_400);
    }

    #[test]
    fn provided_key_wins_over_the_vault() {
        assert_eq!(
            resolve_api_key("  typed-key  ", "stt", CUSTOM_PROVIDER).unwrap(),
            "typed-key"
        );
    }
}
