//! Recording → transcription → (polish) → text insertion pipeline.
//!
//! The state machine is deliberately small:
//!
//! ```text
//! idle → preparing → recording → transcribing → polishing → outputting → idle
//! ```
//!
//! While recording, an optional "live dictation" task emits `stt:partial`.
//! For the default OpenAI-compatible provider it repeatedly transcribes the
//! audio captured since the previous cut point over HTTP. For the Volcengine
//! provider it instead runs a native WebSocket session (`send_audio` queues
//! frames, `recv_transcript` polls parsed events) and `stop()` uses the
//! server's final transcript without re-transcribing the whole recording.
//! When the user stops:
//! * `streaming_insert == false` (default): the segment results are dropped and
//!   the whole recording is transcribed once more for maximum accuracy.
//! * `streaming_insert == true`: the segments were already typed at the cursor,
//!   so only the remaining tail is transcribed and typed.
//! The Volcengine path follows the same insert rules but never re-runs a batch
//! transcription.

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};
use tokio::sync::oneshot;

use crate::audio::{AudioCaptureHandle, AudioConfig};
use crate::credentials::{self, SystemCredentialVault};
use crate::error::AppError;
use crate::llm::{self, LlmConfig, PolishRequest};
use crate::output::clipboard::{ClipboardOutputOptions, PasteShortcut};
use crate::output::{self, OutputMode};
use crate::recording_deadline::{
    drive_recording_deadline, CaptureReadyAt, RecordingDeadline, RecordingKind,
};
use crate::storage::{
    AppConfig, ConfigManager, OUTPUT_MODE_CLIPBOARD, STT_PROVIDER_VOLCENGINE,
};
use crate::stt::config::{build_custom_whisper_config, tail_chars, CUSTOM_WHISPER_PROVIDER};
use crate::stt::volcengine;
use crate::stt::whisper_compat::WhisperCompatProvider;
use crate::stt::{self, SttConfig, SttProvider, TranscriptEvent};

/// Volume polling interval while recording (contract: ~100 ms).
pub const VOLUME_EMIT_INTERVAL: Duration = Duration::from_millis(100);
/// PCM 16 kHz mono 16-bit = 32 000 bytes per second.
pub const AUDIO_BYTES_PER_SECOND: usize = 16_000 * 2;
/// Segments shorter than this are skipped to avoid noisy requests.
pub const MIN_SEGMENT_MS: u64 = 400;
/// The trailing chunk is only transcribed when it is at least this long.
pub const MIN_TAIL_MS: u64 = 200;
/// Max characters of confirmed text sent as the streaming context prompt.
pub const STREAMING_CONTEXT_CHARS: usize = 200;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const COLLECTOR_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(180);
/// Upper bound for the Volcengine WebSocket handshake during `start()`.
const VOLC_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Upper bound for the Volcengine final transcript in `stop()`.
const VOLC_FINAL_TIMEOUT: Duration = Duration::from_secs(15);
/// Poll cadence of the native streaming path (audio upload + event drain).
const VOLC_POLL_INTERVAL: Duration = Duration::from_millis(30);
/// Volcengine accepts small PCM frames; 200 ms (6 400 bytes) per frame keeps
/// latency low even when the audio collector produced a backlog.
const VOLC_MAX_FRAME_BYTES: usize = 6_400;
const LLM_TIMEOUT_NOTE: &str = "润色请求超时";

// ─── macOS accessibility helpers ───

/// On macOS, synthesised key events need Accessibility permission; without it
/// they are silently dropped. Always true on other platforms.
pub fn is_accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> u8;
        }
        unsafe { AXIsProcessTrusted() != 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Ask macOS for Accessibility permission and open the Privacy pane.
pub fn request_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        if is_accessibility_trusted() {
            return true;
        }

        let trusted_after_prompt = request_accessibility_permission_prompt();

        if let Err(error) = std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
        {
            tracing::warn!("无法打开 macOS 辅助功能设置：{error}");
        }

        trusted_after_prompt
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
fn request_accessibility_permission_prompt() -> bool {
    use std::ffi::c_void;
    use std::ptr;

    type Boolean = u8;
    type CFDictionaryRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFBooleanRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: CFBooleanRef;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
        fn CFRelease(cf: *const c_void);
    }

    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            ptr::null(),
            ptr::null(),
        );
        if options.is_null() {
            return false;
        }
        let trusted = AXIsProcessTrustedWithOptions(options) != 0;
        CFRelease(options);
        trusted
    }
}

// ─── Pipeline state ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    Idle,
    Preparing,
    Recording,
    Transcribing,
    Polishing,
    Outputting,
}

impl PipelineState {
    fn as_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Preparing => 1,
            Self::Recording => 2,
            Self::Transcribing => 3,
            Self::Polishing => 4,
            Self::Outputting => 5,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Preparing,
            2 => Self::Recording,
            3 => Self::Transcribing,
            4 => Self::Polishing,
            5 => Self::Outputting,
            _ => Self::Idle,
        }
    }
}

/// Payload of `pipeline:error`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineErrorPayload {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Payload of `recording:deadline` (camelCase per contract).
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingDeadlinePayload {
    pub session_id: u64,
    pub started_at_unix_ms: u64,
    pub deadline_at_unix_ms: u64,
    pub effective_max_seconds: u32,
}

/// Audio accumulated since the recording started, plus the streaming cut point.
#[derive(Debug, Default)]
struct RecorderBuffer {
    pcm: Vec<u8>,
    cut: usize,
}

impl RecorderBuffer {
    fn append(&mut self, chunk: &[u8]) {
        self.pcm.extend_from_slice(chunk);
    }

    /// Move ownership of everything after the cut point, if it is long enough.
    fn take_segment_if_ready(&mut self, min_bytes: usize) -> Option<Vec<u8>> {
        if self.pcm.len().saturating_sub(self.cut) < min_bytes {
            return None;
        }
        let segment = self.pcm[self.cut..].to_vec();
        self.cut = self.pcm.len();
        Some(segment)
    }

    /// Move ownership of whatever audio has not been streamed yet.
    fn take_tail(&mut self) -> Vec<u8> {
        let tail = self.pcm[self.cut..].to_vec();
        self.cut = self.pcm.len();
        tail
    }

    /// Move at most `max_bytes` of the unstreamed tail; leaves the rest for the
    /// next streaming tick so backlogged audio is uploaded in small frames.
    fn take_tail_limited(&mut self, max_bytes: usize) -> Vec<u8> {
        let end = (self.cut + max_bytes).min(self.pcm.len());
        let tail = self.pcm[self.cut..end].to_vec();
        self.cut = end;
        tail
    }

    fn whole(&self) -> Vec<u8> {
        self.pcm.clone()
    }
}

struct ActiveRecording {
    handle: AudioCaptureHandle,
    buffer: Arc<Mutex<RecorderBuffer>>,
    drained: oneshot::Receiver<()>,
    /// Provider that was selected when this recording started, so a settings
    /// change mid-recording cannot switch finalization paths.
    provider: String,
}

/// The connected native streaming provider, tagged with the session that owns
/// it so a late teardown can never clobber the next recording.
struct VolcProviderSlot {
    session: u64,
    provider: Box<dyn SttProvider>,
}

/// Outcome of the finalization phase, before insertion.
struct FinalDraft {
    text: String,
    /// Text that still has to be typed (only for live insertion).
    pending_insert: Option<String>,
    /// True when the transcript was already typed while recording.
    already_typed: bool,
}

/// Handle shared with the Tauri commands and the tray.
#[derive(Clone)]
pub struct PipelineHandle {
    app: tauri::AppHandle,
    client: reqwest::Client,
    state: Arc<AtomicU8>,
    session: Arc<AtomicU64>,
    abort: Arc<AtomicBool>,
    active: Arc<Mutex<Option<ActiveRecording>>>,
    partial_text: Arc<Mutex<String>>,
    /// Byte length of the streaming segment currently being transcribed, kept
    /// so `stop()` can hand that audio back to the tail transcription when the
    /// segment request is cancelled mid-flight.
    inflight_segment: Arc<Mutex<Option<usize>>>,
    streaming_task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// Native streaming provider shared by the poll loop and `stop()`.
    volc_provider: Arc<tokio::sync::Mutex<Option<VolcProviderSlot>>>,
    /// Text already typed at the cursor by the Volcengine live-insert path.
    volc_typed_text: Arc<Mutex<String>>,
    run_lock: Arc<tokio::sync::Mutex<()>>,
}

impl PipelineHandle {
    pub fn new(app: tauri::AppHandle, client: reqwest::Client) -> Self {
        Self {
            app,
            client,
            state: Arc::new(AtomicU8::new(PipelineState::Idle.as_u8())),
            session: Arc::new(AtomicU64::new(0)),
            abort: Arc::new(AtomicBool::new(false)),
            active: Arc::new(Mutex::new(None)),
            partial_text: Arc::new(Mutex::new(String::new())),
            inflight_segment: Arc::new(Mutex::new(None)),
            streaming_task: Arc::new(Mutex::new(None)),
            volc_provider: Arc::new(tokio::sync::Mutex::new(None)),
            volc_typed_text: Arc::new(Mutex::new(String::new())),
            run_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn current_state(&self) -> PipelineState {
        PipelineState::from_u8(self.state.load(Ordering::SeqCst))
    }

    pub fn is_recording(&self) -> bool {
        self.current_state() == PipelineState::Recording
    }

    fn config(&self) -> Result<AppConfig> {
        match self.app.try_state::<ConfigManager>() {
            Some(manager) => manager.load(),
            None => Err(anyhow!("配置管理器尚未初始化")),
        }
    }

    fn set_state(&self, state: PipelineState) {
        self.state.store(state.as_u8(), Ordering::SeqCst);
        let _ = self.app.emit("pipeline:state", state);
        crate::tray::refresh_tray(&self.app);
    }

    fn finish_idle(&self) {
        self.partial_text
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.set_state(PipelineState::Idle);
    }

    fn emit_error(&self, code: &str, details: Option<String>) {
        let payload = PipelineErrorPayload {
            code: code.to_string(),
            details,
        };
        let _ = self.app.emit("pipeline:error", payload);
    }

    fn emit_partial(&self, text: &str) {
        let _ = self.app.emit("stt:partial", text.to_string());
    }

    fn is_active_session(&self, session: u64) -> bool {
        !self.abort.load(Ordering::SeqCst)
            && self.session.load(Ordering::SeqCst) == session
            && self.current_state() != PipelineState::Idle
    }

    fn is_recording_session(&self, session: u64) -> bool {
        !self.abort.load(Ordering::SeqCst)
            && self.session.load(Ordering::SeqCst) == session
            && self.current_state() == PipelineState::Recording
    }

    fn partial_snapshot(&self) -> String {
        self.partial_text
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn cancel_streaming_task(&self) {
        if let Some(task) = self
            .streaming_task
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
    }

    /// Give the audio of an interrupted segment back to the unread tail so the
    /// final pass transcribes it instead of silently dropping it.
    fn recover_inflight_segment(&self, buffer: &Arc<Mutex<RecorderBuffer>>) {
        let inflight = self
            .inflight_segment
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(length) = inflight {
            let mut buffer = buffer
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            buffer.cut = buffer.cut.saturating_sub(length);
            buffer.cut = buffer.cut.min(buffer.pcm.len());
        }
    }

    fn reset_volc_typed_text(&self) {
        self.volc_typed_text
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    fn volc_typed_snapshot(&self) -> String {
        self.volc_typed_text
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// Drop the native streaming provider owned by `session`.
    ///
    /// Called from synchronous teardown paths. Dropping the provider aborts its
    /// reader/writer tasks and closes the WebSocket, so `abort()` never waits.
    fn release_volc_provider(&self, session: u64) {
        match self.volc_provider.try_lock() {
            Ok(mut guard) => {
                if guard
                    .as_ref()
                    .is_some_and(|slot| slot.session == session)
                {
                    *guard = None;
                }
            }
            Err(_) => {
                let slot = self.volc_provider.clone();
                tauri::async_runtime::spawn(async move {
                    let mut guard = slot.lock().await;
                    if guard
                        .as_ref()
                        .is_some_and(|slot| slot.session == session)
                    {
                        *guard = None;
                    }
                });
            }
        }
    }

    fn stop_active_recording(&self) {
        let active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(mut active) = active {
            active.handle.stop();
        }
    }

    // ─── start ───

    pub async fn start(&self) -> Result<()> {
        let _guard = self.run_lock.lock().await;

        if self
            .state
            .compare_exchange(
                PipelineState::Idle.as_u8(),
                PipelineState::Preparing.as_u8(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            // Already preparing/recording/transcribing — ignore the re-entry.
            return Ok(());
        }

        self.abort.store(false, Ordering::SeqCst);
        let session = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        self.partial_text
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        *self
            .inflight_segment
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
        self.reset_volc_typed_text();
        self.set_state(PipelineState::Preparing);

        let config = match self.config() {
            Ok(config) => config,
            Err(error) => {
                self.emit_error("internal", Some(format!("读取配置失败：{error}")));
                self.finish_idle();
                return Err(error);
            }
        };

        let is_volcengine = config.stt_provider == STT_PROVIDER_VOLCENGINE;
        let volc_stt_config = if is_volcengine {
            if !config.stt_is_configured() {
                self.emit_error(
                    "stt_not_configured",
                    Some("请先在设置中选择火山引擎 Resource ID".to_string()),
                );
                self.finish_idle();
                return Ok(());
            }
            let api_key = match credentials::read_volcengine_secret(&SystemCredentialVault) {
                Ok(key) => key,
                Err(error) => {
                    self.emit_error("stt_auth", Some(format!("无法读取转写密钥：{error}")));
                    self.finish_idle();
                    return Ok(());
                }
            };
            if api_key.trim().is_empty() {
                self.emit_error(
                    "stt_not_configured",
                    Some("请先在设置中填写火山引擎 API Key".to_string()),
                );
                self.finish_idle();
                return Ok(());
            }
            Some(
                SttConfig::from_language(api_key, &config.stt_language)
                    .with_resource_id(&config.stt_volc_resource_id)
                    .with_hotwords(crate::dictionary::terms(&config.dictionary)),
            )
        } else {
            None
        };

        let (mut handle, receiver) = match AudioCaptureHandle::start(AudioConfig::default()) {
            Ok(result) => result,
            Err(error) => {
                tracing::error!("音频采集启动失败：{error}");
                self.emit_error(
                    "mic_unavailable",
                    Some(format!("无法打开麦克风：{error}")),
                );
                self.finish_idle();
                return Ok(());
            }
        };

        let ready_at = match tokio::time::timeout(STARTUP_TIMEOUT, handle.wait_until_ready()).await {
            Ok(Ok(ready_at)) => ready_at,
            Ok(Err(error)) => {
                tracing::error!("音频采集未就绪：{error}");
                handle.stop();
                self.emit_error("mic_unavailable", Some(format!("麦克风不可用：{error}")));
                self.finish_idle();
                return Ok(());
            }
            Err(_) => {
                handle.stop();
                self.emit_error(
                    "mic_unavailable",
                    Some("等待麦克风就绪超时，请检查系统录音权限".to_string()),
                );
                self.finish_idle();
                return Ok(());
            }
        };

        if self.abort.load(Ordering::SeqCst) {
            tracing::info!("录音在准备阶段被取消");
            handle.stop();
            self.finish_idle();
            return Ok(());
        }

        let buffer = Arc::new(Mutex::new(RecorderBuffer::default()));
        let (drained_tx, drained) = oneshot::channel();
        spawn_audio_collector(receiver, buffer.clone(), drained_tx);

        *self.active.lock().unwrap_or_else(|error| error.into_inner()) = Some(ActiveRecording {
            handle,
            buffer: buffer.clone(),
            drained,
            provider: config.stt_provider.clone(),
        });

        if let Some(stt_config) = volc_stt_config {
            let mut provider: Box<dyn SttProvider> = stt::create_volcengine_provider();
            let connected = {
                let connect_future =
                    tokio::time::timeout(VOLC_CONNECT_TIMEOUT, provider.connect(&stt_config));
                tokio::pin!(connect_future);
                // Poll the abort flag so cancelling during the handshake closes
                // the socket immediately instead of waiting for the timeout.
                loop {
                    tokio::select! {
                        result = &mut connect_future => break Some(result),
                        _ = tokio::time::sleep(VOLC_POLL_INTERVAL) => {
                            if self.abort.load(Ordering::SeqCst) {
                                break None;
                            }
                        }
                    }
                }
            };

            let Some(connected) = connected else {
                drop(provider);
                self.stop_active_recording();
                self.finish_idle();
                return Ok(());
            };

            match connected {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::error!("连接火山引擎失败：{}", stt_error_details(&error));
                    self.emit_error(stt_error_code(&error), Some(stt_error_details(&error)));
                    drop(provider);
                    self.stop_active_recording();
                    self.finish_idle();
                    return Ok(());
                }
                Err(_) => {
                    tracing::error!("连接火山引擎超时");
                    self.emit_error(
                        "stt_failed",
                        Some(format!(
                            "连接火山引擎超时（{} 秒）",
                            VOLC_CONNECT_TIMEOUT.as_secs()
                        )),
                    );
                    drop(provider);
                    self.stop_active_recording();
                    self.finish_idle();
                    return Ok(());
                }
            }

            if self.abort.load(Ordering::SeqCst) {
                drop(provider);
                self.stop_active_recording();
                self.finish_idle();
                return Ok(());
            }

            *self.volc_provider.lock().await = Some(VolcProviderSlot { session, provider });

            if self.abort.load(Ordering::SeqCst) {
                self.release_volc_provider(session);
                self.stop_active_recording();
                self.finish_idle();
                return Ok(());
            }
        }

        self.set_state(PipelineState::Recording);
        self.spawn_volume_task(session);
        self.spawn_deadline_task(session, ready_at, config.max_recording_seconds);
        if is_volcengine {
            self.spawn_volcengine_task(session, config, buffer);
        } else {
            self.spawn_streaming_task(session, config, buffer);
        }

        if self.abort.load(Ordering::SeqCst) {
            self.cancel_streaming_task();
            self.release_volc_provider(session);
            self.stop_active_recording();
            self.finish_idle();
        }

        Ok(())
    }

    // ─── stop ───

    pub async fn stop(&self) -> Result<()> {
        let _guard = self.run_lock.lock().await;

        if self.current_state() != PipelineState::Recording {
            return Ok(());
        }

        let session = self.session.load(Ordering::SeqCst);
        self.cancel_streaming_task();

        let active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        let Some(mut active) = active else {
            self.release_volc_provider(session);
            self.finish_idle();
            return Ok(());
        };
        let recording_provider = active.provider.clone();

        active.handle.stop();
        // Let the collector drain whatever cpal already produced.
        let _ = tokio::time::timeout(COLLECTOR_DRAIN_TIMEOUT, &mut active.drained).await;
        let buffer = active.buffer.clone();

        if self.abort.load(Ordering::SeqCst) {
            self.release_volc_provider(session);
            self.finish_idle();
            return Ok(());
        }

        let config = match self.config() {
            Ok(config) => config,
            Err(error) => {
                self.emit_error("internal", Some(format!("读取配置失败：{error}")));
                self.release_volc_provider(session);
                self.finish_idle();
                return Ok(());
            }
        };

        let live_insert = config.streaming_enabled && config.streaming_insert;
        self.set_state(PipelineState::Transcribing);

        let draft = if recording_provider == STT_PROVIDER_VOLCENGINE {
            self.finalize_volcengine(&config, &buffer, live_insert).await
        } else {
            self.finalize(&config, &buffer, live_insert).await
        };
        let Some(draft) = draft else {
            self.finish_idle();
            return Ok(());
        };

        if !self.is_active_session(session) {
            self.finish_idle();
            return Ok(());
        }

        let _ = self.app.emit("stt:final", draft.text.clone());

        let insert_text = if draft.already_typed {
            draft.pending_insert
        } else {
            Some(self.polish_or_raw(&config, &draft.text).await)
        };

        if !self.is_active_session(session) {
            self.finish_idle();
            return Ok(());
        }

        if let Some(text) = insert_text.filter(|text| !text.trim().is_empty()) {
            self.set_state(PipelineState::Outputting);
            self.insert_text(&text, &config).await;
        }

        self.finish_idle();
        Ok(())
    }

    /// Discard the current recording (or in-flight processing) immediately.
    pub fn abort(&self) {
        tracing::info!("请求中止听写（当前状态 {:?}）", self.current_state());
        let session = self.session.load(Ordering::SeqCst);
        self.abort.store(true, Ordering::SeqCst);
        self.session.fetch_add(1, Ordering::SeqCst);
        self.cancel_streaming_task();
        self.release_volc_provider(session);
        *self
            .inflight_segment
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;

        let active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(mut active) = active {
            active.handle.stop();
        }

        self.finish_idle();
    }

    // ─── transcription ───

    async fn finalize(
        &self,
        config: &AppConfig,
        buffer: &Arc<Mutex<RecorderBuffer>>,
        live_insert: bool,
    ) -> Option<FinalDraft> {
        if live_insert {
            self.recover_inflight_segment(buffer);
            let previous = self.partial_snapshot();
            let tail = buffer
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take_tail();

            let tail_text = if tail.len() >= bytes_for_ms(MIN_TAIL_MS) {
                match self
                    .transcribe_segment(config, &tail, Some(&previous))
                    .await
                {
                    Ok(text) => text,
                    Err(error) => {
                        tracing::warn!("尾段补转写失败：{}", stt_error_details(&error));
                        None
                    }
                }
            } else {
                None
            };

            let (merged, appended) = match tail_text.as_deref() {
                Some(text) => merge_partial(&previous, text),
                None => (previous, String::new()),
            };

            if merged.trim().is_empty() {
                self.emit_error("no_speech", None);
                return None;
            }

            return Some(FinalDraft {
                text: merged,
                pending_insert: (!appended.trim().is_empty()).then_some(appended),
                already_typed: true,
            });
        }

        let pcm = buffer
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .whole();

        if pcm.is_empty() {
            self.emit_error("no_speech", None);
            return None;
        }

        let text = self.transcribe_full(config, &pcm).await?;

        Some(FinalDraft {
            text,
            pending_insert: None,
            already_typed: false,
        })
    }

    /// Finalize the native streaming path: feed the remaining audio, send the
    /// end frame, wait for the server's final transcript and (when live
    /// insertion is enabled) only type the suffix that is still missing.
    async fn finalize_volcengine(
        &self,
        _config: &AppConfig,
        buffer: &Arc<Mutex<RecorderBuffer>>,
        live_insert: bool,
    ) -> Option<FinalDraft> {
        self.recover_inflight_segment(buffer);
        let tail = buffer
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take_tail();

        let provider = {
            let mut guard = self.volc_provider.lock().await;
            guard.take().map(|slot| slot.provider)
        };
        let Some(mut provider) = provider else {
            return self.volc_draft(self.partial_snapshot(), live_insert);
        };

        if !tail.is_empty() {
            for part in tail.chunks(VOLC_MAX_FRAME_BYTES) {
                if let Err(error) = provider.send_audio(part).await {
                    let code = stt_error_code(&error);
                    self.emit_error(code, Some(stt_error_details(&error)));
                    return None;
                }
            }
        }

        let disconnect_future = provider.disconnect();
        tokio::pin!(disconnect_future);
        let deadline = tokio::time::Instant::now() + VOLC_FINAL_TIMEOUT;
        let outcome = loop {
            tokio::select! {
                result = &mut disconnect_future => break Some(result),
                _ = tokio::time::sleep(VOLC_POLL_INTERVAL) => {
                    if self.abort.load(Ordering::SeqCst) {
                        return None;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        break None;
                    }
                }
            }
        };

        let transcript = match outcome {
            Some(Ok(transcript)) => transcript,
            Some(Err(error)) => {
                let code = stt_error_code(&error);
                self.emit_error(code, Some(stt_error_details(&error)));
                return None;
            }
            None => {
                let partial = self.partial_snapshot();
                if partial.trim().is_empty() {
                    self.emit_error(
                        "stt_failed",
                        Some(format!(
                            "等待火山引擎最终识别结果超时（{} 秒）",
                            VOLC_FINAL_TIMEOUT.as_secs()
                        )),
                    );
                    return None;
                }
                return self.volc_draft(partial, live_insert);
            }
        };

        let final_text = transcript
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| self.partial_snapshot());

        self.volc_draft(final_text, live_insert)
    }

    fn volc_draft(&self, final_text: String, live_insert: bool) -> Option<FinalDraft> {
        let final_text = final_text.trim().to_string();
        if final_text.is_empty() {
            self.emit_error("no_speech", None);
            return None;
        }

        let typed = self.volc_typed_snapshot();
        // Trust what was actually typed at the cursor: if the user toggled
        // streaming_insert mid-recording, the typed prefix still must not be
        // inserted a second time.
        let already_typed = live_insert || !typed.trim().is_empty();
        if !already_typed {
            return Some(FinalDraft {
                text: final_text,
                pending_insert: None,
                already_typed: false,
            });
        }

        let appended = append_after(&typed, &final_text);
        Some(FinalDraft {
            text: final_text,
            pending_insert: (!appended.trim().is_empty()).then_some(appended),
            already_typed: true,
        })
    }

    async fn transcribe_full(&self, config: &AppConfig, pcm: &[u8]) -> Option<String> {
        if !config.stt_is_configured() {
            self.emit_error(
                "stt_not_configured",
                Some("请先在设置中填写转写接口地址与模型".to_string()),
            );
            return None;
        }

        let api_key = match credentials::read_stt_secret(&SystemCredentialVault) {
            Ok(key) => key,
            Err(error) => {
                self.emit_error("stt_auth", Some(format!("无法读取转写密钥：{error}")));
                return None;
            }
        };

        let provider_config = match build_custom_whisper_config(
            &config.stt_base_url,
            &config.stt_model,
            &config.dictionary,
        ) {
            Ok(provider_config) => provider_config,
            Err(message) => {
                self.emit_error("stt_not_configured", Some(message));
                return None;
            }
        };

        let stt_config = SttConfig::from_language(api_key, &config.stt_language);
        let mut provider = match stt::create_provider(
            CUSTOM_WHISPER_PROVIDER,
            provider_config,
            Some(self.client.clone()),
        ) {
            Ok(provider) => provider,
            Err(error) => {
                self.emit_error("stt_failed", Some(stt_error_details(&error)));
                return None;
            }
        };

        if let Err(error) = provider.connect(&stt_config).await {
            self.emit_error(stt_error_code(&error), Some(stt_error_details(&error)));
            return None;
        }
        if let Err(error) = provider.send_audio(pcm).await {
            self.emit_error("stt_failed", Some(stt_error_details(&error)));
            return None;
        }

        let transcript = match tokio::time::timeout(FINALIZE_TIMEOUT, provider.disconnect()).await {
            Ok(Ok(transcript)) => transcript,
            Ok(Err(error)) => {
                self.emit_error(stt_error_code(&error), Some(stt_error_details(&error)));
                return None;
            }
            Err(_) => {
                self.emit_error("stt_failed", Some("转写请求超时".to_string()));
                return None;
            }
        };

        match transcript.map(|text| text.trim().to_string()) {
            Some(text) if !text.is_empty() => Some(text),
            _ => {
                self.emit_error("no_speech", None);
                None
            }
        }
    }

    async fn transcribe_segment(
        &self,
        config: &AppConfig,
        pcm: &[u8],
        context: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let api_key = credentials::read_stt_secret(&SystemCredentialVault).map_err(|error| {
            AppError::Auth(format!("无法读取转写密钥：{error}"))
        })?;
        let provider_config =
            build_custom_whisper_config(&config.stt_base_url, &config.stt_model, &config.dictionary)
                .map_err(AppError::Config)?;
        let stt_config = SttConfig::from_language(api_key, &config.stt_language);
        let provider =
            WhisperCompatProvider::with_client(provider_config, self.client.clone());
        provider.transcribe_pcm(pcm, &stt_config, context).await
    }

    // ─── polishing ───

    async fn polish_or_raw(&self, config: &AppConfig, raw_text: &str) -> String {
        if !config.polish_enabled {
            return raw_text.to_string();
        }
        if !config.llm_is_configured() {
            self.emit_error(
                "llm_not_configured",
                Some("已开启润色，但对话接口地址或模型为空，已直接输出原始转写".to_string()),
            );
            return raw_text.to_string();
        }

        self.set_state(PipelineState::Polishing);

        let api_key = match credentials::read_llm_secret(&SystemCredentialVault) {
            Ok(key) => key,
            Err(error) => {
                self.emit_error(
                    "llm_failed",
                    Some(format!("无法读取润色密钥：{error}，已直接输出原始转写")),
                );
                return raw_text.to_string();
            }
        };

        let llm_config = LlmConfig {
            disable_thinking: config.llm_disable_thinking,
            ..LlmConfig::new(
                config.llm_base_url.clone(),
                config.llm_model.clone(),
                api_key,
            )
        };
        let request = PolishRequest {
            raw_text: raw_text.to_string(),
            dictionary: config.dictionary.clone(),
            polish_style: config.polish_style.clone(),
            custom_prompt: config.polish_custom_prompt.clone(),
        };

        let provider = llm::create_provider(Some(self.client.clone()));
        let app = self.app.clone();
        let callback: llm::ChunkCallback = Box::new(move |chunk: &str| {
            let _ = app.emit("llm:chunk", chunk.to_string());
        });

        match provider.polish(&llm_config, &request, Some(&callback)).await {
            Ok(response) if !response.polished_text.trim().is_empty() => {
                response.polished_text.trim().to_string()
            }
            Ok(_) => {
                self.emit_error(
                    "llm_failed",
                    Some("润色接口没有返回任何文本，已输出原始转写".to_string()),
                );
                raw_text.to_string()
            }
            Err(error) => {
                self.emit_error(
                    "llm_failed",
                    Some(format!("{}，已输出原始转写", llm_error_details(&error))),
                );
                raw_text.to_string()
            }
        }
    }

    // ─── output ───

    async fn insert_text(&self, text: &str, config: &AppConfig) {
        let use_keyboard = config.output_mode != OUTPUT_MODE_CLIPBOARD;

        if use_keyboard && !is_accessibility_trusted() {
            self.emit_error("accessibility_required", None);
            let options = clipboard_options(config, false);
            if let Ok(outcome) = output::output_with_fallback(
                &self.app,
                text,
                OutputMode::Clipboard,
                options,
            )
            .await
            {
                let _ = self
                    .app
                    .emit("pipeline:insert_result", outcome.insert_result);
            }
            return;
        }

        let mode = if use_keyboard {
            OutputMode::Keyboard
        } else {
            OutputMode::Clipboard
        };
        let options = clipboard_options(config, true);

        match output::output_with_fallback(&self.app, text, mode, options).await {
            Ok(outcome) => {
                let _ = self
                    .app
                    .emit("pipeline:insert_result", outcome.insert_result);
                if let Some(warning) = outcome.warning {
                    tracing::warn!("文本输出警告：{}", warning.code);
                }
            }
            Err(error) => {
                if error.contains("ACCESSIBILITY_REQUIRED") {
                    self.emit_error("accessibility_required", None);
                } else {
                    self.emit_error("output_failed", Some(error));
                }
            }
        }
    }

    // ─── background tasks ───

    fn spawn_volume_task(&self, session: u64) {
        let handle = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(VOLUME_EMIT_INTERVAL).await;
                if !handle.is_recording_session(session) {
                    break;
                }
                let level = {
                    let guard = handle
                        .active
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    guard
                        .as_ref()
                        .map(|active| active.handle.get_volume())
                        .unwrap_or(0.0)
                };
                let _ = handle.app.emit("audio:volume", level.clamp(0.0, 1.0));
            }
        });
    }

    fn spawn_deadline_task(&self, session: u64, ready_at: CaptureReadyAt, max_seconds: u32) {
        let deadline = RecordingDeadline::new(
            session,
            RecordingKind::Dictation,
            ready_at,
            max_seconds.max(1),
        );
        let payload = RecordingDeadlinePayload {
            session_id: deadline.event.session_id,
            started_at_unix_ms: deadline.event.started_at_unix_ms,
            deadline_at_unix_ms: deadline.event.deadline_at_unix_ms,
            effective_max_seconds: deadline.event.effective_max_seconds,
        };
        let _ = self.app.emit("recording:deadline", payload);

        let handle = self.clone();
        tauri::async_runtime::spawn(async move {
            let active = handle.clone();
            let reached = drive_recording_deadline(
                deadline,
                move || active.is_recording_session(session),
                |_| {},
            )
            .await;
            if reached {
                tracing::info!("录音达到时长上限，自动停止");
                let _ = handle.stop().await;
            }
        });
    }

    fn spawn_streaming_task(
        &self,
        session: u64,
        config: AppConfig,
        buffer: Arc<Mutex<RecorderBuffer>>,
    ) {
        if !config.streaming_enabled || !config.stt_is_configured() {
            return;
        }

        let handle = self.clone();
        let interval = Duration::from_millis(u64::from(config.streaming_interval_ms));
        let min_segment_bytes = bytes_for_ms(MIN_SEGMENT_MS);
        let insert_during_recording = config.streaming_insert;
        let partial = self.partial_text.clone();

        let task = tauri::async_runtime::spawn(async move {
            let mut confirmed = String::new();
            loop {
                tokio::time::sleep(interval).await;
                if !handle.is_recording_session(session) {
                    break;
                }

                let segment = {
                    let mut guard = buffer
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    guard.take_segment_if_ready(min_segment_bytes)
                };
                let Some(segment) = segment else {
                    continue;
                };

                *handle
                    .inflight_segment
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(segment.len());

                let context = tail_chars(&confirmed, STREAMING_CONTEXT_CHARS);
                let transcription = handle
                    .transcribe_segment(&config, &segment, Some(&context))
                    .await;

                match transcription {
                    Ok(Some(text)) => {
                        let (merged, appended) = merge_partial(&confirmed, &text);
                        if merged != confirmed {
                            confirmed = merged.clone();
                            *partial.lock().unwrap_or_else(|error| error.into_inner()) =
                                merged.clone();
                            handle.emit_partial(&merged);

                            if insert_during_recording && !appended.trim().is_empty() {
                                handle.insert_text(appended.trim(), &config).await;
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(
                            "分段转写失败（继续录音）：{}",
                            stt_error_details(&error)
                        );
                    }
                }

                *handle
                    .inflight_segment
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
            }
        });

        *self
            .streaming_task
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(task);
    }

    /// Native streaming loop: every ~30 ms ship the new PCM and drain the
    /// provider's transcript channel. `send_audio` / `recv_transcript` never
    /// await network I/O, so upload and download genuinely overlap.
    fn spawn_volcengine_task(
        &self,
        session: u64,
        config: AppConfig,
        buffer: Arc<Mutex<RecorderBuffer>>,
    ) {
        let handle = self.clone();
        let provider_slot = self.volc_provider.clone();
        let partial = self.partial_text.clone();
        let typed_text = self.volc_typed_text.clone();
        let emit_partial = config.streaming_enabled;
        let insert_during_recording = config.streaming_enabled && config.streaming_insert;

        let task = tauri::async_runtime::spawn(async move {
            let mut confirmed = String::new();

            'recording: loop {
                tokio::time::sleep(VOLC_POLL_INTERVAL).await;
                if !handle.is_recording_session(session) {
                    break;
                }

                let chunk = {
                    let mut guard = buffer
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    guard.take_tail_limited(VOLC_MAX_FRAME_BYTES)
                };

                // Tag the in-flight bytes before any await so `stop()` can hand
                // them back to the tail if this task is cancelled mid-send.
                if !chunk.is_empty() {
                    *handle
                        .inflight_segment
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(chunk.len());
                }

                let mut events = Vec::new();
                let mut failure: Option<AppError> = None;
                let mut slot_missing = false;

                {
                    let mut guard = provider_slot.lock().await;
                    if let Some(slot) = guard.as_mut() {
                        if slot.session == session {
                            if !chunk.is_empty() {
                                let result = slot.provider.send_audio(&chunk).await;
                                *handle
                                    .inflight_segment
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner()) = None;
                                if let Err(error) = result {
                                    failure = Some(error);
                                }
                            }

                            if failure.is_none() {
                                for _ in 0..64 {
                                    match slot.provider.recv_transcript().await {
                                        Ok(Some(event)) => events.push(event),
                                        Ok(None) => break,
                                        Err(error) => {
                                            failure = Some(error);
                                            break;
                                        }
                                    }
                                }
                            }
                        } else {
                            slot_missing = true;
                        }
                    } else {
                        slot_missing = true;
                    }

                    if slot_missing {
                        *handle
                            .inflight_segment
                            .lock()
                            .unwrap_or_else(|error| error.into_inner()) = None;
                    }
                }

                if slot_missing {
                    break;
                }

                if let Some(error) = failure {
                    let code = stt_error_code(&error);
                    handle.emit_error(code, Some(stt_error_details(&error)));
                    handle.abort();
                    break;
                }

                for event in events {
                    match event {
                        TranscriptEvent::Partial { text } => {
                            let text = text.trim().to_string();
                            if text.is_empty() || text == confirmed {
                                continue;
                            }
                            confirmed = text.clone();
                            *partial
                                .lock()
                                .unwrap_or_else(|error| error.into_inner()) = text.clone();
                            if emit_partial {
                                handle.emit_partial(&text);
                            }
                            if insert_during_recording {
                                let typed = typed_text
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner())
                                    .clone();
                                let appended = append_after(&typed, &text);
                                if !appended.trim().is_empty() {
                                    // Record what will be typed before awaiting:
                                    // if `stop()` cancels this task mid-insert,
                                    // the finalizer must not type it twice.
                                    *typed_text
                                        .lock()
                                        .unwrap_or_else(|error| error.into_inner()) =
                                        format!("{}{appended}", typed.trim_end());
                                    handle.insert_text(appended.trim(), &config).await;
                                }
                            }
                        }
                        TranscriptEvent::Final { text, .. } => {
                            // A final result updates the accumulated transcript
                            // but is not re-emitted as a partial.
                            let text = text.trim().to_string();
                            if text.is_empty() || text == confirmed {
                                continue;
                            }
                            confirmed = text.clone();
                            *partial
                                .lock()
                                .unwrap_or_else(|error| error.into_inner()) = text.clone();
                            if insert_during_recording {
                                let typed = typed_text
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner())
                                    .clone();
                                let appended = append_after(&typed, &text);
                                if !appended.trim().is_empty() {
                                    *typed_text
                                        .lock()
                                        .unwrap_or_else(|error| error.into_inner()) =
                                        format!("{}{appended}", typed.trim_end());
                                    handle.insert_text(appended.trim(), &config).await;
                                }
                            }
                        }
                        TranscriptEvent::Error { message } => {
                            let code = if volcengine::is_authentication_error(&message) {
                                "stt_auth"
                            } else {
                                "stt_failed"
                            };
                            handle.emit_error(code, Some(message));
                            handle.abort();
                            break 'recording;
                        }
                    }
                }
            }
        });

        *self
            .streaming_task
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(task);
    }
}

fn spawn_audio_collector(
    mut receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
    buffer: Arc<Mutex<RecorderBuffer>>,
    drained: oneshot::Sender<()>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(chunk) = receiver.recv().await {
            buffer
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .append(&chunk);
        }
        let _ = drained.send(());
    });
}

/// Convert milliseconds of 16 kHz 16-bit mono audio to bytes.
pub fn bytes_for_ms(milliseconds: u64) -> usize {
    (AUDIO_BYTES_PER_SECOND as u64 * milliseconds / 1000) as usize
}

fn clipboard_options(config: &AppConfig, auto_paste: bool) -> ClipboardOutputOptions {
    ClipboardOutputOptions {
        restore_after_paste: config.restore_clipboard_after_paste,
        paste_shortcut: PasteShortcut::from_config_value(&config.paste_shortcut),
        auto_paste,
    }
}

fn stt_error_code(error: &AppError) -> &'static str {
    match error {
        AppError::Auth(_) => "stt_auth",
        AppError::Api { status, .. } if *status == 401 || *status == 403 => "stt_auth",
        _ => "stt_failed",
    }
}

pub(crate) fn stt_error_details(error: &AppError) -> String {
    match error {
        AppError::Api { status, body } if body.trim().is_empty() => {
            format!("转写接口返回 {status}")
        }
        AppError::Api { status, body } => format!("转写接口返回 {status}：{body}"),
        AppError::Auth(message) if message.starts_with("火山引擎") => message.clone(),
        AppError::Auth(message) => format!("转写接口鉴权失败：{message}"),
        AppError::Timeout(duration) => {
            format!("转写请求超时（{} 秒）", duration.as_secs().max(1))
        }
        AppError::Network(message) if message.starts_with("火山引擎") => message.clone(),
        AppError::Network(message) => format!("无法连接转写接口：{message}"),
        AppError::Config(message) => message.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn llm_error_details(error: &AppError) -> String {
    match error {
        AppError::Api { status, body } if body.trim().is_empty() => {
            format!("润色接口返回 {status}")
        }
        AppError::Api { status, body } => format!("润色接口返回 {status}：{body}"),
        AppError::Timeout(_) => LLM_TIMEOUT_NOTE.to_string(),
        AppError::Network(message) => format!("无法连接润色接口：{message}"),
        AppError::Config(message) => message.clone(),
        other => other.to_string(),
    }
}

/// Harness for the native streaming live-insert path: the server returns the
/// full transcript, so type only the suffix that is not at the cursor yet.
pub fn append_after(previous: &str, full_text: &str) -> String {
    let previous = previous.trim_end();
    let full_text = full_text.trim();

    if previous.is_empty() {
        return full_text.to_string();
    }
    if let Some(rest) = full_text.strip_prefix(previous) {
        return rest.to_string();
    }
    merge_partial(previous, full_text).1
}

/// Merge a freshly transcribed segment into the confirmed text.
///
/// Returns `(merged_full_text, appended_suffix)`. The longest suffix of
/// `previous` that is also a prefix of `segment` is treated as overlap and
/// stripped, which is what Whisper tends to repeat between consecutive windows.
pub fn merge_partial(previous: &str, segment: &str) -> (String, String) {
    let previous = previous.trim_end();
    let segment = segment.trim();

    if segment.is_empty() {
        return (previous.to_string(), String::new());
    }
    if previous.is_empty() {
        return (segment.to_string(), segment.to_string());
    }

    let previous_chars: Vec<char> = previous.chars().collect();
    let segment_chars: Vec<char> = segment.chars().collect();
    let max_overlap = previous_chars.len().min(segment_chars.len());

    let mut overlap = 0;
    for candidate in (1..=max_overlap).rev() {
        if previous_chars[previous_chars.len() - candidate..] == segment_chars[..candidate] {
            overlap = candidate;
            break;
        }
    }

    let appended: String = segment_chars[overlap..].iter().collect();
    let mut merged = String::with_capacity(previous.len() + appended.len());
    merged.push_str(previous);
    merged.push_str(&appended);

    (merged, appended)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_for_ms_matches_16khz_mono_pcm() {
        assert_eq!(bytes_for_ms(1000), 32_000);
        assert_eq!(bytes_for_ms(MIN_SEGMENT_MS), 12_800);
    }

    #[test]
    fn merge_partial_strips_the_repeated_overlap() {
        let (merged, appended) = merge_partial("今天我们来讨论接口", "接口的字段设计");
        assert_eq!(merged, "今天我们来讨论接口的字段设计");
        assert_eq!(appended, "的字段设计");
    }

    #[test]
    fn merge_partial_handles_fully_repeated_segments() {
        let (merged, appended) = merge_partial("你好世界", "你好世界");
        assert_eq!(merged, "你好世界");
        assert_eq!(appended, "");
    }

    #[test]
    fn merge_partial_appends_unrelated_text() {
        let (merged, appended) = merge_partial("第一段。", "第二段。");
        assert_eq!(merged, "第一段。第二段。");
        assert_eq!(appended, "第二段。");
    }

    #[test]
    fn merge_partial_handles_empty_sides() {
        assert_eq!(merge_partial("", "新内容").0, "新内容");
        assert_eq!(merge_partial("", "新内容").1, "新内容");
        assert_eq!(merge_partial("已确认", "   ").0, "已确认");
        assert_eq!(merge_partial("已确认", "   ").1, "");
    }

    #[test]
    fn append_after_types_only_the_new_suffix_of_a_full_transcript() {
        assert_eq!(append_after("", "你好世界"), "你好世界");
        assert_eq!(append_after("你好", "你好世界"), "世界");
        assert_eq!(append_after("你好世界", "你好世界"), "");
        assert_eq!(append_after("  你好  ", "  你好世界  "), "世界");
        assert_eq!(append_after("第一段。", "第二段。"), "第二段。");
    }

    #[test]
    fn recorder_buffer_moves_ownership_at_the_cut_point() {
        let mut buffer = RecorderBuffer::default();
        buffer.append(&[1, 2, 3, 4]);
        assert_eq!(buffer.take_segment_if_ready(10), None);
        assert_eq!(buffer.take_segment_if_ready(4), Some(vec![1, 2, 3, 4]));
        assert_eq!(buffer.take_segment_if_ready(1), None);

        buffer.append(&[5, 6]);
        assert_eq!(buffer.take_tail(), vec![5, 6]);
        assert_eq!(buffer.whole(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn recorder_buffer_can_hand_back_a_limited_streaming_chunk() {
        let mut buffer = RecorderBuffer::default();
        buffer.append(&[1, 2, 3, 4, 5]);

        assert_eq!(buffer.take_tail_limited(2), vec![1, 2]);
        assert_eq!(buffer.take_tail_limited(10), vec![3, 4, 5]);
        assert_eq!(buffer.take_tail_limited(10), Vec::<u8>::new());
        assert_eq!(buffer.take_tail(), Vec::<u8>::new());
    }

    #[test]
    fn pipeline_states_serialize_as_contract_strings() {
        fn state_json(state: PipelineState) -> String {
            serde_json::to_value(state)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default()
        }

        assert_eq!(state_json(PipelineState::Idle), "idle");
        assert_eq!(state_json(PipelineState::Preparing), "preparing");
        assert_eq!(state_json(PipelineState::Recording), "recording");
        assert_eq!(state_json(PipelineState::Transcribing), "transcribing");
        assert_eq!(state_json(PipelineState::Polishing), "polishing");
        assert_eq!(state_json(PipelineState::Outputting), "outputting");
    }

    #[test]
    fn deadline_payload_uses_camel_case_keys() {
        let payload = RecordingDeadlinePayload {
            session_id: 1,
            started_at_unix_ms: 2,
            deadline_at_unix_ms: 3,
            effective_max_seconds: 30,
        };
        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["sessionId"], 1);
        assert_eq!(value["startedAtUnixMs"], 2);
        assert_eq!(value["deadlineAtUnixMs"], 3);
        assert_eq!(value["effectiveMaxSeconds"], 30);
    }

    #[test]
    fn pipeline_error_payload_omits_missing_details() {
        let payload = PipelineErrorPayload {
            code: "no_speech".to_string(),
            details: None,
        };
        let value = serde_json::to_value(payload).unwrap();
        assert_eq!(value["code"], "no_speech");
        assert!(value.get("details").is_none());
    }

    #[test]
    fn stt_errors_map_to_readable_codes_and_messages() {
        let auth = AppError::Api {
            status: 401,
            body: "invalid key".to_string(),
        };
        assert_eq!(stt_error_code(&auth), "stt_auth");
        assert!(stt_error_details(&auth).contains("401"));

        let failed = AppError::Api {
            status: 500,
            body: String::new(),
        };
        assert_eq!(stt_error_code(&failed), "stt_failed");
        assert!(stt_error_details(&failed).contains("转写接口返回 500"));

        let network = AppError::Network("connection refused".to_string());
        assert!(stt_error_details(&network).contains("无法连接转写接口"));
    }

    #[test]
    fn llm_errors_are_described_in_chinese() {
        let error = AppError::Api {
            status: 429,
            body: "rate limited".to_string(),
        };
        let details = llm_error_details(&error);
        assert!(details.contains("润色接口返回 429"));
        assert!(details.contains("rate limited"));

        let timeout = AppError::Timeout(Duration::from_secs(30));
        assert_eq!(llm_error_details(&timeout), LLM_TIMEOUT_NOTE);
    }
}
