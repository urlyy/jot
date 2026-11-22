pub mod audio;
pub mod commands;
pub mod credentials;
pub mod dictionary;
pub mod error;
pub mod hotkey;
#[cfg(target_os = "linux")]
mod linux_x11;
pub mod llm;
#[cfg(target_os = "macos")]
pub mod native_hotkey;
pub mod output;
pub mod pipeline;
pub mod platform;
pub mod recording_deadline;
pub mod storage;
pub mod stt;
pub mod tray;

pub use hotkey::parse_accelerator;
pub use tray::{refresh_tray, TrayHandle};

use std::time::Duration;

use tauri::{Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;
use tracing_subscriber::EnvFilter;

use storage::{AppConfig, ConfigManager};

fn build_shared_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(4)
        .tcp_keepalive(Duration::from_secs(60))
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_else(|error| {
            tracing::warn!("创建 HTTP 客户端失败，退回默认配置：{error}");
            reqwest::Client::new()
        })
}

/// Map the stored `auto_start` preference onto the real launch-at-login state.
fn sync_auto_start(
    app: &tauri::AppHandle,
    config_manager: &ConfigManager,
    config: &mut AppConfig,
) {
    use tauri_plugin_autostart::ManagerExt;

    let autolaunch = app.autolaunch();
    let current = match autolaunch.is_enabled() {
        Ok(enabled) => enabled,
        Err(error) => {
            tracing::warn!("读取开机启动状态失败：{error}");
            return;
        }
    };
    if current == config.auto_start {
        return;
    }

    let apply_result = if config.auto_start {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    if let Err(error) = apply_result {
        tracing::warn!("同步开机启动设置失败：{error}");
        config.auto_start = current;
        if let Err(error) = config_manager.save(config) {
            tracing::warn!("回写开机启动状态失败：{error}");
        }
    }
}

fn attach_main_window_handlers(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            let close_to_tray = handle
                .try_state::<ConfigManager>()
                .and_then(|manager| manager.load().ok())
                .map(|config| config.close_to_tray)
                .unwrap_or(true);
            if close_to_tray {
                api.prevent_close();
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
        }
    });
}

/// Linux WebKit/GTK workarounds applied before any WebView is created.
#[cfg(target_os = "linux")]
fn apply_linux_workarounds() {
    fn flag(name: &str) -> bool {
        std::env::var(name).is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
    }

    let session_type = platform::current_session_type();
    let nvidia_wayland = session_type == "wayland"
        && std::env::var("__GLX_VENDOR_LIBRARY_NAME")
            .is_ok_and(|vendor| vendor.eq_ignore_ascii_case("nvidia"));

    if nvidia_wayland || flag("JOT_DISABLE_WEBKIT_DMABUF") {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if flag("JOT_DISABLE_WEBKIT_COMPOSITING") {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
    if flag("JOT_FORCE_SOFTWARE_GL") {
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    if flag("JOT_FORCE_GDK_X11") && std::env::var("DISPLAY").is_ok() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

pub fn run() {
    #[cfg(target_os = "linux")]
    let xinitthreads_status = linux_x11::init_xlib_threads();

    #[cfg(target_os = "linux")]
    apply_linux_workarounds();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("Jot=debug,info")),
        )
        .init();

    #[cfg(target_os = "linux")]
    tracing::debug!(?xinitthreads_status, "Linux X11 初始化结果");

    // 生产构建里前端是内嵌资源，WebView2 可能在 setup 之前就调用 get_config，
    // 所以配置 state 必须在构建阶段就注册，setup 里再补上 AppHandle。
    let config_state = storage::ConfigManager::pending();
    let builder = tauri::Builder::default()
        .manage(config_state)
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .setup(|app| {
            let app_handle = app.handle().clone();

            #[cfg(feature = "devtools")]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let config = {
                let config_manager = app.state::<ConfigManager>();
                config_manager.attach(app_handle.clone());
                let mut config = config_manager
                    .load()
                    .map_err(|error| format!("读取配置失败：{error}"))?;
                sync_auto_start(&app_handle, &config_manager, &mut config);
                config
            };

            let client = build_shared_http_client();
            let pipeline = pipeline::PipelineHandle::new(app_handle.clone(), client.clone());

            app.manage(client);
            app.manage(pipeline);
            app.manage(hotkey::HotkeyManager::default());

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(hotkey::build_shortcut_handler(app_handle.clone()))
                    .build(),
            )?;

            {
                let manager = app.state::<hotkey::HotkeyManager>().inner().clone();
                if let Err(message) =
                    manager.register(&app_handle, &config.hotkey, &config.hotkey_mode)
                {
                    tracing::warn!("初始全局快捷键注册失败：{message}");
                }
                hotkey::spawn_retry_loop(app_handle.clone());
            }

            tray::create(&app_handle)?;
            attach_main_window_handlers(&app_handle);

            if !config.start_minimized {
                tray::show_main_window(&app_handle);
            }

            tracing::info!("Jot 已启动（版本 {}）", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::get_config,
            commands::config::update_config,
            commands::config::set_auto_start,
            commands::config::set_capsule_auto_hide,
            commands::credentials::read_credential,
            commands::credentials::set_credential,
            commands::credentials::clear_credential,
            commands::credentials::get_credential_status,
            commands::diagnostics::test_stt_connection,
            commands::diagnostics::bench_llm_connection,
            commands::diagnostics::fetch_llm_models,
            commands::misc::get_app_info,
            commands::misc::get_platform_capabilities,
            commands::misc::check_accessibility_permission,
            commands::misc::request_accessibility_permission,
            commands::misc::apply_hotkey,
            commands::misc::pause_hotkey,
            commands::misc::resume_hotkey,
            commands::misc::get_hotkey_registration_error,
            commands::misc::start_recording,
            commands::misc::stop_recording,
            commands::misc::abort_recording,
        ]);

    match builder.build(tauri::generate_context!()) {
        Ok(app) => app.run(|_app_handle, event| match event {
            RunEvent::ExitRequested { .. } => {
                tracing::debug!("应用退出请求");
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => {
                tray::show_main_window(_app_handle);
            }
            _ => {}
        }),
        Err(error) => {
            eprintln!("启动 Jot 失败：{error}");
        }
    }
}
