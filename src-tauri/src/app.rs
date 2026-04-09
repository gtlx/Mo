use serde::Serialize;
use sysinfo::System;
use tauri::{Manager, Runtime};

#[derive(Serialize)]
pub struct SystemInfo {
    cpu_usage: f32,
    memory_used: u64,
    memory_total: u64,
    memory_percent: f32,
}

#[tauri::command]
fn get_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_usage();
    let memory_used = sys.used_memory();
    let memory_total = sys.total_memory();
    let memory_percent = if memory_total > 0 {
        (memory_used as f32 / memory_total as f32) * 100.0
    } else {
        0.0
    };

    SystemInfo {
        cpu_usage,
        memory_used,
        memory_total,
        memory_percent,
    }
}

#[tauri::command]
fn get_cpu_usage() -> f32 {
    let mut sys = System::new_all();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_all();
    sys.global_cpu_usage()
}

#[tauri::command]
fn get_memory_info() -> (u64, u64, f32) {
    let mut sys = System::new_all();
    sys.refresh_all();
    let used = sys.used_memory();
    let total = sys.total_memory();
    let percent = if total > 0 {
        (used as f32 / total as f32) * 100.0
    } else {
        0.0
    };
    (used, total, percent)
}

#[tauri::command]
async fn set_window_visible<R: Runtime>(
    app: tauri::AppHandle<R>,
    visible: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if visible {
            window.show().map_err(|e| e.to_string())?;
        } else {
            window.hide().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn toggle_always_on_top<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        let current = window.is_always_on_top().map_err(|e| e.to_string())?;
        window.set_always_on_top(!current).map_err(|e| e.to_string())?;
        Ok(!current)
    } else {
        Err("Window not found".to_string())
    }
}

#[tauri::command]
async fn close_app<R: Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            get_cpu_usage,
            get_memory_info,
            set_window_visible,
            toggle_always_on_top,
            close_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
