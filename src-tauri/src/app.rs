use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use sysinfo::System;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, Runtime,
};

// ── 系统信息结构 ──

#[derive(Serialize, Clone)]
pub struct SystemInfo {
    cpu_usage: f32,
    memory_used: u64,
    memory_total: u64,
    memory_percent: f32,
}

// ── 后台缓存状态 ──

pub struct AppState {
    pub cpu_usage: Mutex<f32>,
    pub system_info: Mutex<SystemInfo>,
}

// ── Tauri 命令 ──

#[tauri::command]
fn get_system_info(state: tauri::State<'_, AppState>) -> SystemInfo {
    state.system_info.lock().unwrap().clone()
}

#[tauri::command]
fn get_cpu_usage(state: tauri::State<'_, AppState>) -> f32 {
    *state.cpu_usage.lock().unwrap()
}

#[tauri::command]
fn get_memory_info(state: tauri::State<'_, AppState>) -> (u64, u64, f32) {
    let info = state.system_info.lock().unwrap();
    (info.memory_used, info.memory_total, info.memory_percent)
}

#[tauri::command]
async fn set_window_visible<R: Runtime>(
    app: tauri::AppHandle<R>,
    visible: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if visible {
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
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
        window
            .set_always_on_top(!current)
            .map_err(|e| e.to_string())?;
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

// ── 后台 CPU 监测线程 ──

fn start_monitor(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut sys = System::new_all();
        loop {
            sys.refresh_cpu();
            std::thread::sleep(Duration::from_millis(1000));
            sys.refresh_cpu();
            sys.refresh_memory();

            let cpu_usage = sys.global_cpu_usage();
            let memory_used = sys.used_memory();
            let memory_total = sys.total_memory();
            let memory_percent = if memory_total > 0 {
                (memory_used as f32 / memory_total as f32) * 100.0
            } else {
                0.0
            };

            if let Some(state) = app.try_state::<AppState>() {
                let mut usage = state.cpu_usage.lock().unwrap();
                *usage = cpu_usage;
                let mut info = state.system_info.lock().unwrap();
                info.cpu_usage = cpu_usage;
                info.memory_used = memory_used;
                info.memory_total = memory_total;
                info.memory_percent = memory_percent;
            }
        }
    });
}

// ── 系统托盘 ──

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // 加载托盘图标（复用应用图标）
    let icon_bytes = include_bytes!("../icons/icon.png");
    let icon = Image::from_bytes(icon_bytes)?;

    // 菜单项
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &hide, &quit])?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("桌面宠物")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

// ── 应用入口 ──

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            cpu_usage: Mutex::new(0.0),
            system_info: Mutex::new(SystemInfo {
                cpu_usage: 0.0,
                memory_used: 0,
                memory_total: 0,
                memory_percent: 0.0,
            }),
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            get_cpu_usage,
            get_memory_info,
            set_window_visible,
            toggle_always_on_top,
            close_app,
        ])
        .setup(|app| {
            start_monitor(app.handle().clone());
            setup_tray(app).ok();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
