//! Configuration commands: read, partial update, auto-start and capsule flags.

use tauri::{AppHandle, Emitter, Manager};

use crate::hotkey::HotkeyManager;
use crate::storage::{AppConfig, ConfigManager};

#[tauri::command]
pub async fn get_config(state: tauri::State<'_, ConfigManager>) -> Result<AppConfig, String> {
    state.wait_ready().await;
    state.load().map_err(|error| error.to_string())
}

/// Deep-merge a partial patch, persist it and return the full configuration.
///
/// If the hotkey (or its mode) changed, the global shortcut is re-registered.
/// Registration failure rejects the call but keeps the new value on disk, so a
/// later launch can retry — exactly as the contract requires.
#[tauri::command]
pub async fn update_config(
    app: AppHandle,
    state: tauri::State<'_, ConfigManager>,
    patch: serde_json::Value,
) -> Result<AppConfig, String> {
    state.wait_ready().await;
    let previous = state.load().map_err(|error| error.to_string())?;
    let next = state.apply_patch(patch).map_err(|error| error.to_string())?;

    let hotkey_changed =
        previous.hotkey != next.hotkey || previous.hotkey_mode != next.hotkey_mode;
    if hotkey_changed {
        let Some(manager) = app.try_state::<HotkeyManager>() else {
            return Err("快捷键管理器尚未就绪".to_string());
        };
        manager
            .inner()
            .register(&app, &next.hotkey, &next.hotkey_mode)?;
    }

    if previous.capsule_auto_hide != next.capsule_auto_hide {
        crate::tray::refresh_tray(&app);
    }

    Ok(next)
}

#[tauri::command]
pub async fn set_auto_start(
    app: AppHandle,
    state: tauri::State<'_, ConfigManager>,
    enabled: bool,
) -> Result<AppConfig, String> {
    use tauri_plugin_autostart::ManagerExt;
    state.wait_ready().await;

    let autolaunch = app.autolaunch();
    let apply_result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    apply_result.map_err(|error| format!("设置开机启动失败：{error}"))?;

    let mut config = state.load().map_err(|error| error.to_string())?;
    config.auto_start = enabled;
    state
        .save(&config)
        .map_err(|error| error.to_string())?;

    let _ = app.emit(
        "config:patch",
        serde_json::json!({ "auto_start": enabled }),
    );
    crate::tray::refresh_tray(&app);

    Ok(config)
}

#[tauri::command]
pub fn set_capsule_auto_hide(
    app: AppHandle,
    state: tauri::State<'_, ConfigManager>,
    enabled: bool,
) -> Result<AppConfig, String> {
    let mut config = state.load().map_err(|error| error.to_string())?;
    config.capsule_auto_hide = enabled;
    state
        .save(&config)
        .map_err(|error| error.to_string())?;

    let _ = app.emit(
        "config:patch",
        serde_json::json!({ "capsule_auto_hide": enabled }),
    );
    crate::tray::refresh_tray(&app);

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_change_detection_includes_mode_changes() {
        let previous = AppConfig::default();
        let mut next = previous.clone();
        assert!(!(previous.hotkey != next.hotkey || previous.hotkey_mode != next.hotkey_mode));

        next.hotkey_mode = if previous.hotkey_mode == "toggle" {
            "hold".to_string()
        } else {
            "toggle".to_string()
        };
        assert!(previous.hotkey != next.hotkey || previous.hotkey_mode != next.hotkey_mode);
    }
}
