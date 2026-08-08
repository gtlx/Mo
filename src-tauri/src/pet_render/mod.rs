// ============================================================
// pet_render/mod.rs —— 方案 D 宠物窗口:Rust 原生渲染 + 透明悬浮
//
// 透明实现原理(绕开 WebKitGTK alpha 硬伤,本阶段核心):
//   1. 内容层:渲染器直接产出 RGBA8 像素缓冲(透明像素 alpha=0),
//      不经 WebKit,不存在「内容层不合成 alpha」的问题;
//   2. 窗口层:GTK 窗口 app_paintable + RGBA visual + 无边框 +
//      keep_above + skip_taskbar;Wayland 下合成器混合 ARGB 表面;
//   3. blit:draw 回调把 RGBA 转成 cairo ARGB32(预乘)一次贴图。
//
// layer-shell 说明(2026-08-08 实测):
//   gtk-layer-shell crate 与 Cargo.lock 的 gtk 0.18 版本兼容,
//   但需要系统库 libgtk-layer-shell(VM 编译机缺,且 sudo 需密码
//   无法安装)→ 本阶段降级为普通无边框窗口 + ARGB 自绘;
//   overlay 层语义(悬浮全屏之上/穿透点击)后续补。接入点已留:
//   window.upcast_ref::<gtk::Window>() 拿到 GTK 窗口后,
//   调用 gtk_layer::LayerShell::for_window(...) 提升即可
//   (poe2-overlay 先例)。
// ============================================================

/// 子模块声明(渲染器协议 / 精灵图渲染 / 协议解析 / 工厂分发)
pub mod factory;
pub mod manifest;
pub mod renderer;
pub mod sprite;

use glib::ControlFlow;
use gtk::prelude::*;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 动画帧间隔(ms)≈ 60fps。Rust 侧时钟驱动,不依赖前端。
const FRAME_MS: u32 = 16;

/// 演示状态序列(证明 set_state + 动画循环工作;后续由面板事件驱动)
/// - "waving" 是精灵图行名直切(对应前端 greet 挥手,短暂后回 idle)
/// - 其余为业务状态(经 STATUS_TO_ROW 映射到对应行)
const DEMO_STATES: [&str; 5] = ["idle", "waving", "thinking", "working", "jumping"];

/// 创建并显示 Rust 原生宠物窗口。
/// 必须在 GTK 主线程调用(tauri setup 满足);窗口完全脱离 WebKit。
pub fn spawn_pet_window(app: &tauri::App) -> Result<(), Box<dyn Error>> {
    // ── 1. 渲染器(工厂按 pet.json type 分发)──
    let mut renderer = factory::create_renderer().map_err(|e| format!("渲染器创建失败: {e}"))?;
    renderer.set_state("idle"); // 初始待机(构造默认已是 idle,显式设置保证语义清晰)
    let (w, h) = renderer.size();
    log::info!("[pet_render] 渲染器就绪: {}x{}, 透明自绘窗口", w, h);

    // ── 2. 透明无边框 GTK 窗口 ──
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Mo Pet (Rust)");
    window.set_decorated(false); // 无边框(WM 装饰)
    window.set_skip_taskbar_hint(true); // 不出现在任务栏
    window.set_keep_above(true); // 置顶悬浮
    window.set_app_paintable(true); // 应用自绘背景(不填主题背景色)
    window.set_resizable(false);
    window.set_default_size(w as i32, h as i32);
    window.set_position(gtk::WindowPosition::Center);

    // RGBA visual:Wayland 下合成器按 ARGB 混合;X11 下走 ARGB 合成
    // (X11 下 GTK3 会强制 CSD 白标题栏,属已知 Pitfall,验证以
    //  GDK_BACKEND=wayland 为准;layer-shell 落地后此问题消失)
    // 注:gtk-rs 0.18 中 Window::screen() 同时存在于 GtkWindowExt 与
    // WidgetExt,须用完整 trait 路径消歧。
    if let Some(screen) = gtk::prelude::GtkWindowExt::screen(&window) {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }

    // ── 3. DrawingArea:内容层(ARGB 像素贴图)──
    let area = gtk::DrawingArea::new();
    area.set_size_request(w as i32, h as i32);
    window.add(&area);

    let renderer = Arc::new(Mutex::new(renderer));

    // draw 回调:渲染一帧 → RGBA(straight)→ ARGB32(预乘)→ blit
    let r_draw = Arc::clone(&renderer);
    area.connect_draw(move |_area, cr| {
        // ── 阶段2(2026-08-08):清除 GTK 主题背景填充 ──
        // GTK3 在 draw 回调前已按主题 CSS 渲染 widget 背景(实测默认主题
        // (80,80,80) 灰;GTK_THEME=Adwaita:light 变 (78,201,176),实锤主题
        // 背景填充,与 WebKitGTK alpha 无关)。cairo 默认 Over 混合会把企鹅
        // blit 在主题背景之上 → 内容层不透明。修法:Operator::Source(直接
        // 覆盖目标含 alpha,不做混合)+ 全透明 paint 抹掉主题背景,再恢复
        // Over 正常混合贴企鹅像素(企鹅边缘有半透明像素,须保持混合)。
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0); // 全透明,直接覆盖(含 alpha)
        let _ = cr.paint(); // 清空整个 DrawingArea,主题背景不复存在
        cr.set_operator(cairo::Operator::Over); // 恢复默认混合,继续正常 blit

        let mut r = r_draw.lock().unwrap();
        let frame = r.render();
        let (fw, fh) = (frame.width as i32, frame.height as i32);
        if fw <= 0 || fh <= 0 {
            return glib::Propagation::Proceed;
        }

        // RGBA8(straight alpha)→ cairo ARGB32(预乘,小端内存序 B,G,R,A)
        let stride = match cairo::Format::ARgb32.stride_for_width(fw as u32) {
            Ok(s) => s,
            Err(_) => return glib::Propagation::Proceed,
        };
        let mut buf = vec![0u8; (stride * fh) as usize];
        let px = &frame.pixels;
        for y in 0..fh {
            for x in 0..fw {
                let si = ((y * fw + x) * 4) as usize;
                let (r8, g8, b8, a8) = (px[si], px[si + 1], px[si + 2], px[si + 3]);
                if a8 == 0 {
                    continue; // 透明像素保持 0(不预乘,防止色溢)
                }
                // 预乘 alpha(整数近似),按 ARGB32 小端字节序写入
                let di = (y * stride + x * 4) as usize;
                buf[di] = (b8 as u16 * a8 as u16 / 255) as u8; // B
                buf[di + 1] = (g8 as u16 * a8 as u16 / 255) as u8; // G
                buf[di + 2] = (r8 as u16 * a8 as u16 / 255) as u8; // R
                buf[di + 3] = a8; // A
            }
        }
        let surface = match cairo::ImageSurface::create_for_data(
            buf,
            cairo::Format::ARgb32,
            fw,
            fh,
            stride,
        ) {
            Ok(s) => s,
            Err(_) => return glib::Propagation::Proceed,
        };
        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
        let _ = cr.paint();
        glib::Propagation::Proceed
    });

    // ── 4. 动画循环:glib timeout 驱动 tick + queue_draw(Rust 侧时钟)──
    let r_frame = Arc::clone(&renderer);
    let area_frame = area.clone();
    glib::timeout_add_local(Duration::from_millis(FRAME_MS as u64), move || {
        let mut r = r_frame.lock().unwrap();
        r.tick(FRAME_MS as f64); // 推进内部时钟(帧循环/呼吸/眨眼)
        drop(r);
        area_frame.queue_draw(); // 触发 draw 回调渲染一帧
        ControlFlow::Continue
    });

    // ── 5. 演示状态机(1s 粒度):周期性随机切换业务状态 ──
    // 证明 set_state + 动画循环工作;后续阶段由面板窗口发 tauri
    // 事件驱动真实状态(CPU 负载 → working/overload 等)。
    let r_demo = Arc::clone(&renderer);
    let mut demo_now: f64 = 0.0;
    let mut next_change: f64 = 4000.0; // 启动 4s 后第一次切换(先定格 idle 便于观察)
    let mut current: &str = "idle";
    let mut waving_until: f64 = -1.0; // waving 状态结束时间(内部时钟)
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
    glib::timeout_add_local(Duration::from_millis(1000), move || {
        demo_now += 1000.0;

        // waving 到期自动回 idle(对应前端 greet 自动恢复)
        if current == "waving" && demo_now >= waving_until {
            current = "idle";
            let mut r = r_demo.lock().unwrap();
            r.set_state("idle");
            log::info!("[pet_render] waving 结束 → idle");
        }

        if demo_now >= next_change && current == "idle" {
            // 简单 LCG 取随机状态(排除 idle,避免原地切换)
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = 1 + ((seed >> 33) as usize) % (DEMO_STATES.len() - 1);
            let next = DEMO_STATES[idx];
            current = next;
            let mut r = r_demo.lock().unwrap();
            r.set_state(next);
            log::info!("[pet_render] 演示状态切换 → {}", next);

            // 播放时长:waving 短(挥手一下),其余 3~5s
            let dur = if next == "waving" {
                waving_until = demo_now + 1800.0;
                1800.0
            } else {
                3000.0 + ((seed >> 17) % 2000) as f64
            };
            next_change = demo_now + dur;
        }

        ControlFlow::Continue
    });

    window.show_all();
    log::info!("[pet_render] Rust 宠物窗口已显示({}x{})", w, h);

    // 保留 app 句柄(托盘等 tauri 能力仍可用)
    let _ = app;
    Ok(())
}
