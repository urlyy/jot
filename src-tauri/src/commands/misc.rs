//! Misc commands: app info, platform capabilities, accessibility permission,
//! hotkey control and the recording pipeline entry points.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::hotkey::HotkeyManager;
use crate::pipeline::{
    is_accessibility_trusted, request_accessibility_permission as request_accessibility_from_os,
    PipelineHandle,
};
use crate::platform::{self, PlatformCapabilities};
use crate::storage::ConfigManager;

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub os: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: platform::current_os().to_string(),
    }
}

#[tauri::command]
pub fn get_platform_capabilities() -> PlatformCapabilities {
    platform::capabilities()
}

#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    is_accessibility_trusted()
}

#[tauri::command]
pub fn request_accessibility_permission() -> bool {
    request_accessibility_from_os()
}

#[tauri::command]
pub fn apply_hotkey(
    app: AppHandle,
    state: tauri::State<'_, ConfigManager>,
    hotkey: String,
) -> Result<(), String> {
    let Some(manager) = app.try_state::<HotkeyManager>() else {
        return Err("快捷键管理器尚未就绪".to_string());
    };

    let mut config = state.load().map_err(|error| error.to_string())?;
    manager
        .inner()
        .register(&app, &hotkey, &config.hotkey_mode)?;

    config.hotkey = hotkey.trim().to_string();
    state
        .save(&config)
        .map_err(|error| format!("保存快捷键失败：{error}"))?;
    Ok(())
}

#[tauri::command]
pub fn pause_hotkey(app: AppHandle) -> Result<(), String> {
    let Some(manager) = app.try_state::<HotkeyManager>() else {
        return Err("快捷键管理器尚未就绪".to_string());
    };
    manager.inner().pause(&app)
}

#[tauri::command]
pub fn resume_hotkey(app: AppHandle) -> Result<(), String> {
    let Some(manager) = app.try_state::<HotkeyManager>() else {
        return Err("快捷键管理器尚未就绪".to_string());
    };
    manager.inner().resume(&app)
}

#[tauri::command]
pub fn get_hotkey_registration_error(app: AppHandle) -> Option<String> {
    app.try_state::<HotkeyManager>()
        .and_then(|manager| manager.inner().registration_error())
}

#[tauri::command]
pub async fn start_recording(state: tauri::State<'_, PipelineHandle>) -> Result<(), String> {
    state.start().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn stop_recording(state: tauri::State<'_, PipelineHandle>) -> Result<(), String> {
    state.stop().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub fn abort_recording(state: tauri::State<'_, PipelineHandle>) -> Result<(), String> {
    state.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_version_and_os() {
        let info = get_app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(!info.os.is_empty());
    }
}
