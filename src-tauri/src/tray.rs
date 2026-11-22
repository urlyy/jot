//! System tray icon and menu (Chinese labels per the interface contract).

use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::pipeline::{PipelineHandle, PipelineState};
use crate::storage::ConfigManager;

/// Managed handle so the menu/tooltip can be rebuilt on state changes.
pub struct TrayHandle {
    pub tray: Mutex<TrayIcon>,
}

fn is_recording(app: &AppHandle) -> bool {
    app.try_state::<PipelineHandle>()
        .map(|pipeline| pipeline.inner().is_recording())
        .unwrap_or(false)
}

fn auto_start_enabled(app: &AppHandle) -> bool {
    app.try_state::<ConfigManager>()
        .and_then(|manager| manager.load().ok())
        .map(|config| config.auto_start)
        .unwrap_or(false)
}

fn tooltip_for(app: &AppHandle) -> &'static str {
    match app
        .try_state::<PipelineHandle>()
        .map(|pipeline| pipeline.inner().current_state())
    {
        Some(PipelineState::Preparing) => "Jot - 正在准备…",
        Some(PipelineState::Recording) => "Jot - 正在录音…",
        Some(PipelineState::Transcribing) => "Jot - 正在转写…",
        Some(PipelineState::Polishing) => "Jot - 正在润色…",
        Some(PipelineState::Outputting) => "Jot - 正在输入…",
        _ => "Jot",
    }
}

/// Show and focus the settings window.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Show the main window and tell the frontend which route to open.
pub fn navigate_to(app: &AppHandle, route: &str) {
    show_main_window(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("tray:navigate", route.to_string());
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let recording = is_recording(app);

    let show = MenuItem::with_id(app, "show_main", "显示主窗口", true, None::<&str>)?;
    let record = MenuItem::with_id(
        app,
        "toggle_recording",
        if recording { "停止录音" } else { "开始录音" },
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let auto_start = CheckMenuItem::with_id(
        app,
        "auto_start",
        "开机启动",
        true,
        auto_start_enabled(app),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let separator_top = PredefinedMenuItem::separator(app)?;
    let separator_bottom = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &show,
            &separator_top,
            &record,
            &settings,
            &auto_start,
            &separator_bottom,
            &quit,
        ],
    )
}

/// Rebuild the menu and tooltip to reflect the current pipeline state.
pub fn refresh_tray(app: &AppHandle) {
    let Some(handle) = app.try_state::<TrayHandle>() else {
        return;
    };
    let Ok(tray) = handle.tray.lock() else {
        return;
    };

    if let Ok(menu) = build_menu(app) {
        if let Err(error) = tray.set_menu(Some(menu)) {
            tracing::warn!("刷新托盘菜单失败：{error}");
        }
    }
    if let Err(error) = tray.set_tooltip(Some(tooltip_for(app))) {
        tracing::warn!("刷新托盘提示失败：{error}");
    }
}

/// Create the tray icon and register its event handlers.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Jot");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let tray = builder
        .on_menu_event(|app, event| handle_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            let clicked = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );
            if clicked {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayHandle {
        tray: Mutex::new(tray),
    });
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "show_main" => show_main_window(app),
        "toggle_recording" => {
            let Some(pipeline) = app
                .try_state::<PipelineHandle>()
                .map(|pipeline| pipeline.inner().clone())
            else {
                return;
            };
            tauri::async_runtime::spawn(async move {
                let result = if pipeline.is_recording() {
                    pipeline.stop().await
                } else if pipeline.current_state() == PipelineState::Idle {
                    pipeline.start().await
                } else {
                    Ok(())
                };
                if let Err(error) = result {
                    tracing::error!("托盘录音操作失败：{error}");
                }
            });
        }
        "settings" => navigate_to(app, "#/settings"),
        "auto_start" => toggle_auto_start(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

fn toggle_auto_start(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    let Some(config_manager) = app.try_state::<ConfigManager>() else {
        return;
    };
    let Ok(config) = config_manager.load() else {
        tracing::warn!("读取配置失败，无法切换开机启动");
        return;
    };

    let enabled = !config.auto_start;
    let autolaunch = app.autolaunch();
    let apply_result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    if let Err(error) = apply_result {
        tracing::error!("切换开机启动失败：{error}");
        return;
    }

    let mut next = config;
    next.auto_start = enabled;
    if let Err(error) = config_manager.save(&next) {
        tracing::error!("保存开机启动设置失败：{error}");
        return;
    }

    let _ = app.emit("config:patch", serde_json::json!({ "auto_start": enabled }));
    refresh_tray(app);
}
