use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    window::Color,
    Manager, PhysicalPosition, Position, Runtime, WebviewUrl, WebviewWindowBuilder,
};

use crate::monitor; // monitor 模块在 crate 根(lib.rs 声明),app.rs 内用裸 `monitor::` 需引入

// ── 注意:app.rs 已按 PLAN.md 拆分建议(P1-5)瘦身 ──
// 系统信息结构/共享缓存/轮询线程/系统信息命令已抽到 monitor.rs,
// 本文件只保留:窗口控制命令 + 系统托盘 + 应用入口(注册)。
// 托盘与窗口命令后续可再拆 tray.rs / window.rs。

// ── Tauri 命令(窗口控制)──

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

/// 桌面漫游:按增量移动主窗口(物理像素)。
/// 基于当前窗口位置偏移 dx/dy,由前端漫游服务(roam.ts)逐帧调用。
#[tauri::command]
async fn move_window<R: Runtime>(
    app: tauri::AppHandle<R>,
    dx: f64,
    dy: f64,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let pos = window.outer_position().map_err(|e| e.to_string())?;
        window
            .set_position(Position::Physical(PhysicalPosition {
                x: pos.x + dx as i32,
                y: pos.y + dy as i32,
            }))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
        .manage(monitor::AppState::default())
        .invoke_handler(tauri::generate_handler![
            monitor::get_system_info,
            monitor::get_cpu_usage,
            monitor::get_memory_info,
            set_window_visible,
            toggle_always_on_top,
            close_app,
            move_window,
        ])
        .setup(|app| {
            // ── 方案 D 开关:MO_PET_MODE=rust → Rust 原生宠物窗口 ──
            // (透明自绘,绕开 WebKitGTK alpha 硬伤;详见 pet_render/mod.rs)
            // 默认(未设置/其他值)→ 现有 WebKit 窗口路径,行为不变。
            if std::env::var("MO_PET_MODE").as_deref() == Ok("rust") {
                // 监测线程启动:返回事件接收端,宠物窗口订阅后由真实
                // CPU/内存数据驱动状态(高负载 → overload 警示动画)。
                let rx = monitor::start_monitor(app.handle().clone());
                crate::pet_render::spawn_pet_window(app, rx)?;
                setup_tray(app).ok();
                return Ok(());
            }

            // 桌面体验优化:显式 WebviewWindowBuilder 重建主窗口,强制 wry 透明路径。
            // (tauri.conf.json 的 windows 已清空,窗口完全由这里创建——不依赖 config,
            //  链式 .transparent(true) 直接作用于 WebviewWindowBuilder,绕开 niri/Wayland
            //  下 config 透明配置在 webview 内容层不生效的问题)
            let window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("Desktop Pet")
            .inner_size(200.0, 200.0)
            .resizable(false)
            .transparent(true) // 显式透明:强制 wry transparent 路径
            .decorations(false) // 无标题栏
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(true)
            .build()
            .expect("failed to build main window");

            // 兜底(第二保险):显式把窗口背景设为全透明,与 builder/config 三处对齐
            let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));

            // 监测线程照常启动(WebKit 路径不订阅告警事件,receiver 直接
            // 丢弃——drop 后 monitor 线程 send 返回 Err 被忽略,不影响缓存写入)
            monitor::start_monitor(app.handle().clone());
            setup_tray(app).ok();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
