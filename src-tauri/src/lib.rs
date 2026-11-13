//! termtorrent — a lightweight torrent client with a terminal-styled UI.

mod commands;
mod engine;
mod types;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::commands::EngineState;
use crate::engine::{Engine, TICK_EVENT, TICK_MS};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "termtorrent=info,librqbit=warn".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(EngineState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::snapshot,
            commands::add_torrent,
            commands::add_torrent_bytes,
            commands::pause_torrent,
            commands::resume_torrent,
            commands::remove_torrent,
            commands::pause_all,
            commands::resume_all,
            commands::torrent_files,
            commands::torrent_output_dir,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.with_webview(|_| unsafe {
                    disable_webkit_text_touch_bar();
                });
            }

            let handle = app.handle().clone();
            let config_dir = config_dir(app.handle());
            let download_dir = default_download_dir(app.handle());

            // Boot the session off the UI thread so the window paints immediately,
            // then start the refresh loop that feeds the terminal view.
            tauri::async_runtime::spawn(async move {
                match Engine::start(config_dir, download_dir).await {
                    Ok(engine) => {
                        *handle.state::<EngineState>().write() = Some(Arc::clone(&engine));
                        let _ = handle.emit("termtorrent://ready", ());
                        tick_loop(handle, engine).await;
                    }
                    Err(e) => {
                        tracing::error!("failed to start torrent session: {e:#}");
                        let _ = handle.emit("termtorrent://fatal", format!("{e:#}"));
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building termtorrent")
        .run(|app, event| {
            // Let librqbit flush its resume data before the process goes away.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(engine) = app.state::<EngineState>().read().clone() {
                    tauri::async_runtime::block_on(engine.stop());
                }
            }
        });
}

/// Where settings and resume data live.
///
/// `TERMTORRENT_CONFIG_DIR` overrides the OS location, which gives you a
/// self-contained profile: run a second instance for testing or demos without
/// touching the torrents in your everyday session. The port range already
/// spans 4240-4259, so the instances do not fight over a listener.
fn config_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("TERMTORRENT_CONFIG_DIR") {
        return std::path::PathBuf::from(dir);
    }
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Where downloads land the first time the app runs.
///
/// Android has no user-visible Downloads folder an app may write to without
/// runtime permissions, so torrents go to app-private storage there. Desktop
/// platforms use the real Downloads folder.
fn default_download_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    #[cfg(target_os = "android")]
    {
        app.path()
            .app_data_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("downloads")
    }

    #[cfg(not(target_os = "android"))]
    {
        app.path()
            .download_dir()
            .or_else(|_| app.path().home_dir())
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("termtorrent")
    }
}

/// Push a fresh snapshot to the frontend on a fixed cadence.
async fn tick_loop(handle: tauri::AppHandle, engine: Arc<Engine>) {
    let mut interval = tokio::time::interval(Duration::from_millis(TICK_MS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        let _ = handle.emit(TICK_EVENT, engine.snapshot());
    }
}

/// Work around a macOS 27.x WebKit/AppKit crash where `WKTextTouchBarItemController`
/// throws `NSInvalidArgumentException` or dereferences null inside `itemForIdentifier:`
/// when an `<input>` receives focus.
#[cfg(target_os = "macos")]
unsafe fn disable_webkit_text_touch_bar() {
    use std::ffi::{c_char, c_void};

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn class_replaceMethod(
            cls: *mut c_void,
            name: *mut c_void,
            imp: *const c_void,
            types: *const c_char,
        ) -> *mut c_void;
    }

    unsafe extern "C" fn nil_item(
        _this: *mut c_void,
        _cmd: *mut c_void,
        _identifier: *mut c_void,
    ) -> *mut c_void {
        std::ptr::null_mut()
    }

    let cls = objc_getClass(c"WKTextTouchBarItemController".as_ptr());
    if !cls.is_null() {
        let sel = sel_registerName(c"itemForIdentifier:".as_ptr());
        class_replaceMethod(cls, sel, nil_item as *const c_void, c"@@:@".as_ptr());
    }
}
