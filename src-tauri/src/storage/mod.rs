//! Settings persistence for the trimmed-down dictation app.
//!
//! The whole feature set is described by [`AppConfig`]. It is stored as a
//! single JSON object under the `app_config` key of `settings.json` through
//! `tauri-plugin-store`, keeping an in-memory cache so hot paths (hotkey
//! handler, pipeline) never hit the disk twice for the same value.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri_plugin_store::StoreExt;

/// File name handed to `app.store(...)`.
pub const STORE_FILE: &str = "settings.json";
/// Key inside the store that holds the whole configuration object.
pub const CONFIG_KEY: &str = "app_config";

pub const MIN_RECORDING_SECONDS: u32 = 10;
pub const MAX_RECORDING_SECONDS: u32 = 600;
pub const MIN_STREAMING_INTERVAL_MS: u32 = 800;
pub const MAX_STREAMING_INTERVAL_MS: u32 = 5000;
pub const DEFAULT_STREAMING_INTERVAL_MS: u32 = 1200;
pub const MAX_DICTIONARY_ENTRIES: usize = 500;
pub const MAX_DICTIONARY_ENTRY_CHARS: usize = 64;

pub const HOTKEY_MODE_HOLD: &str = "hold";
pub const HOTKEY_MODE_TOGGLE: &str = "toggle";

pub const OUTPUT_MODE_KEYBOARD: &str = "keyboard";
pub const OUTPUT_MODE_CLIPBOARD: &str = "clipboard";

pub const POLISH_STYLE_MINIMAL: &str = "minimal";
pub const POLISH_STYLE_CLEAN: &str = "clean";
pub const POLISH_STYLE_STRUCTURED: &str = "structured";
pub const POLISH_STYLE_CUSTOM: &str = "custom";

pub const THEME_LIGHT: &str = "light";
pub const THEME_DARK: &str = "dark";
pub const THEME_SYSTEM: &str = "system";

pub const UI_LANGUAGE_ZH: &str = "zh";

/// STT provider selection. `openai` is the OpenAI-compatible batch provider,
/// `volcengine` is the native WebSocket streaming provider.
pub const STT_PROVIDER_OPENAI: &str = "openai";
pub const STT_PROVIDER_VOLCENGINE: &str = "volcengine";

/// Resource IDs accepted by Volcengine Speech (Doubao / SeedASR) streaming ASR.
pub const STT_VOLC_RESOURCE_SEEDASR: &str = "volc.seedasr.sauc.duration";
pub const STT_VOLC_RESOURCE_BIGASR: &str = "volc.bigasr.sauc.duration";

fn default_hotkey() -> String {
    "Ctrl+/".to_string()
}

fn default_hotkey_mode() -> String {
    HOTKEY_MODE_HOLD.to_string()
}

fn default_max_recording_seconds() -> u32 {
    30
}

fn default_stt_provider() -> String {
    STT_PROVIDER_OPENAI.to_string()
}

fn default_stt_volc_resource_id() -> String {
    STT_VOLC_RESOURCE_SEEDASR.to_string()
}

fn default_stt_model() -> String {
    "whisper-1".to_string()
}

fn default_stt_language() -> String {
    "auto".to_string()
}

fn default_polish_enabled() -> bool {
    true
}

/// 默认指向本机 Ollama 的 OpenAI 兼容端点；用户可以在设置里改成任意云端服务。
fn default_llm_base_url() -> String {
    "http://127.0.0.1:11434/v1".to_string()
}

fn default_llm_model() -> String {
    "qwen3:1.7b".to_string()
}

fn default_llm_disable_thinking() -> bool {
    true
}

fn default_polish_style() -> String {
    POLISH_STYLE_CLEAN.to_string()
}

fn default_output_mode() -> String {
    OUTPUT_MODE_KEYBOARD.to_string()
}

/// macOS pastes with Command+V; every other platform uses Control+V.
fn default_paste_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "cmdV".to_string()
    } else {
        "ctrlV".to_string()
    }
}

fn default_streaming_enabled() -> bool {
    true
}

fn default_streaming_insert() -> bool {
    false
}

fn default_streaming_interval_ms() -> u32 {
    DEFAULT_STREAMING_INTERVAL_MS
}

fn default_restore_clipboard_after_paste() -> bool {
    true
}

fn default_theme() -> String {
    THEME_SYSTEM.to_string()
}

fn default_auto_start() -> bool {
    true
}

fn default_close_to_tray() -> bool {
    true
}

fn default_start_minimized() -> bool {
    false
}

fn default_ui_language() -> String {
    UI_LANGUAGE_ZH.to_string()
}

fn default_capsule_auto_hide() -> bool {
    true
}

/// The complete, persisted user configuration.
///
/// Field names, types and defaults mirror `INTERFACE.md` exactly. Unknown
/// fields are ignored on read and missing fields fall back to their default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_hotkey_mode")]
    pub hotkey_mode: String,
    #[serde(default = "default_max_recording_seconds")]
    pub max_recording_seconds: u32,

    #[serde(default)]
    pub stt_base_url: String,
    #[serde(default = "default_stt_provider")]
    pub stt_provider: String,
    #[serde(default = "default_stt_volc_resource_id")]
    pub stt_volc_resource_id: String,
    #[serde(default = "default_stt_model")]
    pub stt_model: String,
    #[serde(default = "default_stt_language")]
    pub stt_language: String,

    #[serde(default = "default_polish_enabled")]
    pub polish_enabled: bool,
    #[serde(default = "default_llm_base_url")]
    pub llm_base_url: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
    #[serde(default = "default_polish_style")]
    pub polish_style: String,
    #[serde(default)]
    pub polish_custom_prompt: String,
    /// 关闭推理模型的思考阶段（发送 reasoning_effort=none）。
    /// 本地 qwen3 这类推理模型开着思考时单次润色要 8 秒以上，关掉后约 0.15 秒。
    #[serde(default = "default_llm_disable_thinking")]
    pub llm_disable_thinking: bool,

    #[serde(default)]
    pub dictionary: Vec<String>,

    #[serde(default = "default_streaming_enabled")]
    pub streaming_enabled: bool,
    #[serde(default = "default_streaming_insert")]
    pub streaming_insert: bool,
    #[serde(default = "default_streaming_interval_ms")]
    pub streaming_interval_ms: u32,

    #[serde(default = "default_output_mode")]
    pub output_mode: String,
    #[serde(default = "default_paste_shortcut")]
    pub paste_shortcut: String,
    #[serde(default = "default_restore_clipboard_after_paste")]
    pub restore_clipboard_after_paste: bool,

    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    #[serde(default = "default_start_minimized")]
    pub start_minimized: bool,
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
    #[serde(default = "default_capsule_auto_hide")]
    pub capsule_auto_hide: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            hotkey_mode: default_hotkey_mode(),
            max_recording_seconds: default_max_recording_seconds(),
            stt_base_url: String::new(),
            stt_provider: default_stt_provider(),
            stt_volc_resource_id: default_stt_volc_resource_id(),
            stt_model: default_stt_model(),
            stt_language: default_stt_language(),
            polish_enabled: default_polish_enabled(),
            llm_base_url: default_llm_base_url(),
            llm_model: default_llm_model(),
            polish_style: default_polish_style(),
            polish_custom_prompt: String::new(),
            llm_disable_thinking: default_llm_disable_thinking(),
            dictionary: Vec::new(),
            streaming_enabled: default_streaming_enabled(),
            streaming_insert: default_streaming_insert(),
            streaming_interval_ms: default_streaming_interval_ms(),
            output_mode: default_output_mode(),
            paste_shortcut: default_paste_shortcut(),
            restore_clipboard_after_paste: default_restore_clipboard_after_paste(),
            theme: default_theme(),
            auto_start: default_auto_start(),
            close_to_tray: default_close_to_tray(),
            start_minimized: default_start_minimized(),
            ui_language: default_ui_language(),
            capsule_auto_hide: default_capsule_auto_hide(),
        }
    }
}

fn normalize_choice(value: &str, allowed: &[&str], fallback: &str) -> String {
    let trimmed = value.trim();
    if allowed.contains(&trimmed) {
        trimmed.to_string()
    } else {
        fallback.to_string()
    }
}

fn normalize_hotkey_mode_mode(value: &str) -> String {
    if value.trim() == HOTKEY_MODE_TOGGLE {
        HOTKEY_MODE_TOGGLE.to_string()
    } else {
        HOTKEY_MODE_HOLD.to_string()
    }
}

fn sanitize_dictionary(entries: &[String]) -> Vec<String> {
    let mut cleaned = Vec::new();
    for entry in entries {
        let word: String = entry
            .replace('\0', "")
            .trim()
            .chars()
            .take(MAX_DICTIONARY_ENTRY_CHARS)
            .collect();
        if word.is_empty() || cleaned.contains(&word) {
            continue;
        }
        cleaned.push(word);
        if cleaned.len() >= MAX_DICTIONARY_ENTRIES {
            break;
        }
    }
    cleaned
}

impl AppConfig {
    /// Clamp and canonicalize every field so the rest of the app can trust the
    /// values it reads back.
    pub fn normalize(&mut self) {
        self.hotkey = self.hotkey.trim().to_string();
        if self.hotkey.is_empty() {
            self.hotkey = default_hotkey();
        }
        self.hotkey_mode = normalize_hotkey_mode_mode(&self.hotkey_mode);
        self.max_recording_seconds = self
            .max_recording_seconds
            .clamp(MIN_RECORDING_SECONDS, MAX_RECORDING_SECONDS);

        self.stt_base_url = self.stt_base_url.trim().to_string();
        self.stt_provider = normalize_choice(
            &self.stt_provider.trim().to_ascii_lowercase(),
            &[STT_PROVIDER_OPENAI, STT_PROVIDER_VOLCENGINE],
            STT_PROVIDER_OPENAI,
        );
        self.stt_volc_resource_id = normalize_choice(
            &self.stt_volc_resource_id.trim().to_ascii_lowercase(),
            &[STT_VOLC_RESOURCE_SEEDASR, STT_VOLC_RESOURCE_BIGASR],
            STT_VOLC_RESOURCE_SEEDASR,
        );
        self.stt_model = self.stt_model.trim().to_string();
        self.stt_language = {
            let language = self.stt_language.trim();
            if language.is_empty() {
                default_stt_language()
            } else {
                language.to_string()
            }
        };

        self.llm_base_url = self.llm_base_url.trim().to_string();
        self.llm_model = self.llm_model.trim().to_string();
        self.polish_style = normalize_choice(
            &self.polish_style,
            &[
                POLISH_STYLE_MINIMAL,
                POLISH_STYLE_CLEAN,
                POLISH_STYLE_STRUCTURED,
                POLISH_STYLE_CUSTOM,
            ],
            POLISH_STYLE_CLEAN,
        );
        self.polish_custom_prompt = self
            .polish_custom_prompt
            .replace('\0', "")
            .trim()
            .chars()
            .take(2000)
            .collect();

        self.dictionary = sanitize_dictionary(&self.dictionary);

        self.streaming_interval_ms = self
            .streaming_interval_ms
            .clamp(MIN_STREAMING_INTERVAL_MS, MAX_STREAMING_INTERVAL_MS);

        self.output_mode = normalize_choice(
            &self.output_mode,
            &[OUTPUT_MODE_KEYBOARD, OUTPUT_MODE_CLIPBOARD],
            OUTPUT_MODE_KEYBOARD,
        );
        self.paste_shortcut = normalize_choice(
            &self.paste_shortcut,
            &["ctrlV", "cmdV", "ctrlShiftV", "shiftInsert"],
            &default_paste_shortcut(),
        );

        self.theme = normalize_choice(
            &self.theme,
            &[THEME_LIGHT, THEME_DARK, THEME_SYSTEM],
            THEME_SYSTEM,
        );
        self.ui_language = UI_LANGUAGE_ZH.to_string();
    }

    /// Deserialize a stored value, tolerating nulls and unknown fields.
    pub fn from_stored_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        let mut config: Self = serde_json::from_value(value)?;
        config.normalize();
        Ok(config)
    }

    /// True when the selected STT provider has everything it needs to run.
    ///
    /// The API key is stored in the system keychain and checked when the
    /// provider connects, so it is deliberately not part of this synchronous
    /// predicate.
    pub fn stt_is_configured(&self) -> bool {
        if self.stt_provider == STT_PROVIDER_VOLCENGINE {
            return !self.stt_volc_resource_id.trim().is_empty();
        }
        !self.stt_base_url.trim().is_empty() && !self.stt_model.trim().is_empty()
    }

    /// True when polishing should actually call the LLM.
    pub fn llm_is_configured(&self) -> bool {
        self.polish_enabled
            && !self.llm_base_url.trim().is_empty()
            && !self.llm_model.trim().is_empty()
    }
}

/// Recursively merge `patch` into `base`, with `patch` winning on conflicts.
fn deep_merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match base_map.get_mut(&key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base_map.insert(key, value);
                    }
                }
            }
        }
        (base_slot, patch_value) => {
            *base_slot = patch_value;
        }
    }
}

/// Store-backed configuration access with an in-memory cache.
///
/// 生产构建里前端资源是内嵌的，WebView2 可能在 `setup` 完成之前就发起第一次
/// `get_config`。因此这个 state 必须在 `Builder` 阶段注册（[`ConfigManager::pending`]），
/// 拿到 AppHandle 之后再 [`ConfigManager::attach`]。读配置时对「尚未 attach」
/// 做一个很短的等待，避免启动竞态导致设置页加载失败。
pub struct ConfigManager {
    app_handle: std::sync::OnceLock<tauri::AppHandle>,
    cache: Mutex<Option<AppConfig>>,
}

/// 等待 AppHandle 就绪的上限，约 1 秒；正常情况下 setup 在几十毫秒内完成。
const HANDLE_WAIT_ATTEMPTS: usize = 100;
const HANDLE_WAIT_STEP: std::time::Duration = std::time::Duration::from_millis(10);

impl ConfigManager {
    /// 在 `Builder` 阶段创建；此时还没有 AppHandle。
    pub fn pending() -> Self {
        Self {
            app_handle: std::sync::OnceLock::new(),
            cache: Mutex::new(None),
        }
    }

    /// 在 `setup` 里把 AppHandle 交给它。
    pub fn attach(&self, app_handle: tauri::AppHandle) {
        let _ = self.app_handle.set(app_handle);
    }

    fn handle(&self) -> Result<&tauri::AppHandle> {
        for _ in 0..HANDLE_WAIT_ATTEMPTS {
            if let Some(handle) = self.app_handle.get() {
                return Ok(handle);
            }
            std::thread::sleep(HANDLE_WAIT_STEP);
        }
        Err(anyhow!("配置尚未就绪，请稍后重试"))
    }

    /// 异步等 `setup` 把 AppHandle 交进来。
    ///
    /// 生产构建里前端是内嵌资源，WebView2 可能在 `setup` 之前就调用 `get_config`；
    /// 这里不阻塞运行时线程，等就绪后再读，最多等 30 秒。
    pub async fn wait_ready(&self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while self.app_handle.get().is_none() {
            if std::time::Instant::now() >= deadline {
                tracing::warn!("等待配置就绪超时");
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    /// Return the cached configuration, loading it from the store on first use.
    pub fn load(&self) -> Result<AppConfig> {
        if let Some(config) = self
            .cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
        {
            return Ok(config);
        }

        let config = self.read_from_store()?;
        *self.cache.lock().unwrap_or_else(|error| error.into_inner()) = Some(config.clone());
        Ok(config)
    }

    fn read_from_store(&self) -> Result<AppConfig> {
        let store = match self.handle()?.store(STORE_FILE) {
            Ok(store) => store,
            Err(error) => {
                tracing::warn!("无法打开设置存储：{error}");
                return Ok(AppConfig::default());
            }
        };

        Ok(match store.get(CONFIG_KEY) {
            Some(value) => match AppConfig::from_stored_value(value) {
                Ok(config) => config,
                Err(error) => {
                    tracing::warn!("设置内容无法解析，使用默认配置：{error}");
                    AppConfig::default()
                }
            },
            None => AppConfig::default(),
        })
    }

    /// Normalize, cache and persist a complete configuration.
    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let mut config = config.clone();
        config.normalize();
        self.persist(&config)?;
        *self.cache.lock().unwrap_or_else(|error| error.into_inner()) = Some(config);
        Ok(())
    }

    /// Deep-merge a partial patch into the current config, persist it and
    /// return the merged (complete) configuration.
    pub fn apply_patch(&self, patch: serde_json::Value) -> Result<AppConfig> {
        if !patch.is_object() {
            return Err(anyhow!("配置补丁必须是一个 JSON 对象"));
        }

        let current = self.load()?;
        let mut merged = serde_json::to_value(&current)?;
        deep_merge(&mut merged, patch);

        let next = AppConfig::from_stored_value(merged)?;
        self.persist(&next)?;
        *self.cache.lock().unwrap_or_else(|error| error.into_inner()) = Some(next.clone());
        Ok(next)
    }

    fn persist(&self, config: &AppConfig) -> Result<()> {
        let store = self
            .handle()?
            .store(STORE_FILE)
            .map_err(|error| anyhow!("无法打开设置存储：{error}"))?;
        let value = serde_json::to_value(config)?;
        store.set(CONFIG_KEY, value);
        store
            .save()
            .map_err(|error| anyhow!("无法写入设置文件：{error}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_interface_contract() {
        let config = AppConfig::default();
        assert_eq!(config.hotkey, "Ctrl+/");
        assert_eq!(config.hotkey_mode, "hold");
        assert_eq!(config.max_recording_seconds, 30);
        assert_eq!(config.stt_provider, "openai");
        assert_eq!(config.stt_volc_resource_id, "volc.seedasr.sauc.duration");
        assert_eq!(config.stt_model, "whisper-1");
        assert_eq!(config.stt_language, "auto");
        assert!(config.polish_enabled);
        assert_eq!(config.polish_style, "clean");
        assert!(config.streaming_enabled);
        assert!(!config.streaming_insert);
        assert_eq!(config.streaming_interval_ms, 1200);
        assert_eq!(config.output_mode, "keyboard");
        let expected_paste = if cfg!(target_os = "macos") {
            "cmdV"
        } else {
            "ctrlV"
        };
        assert_eq!(config.paste_shortcut, expected_paste);
        assert!(config.restore_clipboard_after_paste);
        assert_eq!(config.theme, "system");
        assert!(config.auto_start);
        assert!(config.close_to_tray);
        assert!(!config.start_minimized);
        assert_eq!(config.ui_language, "zh");
        assert!(config.capsule_auto_hide);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults_and_unknown_fields_are_ignored() {
        let value = serde_json::json!({
            "hotkey": "Ctrl+Shift+A",
            "unknown_field": 42,
            "dictionary": ["Jot", "  Jot  ", "  "]
        });

        let config = AppConfig::from_stored_value(value).unwrap();

        assert_eq!(config.hotkey, "Ctrl+Shift+A");
        assert_eq!(config.hotkey_mode, "hold");
        assert_eq!(config.max_recording_seconds, 30);
        assert_eq!(config.stt_provider, "openai");
        assert_eq!(config.stt_volc_resource_id, "volc.seedasr.sauc.duration");
        assert_eq!(config.dictionary, vec!["Jot".to_string()]);
    }

    #[test]
    fn invalid_stored_values_are_reported_so_callers_can_fall_back() {
        let value = serde_json::json!({ "dictionary": [3] });
        assert!(AppConfig::from_stored_value(value).is_err());
    }

    #[test]
    fn normalize_clamps_recording_seconds_and_choices() {
        let mut config = AppConfig {
            max_recording_seconds: 9000,
            hotkey_mode: "weird".to_string(),
            polish_style: "professional".to_string(),
            output_mode: "pdf".to_string(),
            paste_shortcut: "cmdV".to_string(),
            ui_language: "en".to_string(),
            streaming_interval_ms: 60_000,
            ..AppConfig::default()
        };

        config.normalize();

        assert_eq!(config.max_recording_seconds, MAX_RECORDING_SECONDS);
        assert_eq!(config.hotkey_mode, "hold");
        assert_eq!(config.polish_style, "clean");
        assert_eq!(config.output_mode, "keyboard");
        assert_eq!(config.paste_shortcut, "cmdV");
        assert_eq!(config.streaming_interval_ms, MAX_STREAMING_INTERVAL_MS);
        assert_eq!(config.ui_language, "zh");
    }

    #[test]
    fn deep_merge_keeps_untouched_fields() {
        let mut base = serde_json::json!({
            "hotkey": "Ctrl+/",
            "dictionary": ["a"],
            "nested": {"keep": 1, "replace": 2}
        });
        let patch = serde_json::json!({
            "hotkey": "Ctrl+.",
            "nested": {"replace": 3}
        });

        deep_merge(&mut base, patch);

        assert_eq!(base["hotkey"], "Ctrl+.");
        assert_eq!(base["dictionary"][0], "a");
        assert_eq!(base["nested"]["keep"], 1);
        assert_eq!(base["nested"]["replace"], 3);
    }

    #[test]
    fn configuration_flags_track_required_fields() {
        let mut config = AppConfig::default();
        assert!(!config.stt_is_configured());
        // The defaults point at a local Ollama endpoint and polishing is on.
        assert!(config.llm_is_configured());

        config.stt_base_url = "https://api.openai.com/v1".to_string();
        config.llm_base_url = "https://api.openai.com/v1".to_string();
        config.llm_model = "gpt-4o-mini".to_string();
        assert!(config.stt_is_configured());
        assert!(config.llm_is_configured());

        config.polish_enabled = false;
        assert!(!config.llm_is_configured());
    }

    #[test]
    fn volcengine_provider_does_not_require_the_openai_fields() {
        let mut config = AppConfig {
            stt_provider: STT_PROVIDER_VOLCENGINE.to_string(),
            stt_base_url: String::new(),
            stt_model: String::new(),
            ..AppConfig::default()
        };
        assert!(config.stt_is_configured());

        config.stt_volc_resource_id = "  ".to_string();
        assert!(!config.stt_is_configured());
    }

    #[test]
    fn provider_and_resource_id_are_normalized_to_contract_values() {
        let mut config = AppConfig {
            stt_provider: " VOLCENGINE ".to_string(),
            stt_volc_resource_id: " volc.bigasr.sauc.duration ".to_string(),
            ..AppConfig::default()
        };
        config.normalize();
        assert_eq!(config.stt_provider, STT_PROVIDER_VOLCENGINE);
        assert_eq!(config.stt_volc_resource_id, STT_VOLC_RESOURCE_BIGASR);

        config.stt_provider = "whisper".to_string();
        config.stt_volc_resource_id = "unknown".to_string();
        config.normalize();
        assert_eq!(config.stt_provider, STT_PROVIDER_OPENAI);
        assert_eq!(config.stt_volc_resource_id, STT_VOLC_RESOURCE_SEEDASR);
    }
}
