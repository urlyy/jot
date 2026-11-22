//! Single dictation hotkey: accelerator parsing, registration, retries and
//! hold/toggle dispatch.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::pipeline::{PipelineHandle, PipelineState};
use crate::storage::{AppConfig, ConfigManager, HOTKEY_MODE_TOGGLE};

/// Delay between registration retry attempts while the shortcut is unusable.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// After this many quick retries the supervisor slows down.
pub const FAST_RETRY_LIMIT: u32 = 3;
/// Slow retry delay once the fast retries are exhausted.
pub const SLOW_RETRY_INTERVAL: Duration = Duration::from_secs(30);

const SUPPORTED_KEYS_HINT: &str =
    "支持 A-Z、0-9、F1-F12、Space、Enter、Tab、Escape 以及 / . , ; - = [ ]";

#[derive(Debug, Clone)]
struct RegisteredHotkey {
    shortcut: Shortcut,
}

#[derive(Debug, Default)]
struct HotkeyInner {
    registered: Option<RegisteredHotkey>,
    mode: String,
    paused: bool,
    retry_attempts: u32,
    last_error: Option<String>,
    last_emitted_error: Option<String>,
}

/// Shared runtime state of the global dictation shortcut.
#[derive(Clone, Default)]
pub struct HotkeyManager {
    inner: Arc<Mutex<HotkeyInner>>,
}

impl HotkeyManager {
    fn lock(&self) -> std::sync::MutexGuard<'_, HotkeyInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Current hotkey mode (`hold` or `toggle`).
    pub fn mode(&self) -> String {
        let mode = self.lock().mode.clone();
        if mode.is_empty() {
            "hold".to_string()
        } else {
            mode
        }
    }

    /// Last registration failure, if the shortcut is currently unusable.
    pub fn registration_error(&self) -> Option<String> {
        self.lock().last_error.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.lock().paused
    }

    /// Register `accelerator` with the given mode, replacing any previous one.
    ///
    /// On failure the previous shortcut is restored and a readable error is
    /// returned, so the saved configuration can stay ahead of the runtime.
    pub fn register(
        &self,
        app: &AppHandle,
        accelerator: &str,
        mode: &str,
    ) -> Result<(), String> {
        let shortcut = parse_accelerator(accelerator)?;
        let normalized_mode = if mode.trim() == HOTKEY_MODE_TOGGLE {
            HOTKEY_MODE_TOGGLE
        } else {
            "hold"
        };
        let previous = {
            let mut inner = self.lock();
            inner.mode = normalized_mode.to_string();
            inner.paused = false;
            inner.registered.clone()
        };

        let manager = app.global_shortcut();
        if let Err(error) = manager.unregister_all() {
            tracing::warn!("注销旧快捷键失败：{error}");
        }

        match manager.register(shortcut) {
            Ok(()) => {
                let recovered = {
                    let mut inner = self.lock();
                    let had_error = inner.last_error.take().is_some();
                    inner.registered = Some(RegisteredHotkey { shortcut });
                    inner.retry_attempts = 0;
                    inner.last_emitted_error = None;
                    had_error
                };
                if recovered {
                    let _ = app.emit("hotkey:registration-recovered", ());
                }
                tracing::info!("已注册全局快捷键：{accelerator}（{normalized_mode}）");
                Ok(())
            }
            Err(error) => {
                // Restore the previous registration so the user keeps a
                // working shortcut while the stored config already changed.
                let mut restored = false;
                if let Some(previous) = previous {
                    if let Ok(()) = manager.register(previous.shortcut) {
                        restored = true;
                        let mut inner = self.lock();
                        inner.registered = Some(previous);
                    }
                }

                let message = format!(
                    "无法注册快捷键「{}」：{}（可能已被其他程序占用）",
                    accelerator.trim(),
                    error
                );
                self.record_failure(app, message.clone());

                if restored {
                    tracing::warn!("{message}，已恢复上一个可用的快捷键");
                }
                Err(message)
            }
        }
    }

    fn record_failure(&self, app: &AppHandle, message: String) {
        let should_emit = {
            let mut inner = self.lock();
            inner.retry_attempts = inner.retry_attempts.saturating_add(1);
            let changed = inner.last_emitted_error.as_deref() != Some(message.as_str());
            inner.last_error = Some(message.clone());
            if changed {
                inner.last_emitted_error = Some(message.clone());
            }
            changed
        };
        if should_emit {
            let _ = app.emit("hotkey:registration-failed", message);
        }
    }

    /// Temporarily release the global shortcut (used while capturing a new one).
    pub fn pause(&self, app: &AppHandle) -> Result<(), String> {
        {
            let mut inner = self.lock();
            inner.paused = true;
        }
        app.global_shortcut()
            .unregister_all()
            .map_err(|error| format!("释放全局快捷键失败：{error}"))
    }

    /// Re-register the shortcut stored in the configuration.
    pub fn resume(&self, app: &AppHandle) -> Result<(), String> {
        let config = load_config(app).ok_or_else(|| "配置尚未就绪".to_string())?;
        self.register(app, &config.hotkey, &config.hotkey_mode)
    }

    fn next_retry_delay(&self) -> Option<Duration> {
        let inner = self.lock();
        if inner.paused || inner.last_error.is_none() {
            return None;
        }
        if inner.retry_attempts <= FAST_RETRY_LIMIT {
            Some(RETRY_INTERVAL)
        } else {
            Some(SLOW_RETRY_INTERVAL)
        }
    }
}

fn load_config(app: &AppHandle) -> Option<AppConfig> {
    app.try_state::<ConfigManager>()
        .and_then(|manager| manager.load().ok())
}

/// Background supervisor that keeps retrying a failed registration.
pub fn spawn_retry_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let delay = app
                .try_state::<HotkeyManager>()
                .and_then(|manager| manager.inner().next_retry_delay())
                .unwrap_or(SLOW_RETRY_INTERVAL);
            tokio::time::sleep(delay).await;

            let Some(manager) = app.try_state::<HotkeyManager>().map(|state| state.inner().clone())
            else {
                break;
            };
            if manager.next_retry_delay().is_none() {
                continue;
            }
            let Some(config) = load_config(&app) else {
                continue;
            };

            match manager.register(&app, &config.hotkey, &config.hotkey_mode) {
                Ok(()) => tracing::info!("全局快捷键重试注册成功"),
                Err(message) => tracing::warn!("全局快捷键重试注册失败：{message}"),
            }
        }
    });
}

/// Build the handler that turns global shortcut events into pipeline actions.
pub fn build_shortcut_handler(
    app: AppHandle,
) -> impl Fn(&AppHandle, &Shortcut, tauri_plugin_global_shortcut::ShortcutEvent) + Send + Sync + 'static
{
    move |_app, _shortcut, event| {
        handle_shortcut_event(&app, event.state);
    }
}

fn handle_shortcut_event(app: &AppHandle, state: ShortcutState) {
    let Some(pipeline) = app.try_state::<PipelineHandle>().map(|handle| handle.inner().clone())
    else {
        return;
    };
    let mode = app
        .try_state::<HotkeyManager>()
        .map(|manager| manager.inner().mode())
        .unwrap_or_else(|| "hold".to_string());

    let toggle = mode == HOTKEY_MODE_TOGGLE;
    match state {
        ShortcutState::Pressed if toggle => {
            if pipeline.current_state() == PipelineState::Idle {
                start_recording(app, pipeline);
            } else {
                stop_recording(app, pipeline);
            }
        }
        ShortcutState::Pressed => start_recording(app, pipeline),
        ShortcutState::Released if !toggle => stop_recording(app, pipeline),
        ShortcutState::Released => {}
    }
}

fn start_recording(app: &AppHandle, pipeline: PipelineHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = pipeline.start().await {
            tracing::error!("启动录音失败：{error}");
            let _ = app.emit(
                "pipeline:error",
                crate::pipeline::PipelineErrorPayload {
                    code: "internal".to_string(),
                    details: Some(format!("启动录音失败：{error}")),
                },
            );
        }
    });
}

fn stop_recording(app: &AppHandle, pipeline: PipelineHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = pipeline.stop().await {
            tracing::error!("停止录音失败：{error}");
            let _ = app.emit(
                "pipeline:error",
                crate::pipeline::PipelineErrorPayload {
                    code: "internal".to_string(),
                    details: Some(format!("停止录音失败：{error}")),
                },
            );
        }
    });
}

/// Parse an accelerator such as `Ctrl+Shift+Space` into a global shortcut.
pub fn parse_accelerator(accelerator: &str) -> Result<Shortcut, String> {
    let trimmed = accelerator.trim();
    if trimmed.is_empty() {
        return Err("快捷键不能为空，请填写例如 Ctrl+/".to_string());
    }

    let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(format!("快捷键格式不正确：「{trimmed}」"));
    }
    let Some(key_part) = parts.last() else {
        return Err(format!("快捷键格式不正确：「{trimmed}」"));
    };

    let mut modifiers = Modifiers::empty();
    for part in &parts[..parts.len() - 1] {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "super" | "cmd" | "command" | "meta" | "win" | "windows" => {
                modifiers |= Modifiers::SUPER
            }
            other => {
                return Err(format!(
                    "无法识别的修饰键「{other}」，支持 Ctrl、Alt、Shift、Super/Command"
                ))
            }
        }
    }

    let code = key_code(key_part).ok_or_else(|| {
        format!("无法识别的主键「{key_part}」，{SUPPORTED_KEYS_HINT}")
    })?;

    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };
    Ok(Shortcut::new(mods, code))
}

fn key_code(key: &str) -> Option<Code> {
    let lower = key.to_ascii_lowercase();
    let code = match lower.as_str() {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "/" | "slash" => Code::Slash,
        "." | "period" | "。" => Code::Period,
        "," | "comma" => Code::Comma,
        ";" | "semicolon" => Code::Semicolon,
        "-" | "minus" => Code::Minus,
        "=" | "equal" => Code::Equal,
        "[" | "bracketleft" => Code::BracketLeft,
        "]" | "bracketright" => Code::BracketRight,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        _ => return None,
    };
    Some(code)
}

/// Default accelerator, matching [`AppConfig::default`].
pub fn default_accelerator() -> String {
    AppConfig::default().hotkey
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ctrl_slash() {
        let shortcut = parse_accelerator("Ctrl+/").unwrap();
        assert_eq!(shortcut.mods, Modifiers::CONTROL);
        assert_eq!(shortcut.key, Code::Slash);
    }

    #[test]
    fn parsing_is_case_insensitive_and_accepts_aliases() {
        for accelerator in ["cTrL+/", "Control+Slash", " control + / "] {
            let shortcut = parse_accelerator(accelerator).unwrap();
            assert_eq!(shortcut.mods, Modifiers::CONTROL, "{accelerator}");
            assert_eq!(shortcut.key, Code::Slash, "{accelerator}");
        }
    }

    #[test]
    fn all_modifier_aliases_map_to_tauri_modifiers() {
        assert_eq!(
            parse_accelerator("Option+A").unwrap().mods,
            Modifiers::ALT
        );
        assert_eq!(parse_accelerator("Alt+A").unwrap().mods, Modifiers::ALT);
        for name in ["Super", "Command", "Cmd", "Meta", "Win"] {
            let shortcut = parse_accelerator(&format!("{name}+A")).unwrap();
            assert_eq!(shortcut.mods, Modifiers::SUPER, "{name}");
            assert_eq!(shortcut.key, Code::KeyA, "{name}");
        }
    }

    #[test]
    fn supports_the_documented_key_set() {
        for (text, expected) in [
            ("Space", Code::Space),
            ("Enter", Code::Enter),
            ("Tab", Code::Tab),
            ("Escape", Code::Escape),
            ("F1", Code::F1),
            ("F12", Code::F12),
            ("0", Code::Digit0),
            ("9", Code::Digit9),
            ("z", Code::KeyZ),
            (".", Code::Period),
            (",", Code::Comma),
            (";", Code::Semicolon),
            ("-", Code::Minus),
            ("=", Code::Equal),
            ("[", Code::BracketLeft),
            ("]", Code::BracketRight),
        ] {
            assert_eq!(
                parse_accelerator(&format!("Ctrl+{text}")).unwrap().key,
                expected,
                "{text}"
            );
        }
    }

    #[test]
    fn allows_shortcuts_without_modifiers() {
        let shortcut = parse_accelerator("F8").unwrap();
        assert_eq!(shortcut.mods, Modifiers::empty());
        assert_eq!(shortcut.key, Code::F8);
    }

    #[test]
    fn returns_readable_errors_instead_of_panicking() {
        assert!(parse_accelerator("").unwrap_err().contains("不能为空"));
        assert!(parse_accelerator("Ctrl+").unwrap_err().contains("格式不正确"));
        assert!(parse_accelerator("Ctrl+Nope")
            .unwrap_err()
            .contains("无法识别的主键"));
        assert!(parse_accelerator("Hyper+A")
            .unwrap_err()
            .contains("无法识别的修饰键"));
    }

    #[test]
    fn default_accelerator_matches_the_config_default() {
        assert_eq!(default_accelerator(), "Ctrl+/");
    }

    #[test]
    fn fresh_manager_has_no_registration_error() {
        let manager = HotkeyManager::default();
        assert_eq!(manager.registration_error(), None);
        assert_eq!(manager.mode(), "hold");
        assert!(!manager.is_paused());
    }
}
