//! Native streaming transcription provider for Volcengine Speech (Doubao /
//! SeedASR).
//!
//! Binary framing, gzip handling and authentication headers follow the
//! Volcengine streaming ASR protocol. The concurrency model is:
//!
//! * the WebSocket is split into a sink and a stream after the handshake;
//! * a reader task parses every server frame and pushes [`TranscriptEvent`]s
//!   into an mpsc channel;
//! * a writer task drains a second mpsc channel and sends binary frames;
//! * [`SttProvider::send_audio`] only queues the frame (never awaits I/O) and
//!   [`SttProvider::recv_transcript`] only uses `try_recv`, so the pipeline can
//!   upload audio and collect partial transcripts in the same 30 ms tick.

use async_trait::async_trait;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use futures_util::{SinkExt, StreamExt};
use std::io::{Read, Write};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        handshake::client::generate_key,
        http::Request,
        Error as WsError, Message,
    },
};

use crate::error::AppError;
use crate::storage::{
    STT_PROVIDER_VOLCENGINE, STT_VOLC_RESOURCE_BIGASR, STT_VOLC_RESOURCE_SEEDASR,
};

use super::{SttConfig, SttProvider, TranscriptEvent};

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsSink = futures_util::stream::SplitSink<WsStream, Message>;
type WsSource = futures_util::stream::SplitStream<WsStream>;

/// Default Volcengine streaming ASR endpoint.
pub const VOLCENGINE_ASR_URL: &str =
    "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";
const VOLCENGINE_ASR_HOST: &str = "openspeech.bytedance.com";

/// One WebSocket attempt must finish its handshake within this window.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The initial full-client request must be acknowledged within this window.
pub const INITIAL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
/// `disconnect` waits at most this long for the final transcript.
pub const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the writer task may spend flushing the end frame + close frame.
const WRITER_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

const FULL_CLIENT_REQUEST: u8 = 0x1;
const AUDIO_ONLY_REQUEST: u8 = 0x2;
const FULL_SERVER_RESPONSE: u8 = 0x9;
const ERROR_RESPONSE: u8 = 0xf;
const POS_SEQUENCE: u8 = 0x1;
const NEG_SEQUENCE: u8 = 0x2;
const NEG_WITH_SEQUENCE: u8 = 0x3;
const JSON_SERIALIZATION: u8 = 0x1;
const NO_SERIALIZATION: u8 = 0x0;
const GZIP_COMPRESSION: u8 = 0x1;

/// Terminal events that only mean "the socket closed", not a protocol error.
const CONNECTION_CLOSED_MESSAGE: &str = "火山引擎识别连接已关闭";
const CONNECTION_ENDED_MESSAGE: &str = "火山引擎识别连接已断开";

/// Resolve the `X-Api-Resource-Id` value, falling back to SeedASR streaming.
pub fn resolve_resource_id(config: &SttConfig) -> &str {
    config
        .resource_id
        .as_deref()
        .map(str::trim)
        .filter(|resource_id| !resource_id.is_empty())
        .unwrap_or(STT_VOLC_RESOURCE_SEEDASR)
}

/// Normalize a user-supplied resource id to one of the two supported presets.
pub fn normalize_resource_id(resource_id: &str) -> String {
    let resource_id = resource_id.trim();
    if resource_id.eq_ignore_ascii_case(STT_VOLC_RESOURCE_BIGASR) {
        STT_VOLC_RESOURCE_BIGASR.to_string()
    } else if resource_id.eq_ignore_ascii_case(STT_VOLC_RESOURCE_SEEDASR) {
        STT_VOLC_RESOURCE_SEEDASR.to_string()
    } else {
        STT_VOLC_RESOURCE_SEEDASR.to_string()
    }
}

/// Heuristic used to map server-side error frames to `stt_auth`.
///
/// Volcengine reports bad keys both as HTTP 401/403 during the handshake and as
/// error frames with codes such as 401 plus textual messages after the
/// handshake; both shapes must end up as an authentication failure.
pub fn is_authentication_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    // Match 401/403 as standalone tokens so codes such as 40100 do not trigger
    // a false authentication classification.
    let has_auth_status = lower
        .split(|character: char| !character.is_ascii_digit())
        .any(|part| part == "401" || part == "403");
    if has_auth_status {
        return true;
    }

    const HINTS: &[&str] = &[
        "auth",
        "unauthor",
        "forbidden",
        "invalid key",
        "invalid api key",
        "invalid app key",
        "api key",
        "access key",
        "app key",
        "permission",
        "token",
        "credential",
        "鉴权",
        "密钥",
        "未授权",
    ];
    HINTS.iter().any(|hint| lower.contains(hint))
}

fn classify_event_error(message: String) -> AppError {
    if is_authentication_error(&message) {
        AppError::Auth(message)
    } else {
        AppError::Config(message)
    }
}

fn auth_error_message(status: Option<u16>) -> String {
    match status {
        Some(status) => format!(
            "火山引擎返回 {status}：鉴权失败。请填写火山引擎语音识别 API Key（UUID 形式），\
             或旧版控制台的 app_key:access_key；方舟大模型密钥不能用于语音识别。"
        ),
        None => "火山引擎鉴权失败：请填写火山引擎语音识别 API Key（UUID 形式），\
                 或旧版控制台的 app_key:access_key；方舟大模型密钥不能用于语音识别。"
            .to_string(),
    }
}

pub struct VolcengineProvider {
    audio_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    event_rx: Option<mpsc::UnboundedReceiver<TranscriptEvent>>,
    read_task: Option<tokio::task::JoinHandle<()>>,
    write_task: Option<tokio::task::JoinHandle<()>>,
    sequence: i32,
    sent_audio: bool,
    url: String,
}

impl Default for VolcengineProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl VolcengineProvider {
    pub fn new() -> Self {
        Self {
            audio_tx: None,
            event_rx: None,
            read_task: None,
            write_task: None,
            sequence: 1,
            sent_audio: false,
            url: VOLCENGINE_ASR_URL.to_string(),
        }
    }

    #[cfg(test)]
    fn with_url(url: String) -> Self {
        let mut provider = Self::new();
        provider.url = url;
        provider
    }

    fn abort_tasks(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }
        if let Some(task) = self.write_task.take() {
            task.abort();
        }
        self.audio_tx = None;
        self.event_rx = None;
    }
}

impl Drop for VolcengineProvider {
    fn drop(&mut self) {
        self.abort_tasks();
    }
}

fn gzip_payload(payload: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(payload)
        .map_err(|error| AppError::Config(format!("火山引擎请求压缩失败：{error}")))?;
    encoder
        .finish()
        .map_err(|error| AppError::Config(format!("火山引擎请求压缩失败：{error}")))
}

fn ungzip_payload(payload: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = GzDecoder::new(payload);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|error| AppError::Config(format!("火山引擎响应解压失败：{error}")))?;
    Ok(out)
}

fn volcengine_language(language: Option<&str>) -> Option<String> {
    match language.unwrap_or("").trim() {
        "" | "auto" | "Auto" | "AUTO" => None,
        "multi" | "zh" | "zh-CN" | "zh_cn" => Some("zh-CN".to_string()),
        "en" | "en-US" | "en_us" => Some("en-US".to_string()),
        "ja" | "ja-JP" | "ja_jp" => Some("ja-JP".to_string()),
        "ko" | "ko-KR" | "ko_kr" => Some("ko-KR".to_string()),
        other => Some(other.to_string()),
    }
}

fn header(message_type: u8, flags: u8, serialization: u8, compression: u8) -> [u8; 4] {
    [
        0x11,
        (message_type << 4) | flags,
        (serialization << 4) | compression,
        0x00,
    ]
}

fn build_frame(
    message_type: u8,
    flags: u8,
    serialization: u8,
    payload: &[u8],
    sequence: i32,
) -> Result<Vec<u8>, AppError> {
    let compressed = gzip_payload(payload)?;
    let mut frame = Vec::with_capacity(12 + compressed.len());
    frame.extend_from_slice(&header(
        message_type,
        flags,
        serialization,
        GZIP_COMPRESSION,
    ));
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    frame.extend_from_slice(&compressed);
    Ok(frame)
}

/// 组装 full client request 的 JSON。
///
/// 术语表通过 `request.corpus.context` 以**字符串形式**直传热词（火山引擎的
/// 协议要求 context 是一个 JSON 字符串），这是官方文档里「请求级、细粒度
/// 个性化热词」的用法，双向流式额度 100 token。
fn build_request_payload(config: &SttConfig) -> serde_json::Value {
    let mut audio = serde_json::json!({
        "format": "pcm",
        "codec": "raw",
        "rate": config.sample_rate,
        "bits": 16,
        "channel": 1
    });
    if let Some(language) = volcengine_language(config.language.as_deref()) {
        audio["language"] = serde_json::Value::String(language);
    }

    let mut request = serde_json::json!({
        "model_name": "bigmodel",
        "enable_itn": true,
        "enable_ddc": false,
        "enable_punc": true,
        "show_utterances": true
    });

    if !config.hotwords.is_empty() {
        let hotwords: Vec<serde_json::Value> = config
            .hotwords
            .iter()
            .map(|word| serde_json::json!({ "word": word }))
            .collect();
        let context = serde_json::json!({ "hotwords": hotwords }).to_string();
        request["corpus"] = serde_json::json!({ "context": context });
    }

    serde_json::json!({
        "user": { "uid": "Jot" },
        "audio": audio,
        "request": request
    })
}

fn build_full_client_request_frame(config: &SttConfig, sequence: i32) -> Result<Vec<u8>, AppError> {
    let payload = build_request_payload(config);
    build_frame(
        FULL_CLIENT_REQUEST,
        POS_SEQUENCE,
        JSON_SERIALIZATION,
        payload.to_string().as_bytes(),
        sequence,
    )
}

fn build_audio_request_frame(chunk: &[u8], sequence: i32) -> Result<Vec<u8>, AppError> {
    let flags = if sequence < 0 {
        NEG_WITH_SEQUENCE
    } else {
        POS_SEQUENCE
    };
    build_frame(AUDIO_ONLY_REQUEST, flags, NO_SERIALIZATION, chunk, sequence)
}

fn extract_transcript(value: &serde_json::Value) -> String {
    value["result"]["text"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            value["result"]["utterances"]
                .as_array()
                .and_then(|utterances| utterances.last())
                .and_then(|item| item["text"].as_str())
                .filter(|text| !text.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn response_message(value: &serde_json::Value) -> String {
    value["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .or_else(|| value["error"]["message"].as_str())
        .unwrap_or("")
        .to_string()
}

fn server_error_message(error_code: Option<i32>, message: &str) -> String {
    let prefix = match error_code {
        Some(code) => format!("火山引擎返回错误码 {code}"),
        None => "火山引擎返回错误".to_string(),
    };
    let message = truncate_chars(message.trim(), 200);
    if message.is_empty() {
        prefix
    } else {
        format!("{prefix}：{message}")
    }
}

fn parse_response_frame(frame: &[u8]) -> Result<Option<TranscriptEvent>, AppError> {
    if frame.len() < 8 {
        return Err(AppError::Config("火山引擎响应帧过短".to_string()));
    }

    let header_size = ((frame[0] & 0x0f) as usize) * 4;
    if frame.len() < header_size + 4 {
        return Err(AppError::Config("火山引擎响应帧头不完整".to_string()));
    }

    let message_type = frame[1] >> 4;
    let flags = frame[1] & 0x0f;
    let serialization = frame[2] >> 4;
    let compression = frame[2] & 0x0f;

    let mut offset = header_size;
    let sequence = if flags & POS_SEQUENCE != 0 {
        if frame.len() < offset + 4 {
            return Err(AppError::Config("火山引擎响应帧序号不完整".to_string()));
        }
        let seq = i32::from_be_bytes(frame[offset..offset + 4].try_into().unwrap());
        offset += 4;
        Some(seq)
    } else {
        None
    };
    let is_last = flags & NEG_SEQUENCE != 0;

    if flags & 0x04 != 0 {
        if frame.len() < offset + 4 {
            return Err(AppError::Config("火山引擎响应帧事件字段不完整".to_string()));
        }
        offset += 4;
    }

    let error_code = if message_type == ERROR_RESPONSE {
        if frame.len() < offset + 8 {
            return Err(AppError::Config("火山引擎错误帧不完整".to_string()));
        }
        let code = i32::from_be_bytes(frame[offset..offset + 4].try_into().unwrap());
        offset += 4;
        Some(code)
    } else {
        None
    };

    if frame.len() < offset + 4 {
        return Err(AppError::Config("火山引擎响应帧缺少负载长度".to_string()));
    }
    let payload_size = u32::from_be_bytes(frame[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;

    if payload_size == 0 && message_type == ERROR_RESPONSE {
        return Ok(Some(TranscriptEvent::Error {
            message: server_error_message(error_code, ""),
        }));
    }

    if frame.len() < offset + payload_size {
        return Err(AppError::Config("火山引擎响应帧负载不完整".to_string()));
    }
    let payload = &frame[offset..offset + payload_size];
    let payload = if compression == GZIP_COMPRESSION {
        ungzip_payload(payload)?
    } else {
        payload.to_vec()
    };

    if serialization != JSON_SERIALIZATION {
        if message_type == ERROR_RESPONSE {
            return Ok(Some(TranscriptEvent::Error {
                message: server_error_message(error_code, ""),
            }));
        }
        return Ok(None);
    }

    let value: serde_json::Value = serde_json::from_slice(&payload)
        .map_err(|error| AppError::Config(format!("火山引擎响应不是合法 JSON：{error}")))?;

    if message_type == ERROR_RESPONSE
        || value["code"]
            .as_i64()
            .is_some_and(|code| code != 0 && code != 20_000_000)
    {
        let message = response_message(&value);
        let message = if message.is_empty() {
            server_error_message(error_code, "")
        } else {
            server_error_message(error_code, &message)
        };
        return Ok(Some(TranscriptEvent::Error { message }));
    }

    if message_type != FULL_SERVER_RESPONSE {
        return Ok(None);
    }

    let text = extract_transcript(&value);
    if text.trim().is_empty() {
        return Ok(None);
    }

    let is_final = is_last || sequence.is_some_and(|seq| seq < 0);
    if is_final {
        Ok(Some(TranscriptEvent::Final {
            text,
            confidence: 1.0,
        }))
    } else {
        Ok(Some(TranscriptEvent::Partial { text }))
    }
}

async fn read_initial_response(stream: &mut WsSource) -> Result<(), AppError> {
    let next = tokio::time::timeout(INITIAL_RESPONSE_TIMEOUT, stream.next())
        .await
        .map_err(|_| {
            AppError::Network(format!(
                "火山引擎在 {} 秒内没有返回初始响应",
                INITIAL_RESPONSE_TIMEOUT.as_secs()
            ))
        })?;

    match next {
        Some(Ok(Message::Binary(data))) => match parse_response_frame(&data)? {
            Some(TranscriptEvent::Error { message }) => Err(classify_event_error(message)),
            _ => Ok(()),
        },
        Some(Ok(Message::Text(text))) => Err(classify_event_error(text)),
        Some(Ok(Message::Close(_))) | None => Err(AppError::Network(
            "火山引擎在返回初始响应前关闭了连接".to_string(),
        )),
        Some(Ok(_)) => Ok(()),
        Some(Err(error)) => Err(AppError::Network(format!(
            "火山引擎连接出错：{error}"
        ))),
    }
}

async fn read_loop(mut stream: WsSource, event_tx: mpsc::UnboundedSender<TranscriptEvent>) {
    while let Some(message) = stream.next().await {
        match message {
            Ok(Message::Binary(data)) => match parse_response_frame(&data) {
                Ok(Some(event)) => {
                    if event_tx.send(event).is_err() {
                        return;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = event_tx.send(TranscriptEvent::Error {
                        message: format!("火山引擎响应解析失败：{error}"),
                    });
                    return;
                }
            },
            Ok(Message::Text(text)) => {
                let _ = event_tx.send(TranscriptEvent::Error {
                    message: format!(
                        "火山引擎返回：{}",
                        truncate_chars(text.trim(), 200)
                    ),
                });
                return;
            }
            Ok(Message::Close(_)) => {
                let _ = event_tx.send(TranscriptEvent::Error {
                    message: CONNECTION_CLOSED_MESSAGE.to_string(),
                });
                return;
            }
            Ok(_) => {}
            Err(error) => {
                let _ = event_tx.send(TranscriptEvent::Error {
                    message: format!("火山引擎连接出错：{error}"),
                });
                return;
            }
        }
    }

    let _ = event_tx.send(TranscriptEvent::Error {
        message: CONNECTION_ENDED_MESSAGE.to_string(),
    });
}

async fn write_loop(
    mut sink: WsSink,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    event_tx: mpsc::UnboundedSender<TranscriptEvent>,
) {
    while let Some(frame) = audio_rx.recv().await {
        if let Err(error) = sink.send(Message::Binary(frame)).await {
            let _ = event_tx.send(TranscriptEvent::Error {
                message: format!("火山引擎音频发送失败：{error}"),
            });
            return;
        }
    }

    let _ = tokio::time::timeout(WRITER_FLUSH_TIMEOUT, sink.close()).await;
}

fn connect_id() -> String {
    format!(
        "jot-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    )
}

fn auth_headers(api_key: &str) -> Result<Vec<(&'static str, String)>, AppError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::Auth(
            "火山引擎 API Key 为空，请先在设置中填写".to_string(),
        ));
    }

    if let Some((app_key, access_key)) = api_key.split_once(':') {
        if !app_key.trim().is_empty() && !access_key.trim().is_empty() {
            return Ok(vec![
                ("X-Api-App-Key", app_key.trim().to_string()),
                ("X-Api-Access-Key", access_key.trim().to_string()),
            ]);
        }
    }

    Ok(vec![("X-Api-Key", api_key.to_string())])
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn map_connect_error(error: WsError) -> AppError {
    match error {
        WsError::Http(response) if response.status() == 401 || response.status() == 403 => {
            AppError::Auth(auth_error_message(Some(response.status().as_u16())))
        }
        WsError::Http(response) => {
            let status = response.status();
            let body = response
                .body()
                .as_ref()
                .and_then(|body| String::from_utf8(body.clone()).ok())
                .map(|body| truncate_chars(body.trim(), 200))
                .filter(|body| !body.is_empty())
                .unwrap_or_else(|| status.to_string());
            AppError::Api {
                status: status.as_u16(),
                body,
            }
        }
        other => AppError::Network(format!("无法连接火山引擎：{other}")),
    }
}

#[async_trait]
impl SttProvider for VolcengineProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError> {
        self.abort_tasks();
        let api_key = config.api_key.trim().to_string();
        let auth_headers = auth_headers(&api_key)?;
        let resource_id = resolve_resource_id(config).to_string();
        let url = self.url.clone();

        let mut attempt = 0u32;
        loop {
            let mut builder = Request::builder()
                .uri(&url)
                .header("Host", VOLCENGINE_ASR_HOST)
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", generate_key())
                .header("X-Api-Resource-Id", resource_id.as_str())
                .header("X-Api-Connect-Id", connect_id());
            for (name, value) in &auth_headers {
                builder = builder.header(*name, value.as_str());
            }

            let request = builder
                .body(())
                .map_err(|error| AppError::Config(format!("构建火山引擎连接请求失败：{error}")))?;

            let attempt_result =
                tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request)).await;

            let stream = match attempt_result {
                Ok(Ok((stream, _))) => stream,
                Ok(Err(error)) => {
                    let error = map_connect_error(error);
                    if !error.is_retryable() || attempt >= 2 {
                        return Err(error);
                    }
                    tracing::warn!(
                        "火山引擎连接失败（第 {}/3 次）：{}",
                        attempt + 1,
                        error
                    );
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(1000 * 2u64.pow(attempt - 1))).await;
                    continue;
                }
                Err(_) => {
                    let error = AppError::Network(format!(
                        "连接火山引擎超时（{} 秒）",
                        CONNECT_TIMEOUT.as_secs()
                    ));
                    if attempt >= 2 {
                        return Err(error);
                    }
                    tracing::warn!(
                        "连接火山引擎超时（第 {}/3 次）",
                        attempt + 1
                    );
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(1000 * 2u64.pow(attempt - 1))).await;
                    continue;
                }
            };

            let (mut sink, mut input_stream) = stream.split();

            let initial_frame = build_full_client_request_frame(config, 1)?;
            sink.send(Message::Binary(initial_frame))
                .await
                .map_err(|error| AppError::Network(format!("发送火山引擎初始请求失败：{error}")))?;
            read_initial_response(&mut input_stream).await?;

            let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<u8>>();
            let (event_tx, event_rx) = mpsc::unbounded_channel::<TranscriptEvent>();

            let read_event_tx = event_tx.clone();
            let read_task = tokio::spawn(async move {
                read_loop(input_stream, read_event_tx).await;
            });

            let write_event_tx = event_tx;
            let write_task = tokio::spawn(async move {
                write_loop(sink, audio_rx, write_event_tx).await;
            });

            self.abort_tasks();
            self.sequence = 1;
            self.sent_audio = false;
            self.audio_tx = Some(audio_tx);
            self.event_rx = Some(event_rx);
            self.read_task = Some(read_task);
            self.write_task = Some(write_task);
            tracing::info!(
                "火山引擎流式识别已连接（resource_id={resource_id}）"
            );
            return Ok(());
        }
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError> {
        if chunk.is_empty() {
            return Ok(());
        }
        let Some(audio_tx) = self.audio_tx.as_ref() else {
            return Err(AppError::Network(
                "火山引擎识别连接尚未建立".to_string(),
            ));
        };

        self.sequence += 1;
        let frame = build_audio_request_frame(chunk, self.sequence)?;
        audio_tx.send(frame).map_err(|_| {
            AppError::Network("火山引擎识别连接已断开，无法继续发送音频".to_string())
        })?;
        self.sent_audio = true;
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError> {
        let Some(event_rx) = self.event_rx.as_mut() else {
            return Ok(None);
        };

        match event_rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(AppError::Network(
                "火山引擎识别连接已断开".to_string(),
            )),
        }
    }

    async fn disconnect(&mut self) -> Result<Option<String>, AppError> {
        let end_frame = if self.sent_audio {
            self.sequence += 1;
            Some(build_audio_request_frame(&[], -self.sequence.abs())?)
        } else {
            None
        };

        let audio_tx = self.audio_tx.take();
        let mut event_rx = self.event_rx.take();
        let read_task = self.read_task.take();
        let mut write_task = self.write_task.take();

        let mut final_text: Option<String> = None;
        let mut fatal_error: Option<AppError> = None;

        if let Some(end_frame) = end_frame {
            if let Some(audio_tx) = audio_tx.as_ref() {
                let _ = audio_tx.send(end_frame);
            }
            drop(audio_tx);

            if let Some(event_rx) = event_rx.as_mut() {
                let deadline = tokio::time::Instant::now() + DISCONNECT_TIMEOUT;
                loop {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    match tokio::time::timeout(remaining, event_rx.recv()).await {
                        Ok(Some(TranscriptEvent::Final { text, .. })) => {
                            if !text.trim().is_empty() {
                                final_text = Some(text);
                            }
                            break;
                        }
                        Ok(Some(TranscriptEvent::Partial { text })) => {
                            if !text.trim().is_empty() {
                                final_text = Some(text);
                            }
                        }
                        Ok(Some(TranscriptEvent::Error { message })) => {
                            // A plain close after the end frame is a normal
                            // terminator; keep the last partial as the final.
                            if message == CONNECTION_CLOSED_MESSAGE
                                || message == CONNECTION_ENDED_MESSAGE
                            {
                                break;
                            }
                            fatal_error = Some(classify_event_error(message));
                            break;
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        } else {
            drop(audio_tx);
        }

        if let Some(mut write_task) = write_task.take() {
            if tokio::time::timeout(WRITER_FLUSH_TIMEOUT, &mut write_task)
                .await
                .is_err()
            {
                write_task.abort();
            }
        }
        if let Some(read_task) = read_task {
            read_task.abort();
        }

        self.sent_audio = false;
        tracing::info!("火山引擎流式识别已断开");

        if let Some(error) = fatal_error {
            return Err(error);
        }
        Ok(final_text)
    }

    fn name(&self) -> &str {
        STT_PROVIDER_VOLCENGINE
    }
}

#[cfg(test)]
fn build_test_server_response_frame(sequence: i32, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(12 + payload.len());
    frame.extend_from_slice(&header(
        FULL_SERVER_RESPONSE,
        if sequence < 0 { 0x03 } else { POS_SEQUENCE },
        JSON_SERIALIZATION,
        GZIP_COMPRESSION,
    ));
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

#[cfg(test)]
mod tests {
    use crate::stt::{SttConfig, TranscriptEvent};
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    use super::*;

    fn test_config(language: Option<&str>) -> SttConfig {
        SttConfig {
            api_key: "test-key".to_string(),
            language: language.map(str::to_string),
            resource_id: None,
            ..SttConfig::default()
        }
    }

    fn gzipped_json(value: serde_json::Value) -> Vec<u8> {
        gzip_payload(value.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn request_payload_omits_corpus_without_hotwords() {
        let payload = build_request_payload(&test_config(None));
        assert!(payload["request"].get("corpus").is_none());
        assert_eq!(payload["request"]["model_name"], "bigmodel");
    }

    #[test]
    fn request_payload_carries_hotwords_as_context_string() {
        let config = test_config(None).with_hotwords(vec!["飞书", "接口联调", "SeedASR"]);
        let payload = build_request_payload(&config);

        // 协议要求 context 是字符串，里面才是 JSON
        let context = payload["request"]["corpus"]["context"]
            .as_str()
            .expect("context 必须是字符串");
        let parsed: serde_json::Value = serde_json::from_str(context).unwrap();
        let words: Vec<&str> = parsed["hotwords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["word"].as_str().unwrap())
            .collect();

        assert_eq!(words, vec!["飞书", "接口联调", "SeedASR"]);
    }

    #[test]
    fn request_payload_drops_duplicate_and_overlong_hotwords() {
        let mut words: Vec<String> = vec!["飞书".to_string(), "飞书".to_string()];
        words.extend((0..60).map(|index| format!("术语{index}")));
        let borrowed: Vec<&str> = words.iter().map(String::as_str).collect();
        let config = test_config(None).with_hotwords(borrowed);

        assert!(config.hotwords.len() <= 40);
        assert_eq!(config.hotwords.iter().filter(|word| *word == "飞书").count(), 1);
    }

    fn server_response_frame(sequence: i32, value: serde_json::Value) -> Vec<u8> {
        build_test_server_response_frame(sequence, &gzipped_json(value))
    }

    fn build_test_error_response_frame(code: i32, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(12 + payload.len());
        frame.extend_from_slice(&header(
            ERROR_RESPONSE,
            0x00,
            JSON_SERIALIZATION,
            GZIP_COMPRESSION,
        ));
        frame.extend_from_slice(&code.to_be_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn client_sequence(frame: &[u8]) -> i32 {
        if frame.len() < 8 {
            return 0;
        }
        i32::from_be_bytes(frame[4..8].try_into().unwrap())
    }

    #[test]
    fn resolves_default_and_custom_resource_id() {
        let mut config = test_config(None);
        assert_eq!(resolve_resource_id(&config), "volc.seedasr.sauc.duration");

        config.resource_id = Some(" volc.bigasr.sauc.duration ".to_string());
        assert_eq!(resolve_resource_id(&config), "volc.bigasr.sauc.duration");
    }

    #[test]
    fn normalizes_resource_id_to_supported_presets() {
        assert_eq!(
            normalize_resource_id("VOLC.BIGASR.SAUC.DURATION"),
            "volc.bigasr.sauc.duration"
        );
        assert_eq!(normalize_resource_id("nope"), "volc.seedasr.sauc.duration");
        assert_eq!(normalize_resource_id("  "), "volc.seedasr.sauc.duration");
    }

    #[test]
    fn builds_full_client_request_with_sequence_and_gzipped_json_payload() {
        let frame = build_full_client_request_frame(&test_config(Some("zh")), 1).unwrap();

        assert_eq!(frame[0], 0x11);
        assert_eq!(frame[1], 0x11);
        assert_eq!(frame[2], 0x11);
        assert_eq!(frame[3], 0x00);
        assert_eq!(i32::from_be_bytes(frame[4..8].try_into().unwrap()), 1);

        let payload_size = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
        let payload = ungzip_payload(&frame[12..12 + payload_size]).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();

        assert_eq!(value["audio"]["format"], "pcm");
        assert_eq!(value["audio"]["rate"], 16000);
        assert_eq!(value["audio"]["bits"], 16);
        assert_eq!(value["audio"]["channel"], 1);
        assert_eq!(value["audio"]["language"], "zh-CN");
        assert_eq!(value["request"]["model_name"], "bigmodel");
        assert_eq!(value["request"]["enable_itn"], true);
        assert_eq!(value["request"]["enable_punc"], true);
        assert_eq!(value["request"]["show_utterances"], true);
    }

    #[test]
    fn auto_language_is_omitted_from_the_request_json() {
        let frame = build_full_client_request_frame(&test_config(None), 1).unwrap();
        let payload_size = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
        let payload = ungzip_payload(&frame[12..12 + payload_size]).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();

        assert!(value["audio"].get("language").is_none());
    }

    #[test]
    fn builds_final_audio_frame_with_negative_sequence() {
        let frame = build_audio_request_frame(b"\x01\x02", -2).unwrap();

        assert_eq!(frame[0], 0x11);
        assert_eq!(frame[1], 0x23);
        assert_eq!(frame[2], 0x01);
        assert_eq!(frame[3], 0x00);
        assert_eq!(i32::from_be_bytes(frame[4..8].try_into().unwrap()), -2);

        let payload_size = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
        assert_eq!(
            ungzip_payload(&frame[12..12 + payload_size]).unwrap(),
            b"\x01\x02"
        );
    }

    #[test]
    fn parses_error_response_code_and_message_payload() {
        let payload = gzip_payload(
            serde_json::json!({
                "message": "invalid resource"
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let frame = build_test_error_response_frame(400, &payload);

        let event = parse_response_frame(&frame).unwrap().unwrap();

        match event {
            TranscriptEvent::Error { message } => {
                assert!(message.contains("400"));
                assert!(message.contains("invalid resource"));
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[test]
    fn parses_final_response_without_sequence_flag() {
        let payload = gzipped_json(serde_json::json!({
            "result": { "text": "hello world" }
        }));
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(&header(
            FULL_SERVER_RESPONSE,
            0x02,
            JSON_SERIALIZATION,
            GZIP_COMPRESSION,
        ));
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);

        let event = parse_response_frame(&frame).unwrap().unwrap();

        match event {
            TranscriptEvent::Final { text, confidence } => {
                assert_eq!(text, "hello world");
                assert_eq!(confidence, 1.0);
            }
            other => panic!("expected final transcript, got {other:?}"),
        }
    }

    #[test]
    fn parses_partial_server_response_text() {
        let frame = server_response_frame(
            2,
            serde_json::json!({ "result": { "text": "你好" } }),
        );

        let event = parse_response_frame(&frame).unwrap().unwrap();

        assert_eq!(
            event,
            TranscriptEvent::Partial {
                text: "你好".to_string()
            }
        );
    }

    #[test]
    fn parses_final_server_response_text() {
        let frame = server_response_frame(
            -3,
            serde_json::json!({
                "result": {
                    "text": "hello world",
                    "utterances": [
                        { "text": "hello world", "definite": true }
                    ]
                }
            }),
        );

        let event = parse_response_frame(&frame).unwrap().unwrap();

        match event {
            TranscriptEvent::Final { text, confidence } => {
                assert_eq!(text, "hello world");
                assert_eq!(confidence, 1.0);
            }
            other => panic!("expected final transcript, got {other:?}"),
        }
    }

    #[test]
    fn recognizes_authentication_error_messages() {
        assert!(is_authentication_error("火山引擎返回错误码 401：bad key"));
        assert!(is_authentication_error("invalid api key"));
        assert!(is_authentication_error("鉴权失败"));
        assert!(!is_authentication_error("火山引擎响应解析失败：bad frame"));
        assert!(!is_authentication_error("火山引擎返回错误码 40100"));
    }

    #[test]
    fn maps_websocket_unauthorized_to_actionable_auth_error() {
        let response = tokio_tungstenite::tungstenite::http::Response::builder()
            .status(401)
            .body(None)
            .unwrap();

        let error = map_connect_error(WsError::Http(response));

        match error {
            AppError::Auth(message) => {
                assert!(message.contains("401"));
                assert!(message.contains("API Key"));
            }
            other => panic!("expected auth error, got {other}"),
        }
    }

    #[tokio::test]
    async fn connect_fails_when_initial_server_response_is_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await.unwrap().unwrap();

            let payload = gzip_payload(
                serde_json::json!({
                    "message": "invalid resource"
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
            let frame = build_test_error_response_frame(400, &payload);
            ws.send(Message::Binary(frame)).await.unwrap();
        });

        let mut provider = VolcengineProvider::with_url(format!("ws://{address}"));
        let error = provider.connect(&test_config(None)).await.unwrap_err();

        assert!(error.to_string().contains("invalid resource"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_maps_server_auth_error_to_auth_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await.unwrap().unwrap();

            let payload = gzip_payload(
                serde_json::json!({ "message": "invalid api key" })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
            let frame = build_test_error_response_frame(401, &payload);
            ws.send(Message::Binary(frame)).await.unwrap();
        });

        let mut provider = VolcengineProvider::with_url(format!("ws://{address}"));
        let error = provider.connect(&test_config(None)).await.unwrap_err();

        assert!(matches!(error, AppError::Auth(_)), "{error:?}");
        assert!(error.to_string().contains("401"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_send_and_receive_returns_the_final_transcript() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            let _initial = ws.next().await.unwrap().unwrap();
            ws.send(Message::Binary(server_response_frame(
                1,
                serde_json::json!({ "result": { "text": "" } }),
            )))
            .await
            .unwrap();

            while let Some(Ok(message)) = ws.next().await {
                let Message::Binary(frame) = message else {
                    continue;
                };
                let sequence = client_sequence(&frame);
                if sequence < 0 {
                    ws.send(Message::Binary(server_response_frame(
                        -3,
                        serde_json::json!({ "result": { "text": "你好世界" } }),
                    )))
                    .await
                    .unwrap();
                    break;
                }
                ws.send(Message::Binary(server_response_frame(
                    sequence,
                    serde_json::json!({ "result": { "text": "你好" } }),
                )))
                .await
                .unwrap();
            }
        });

        let mut provider = VolcengineProvider::with_url(format!("ws://{address}"));
        provider.connect(&test_config(None)).await.unwrap();
        provider.send_audio(&[0u8; 3_200]).await.unwrap();

        let mut partial = None;
        for _ in 0..100 {
            if let Some(event) = provider.recv_transcript().await.unwrap() {
                partial = Some(event);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            partial,
            Some(TranscriptEvent::Partial {
                text: "你好".to_string()
            })
        );

        let transcript = provider.disconnect().await.unwrap();
        assert_eq!(transcript.as_deref(), Some("你好世界"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn recv_transcript_is_non_blocking_before_any_event_arrives() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _initial = ws.next().await.unwrap().unwrap();
            ws.send(Message::Binary(server_response_frame(
                1,
                serde_json::json!({ "result": { "text": "" } }),
            )))
            .await
            .unwrap();
            // Keep the connection open until the client drops it.
            while ws.next().await.is_some() {}
        });

        let mut provider = VolcengineProvider::with_url(format!("ws://{address}"));
        provider.connect(&test_config(None)).await.unwrap();

        let event = tokio::time::timeout(Duration::from_millis(50), provider.recv_transcript())
            .await
            .expect("recv_transcript must not block")
            .unwrap();
        assert_eq!(event, None);

        drop(provider);
        server.await.unwrap();
    }
}
