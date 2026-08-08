// ============================================================
// pet_render/mod.rs —— 方案 D 宠物窗口:Rust 原生渲染 + 透明悬浮
//
// 透明实现原理(绕开 WebKitGTK alpha 硬伤,本阶段核心):
//   1. 内容层:渲染器直接产出 RGBA8 像素缓冲(透明像素 alpha=0),
//      不经 WebKit,不存在「内容层不合成 alpha」的问题;
//   2. 窗口层:gtk-layer-shell 把窗口提升为 wlr-layer-shell 表面
//      (Overlay 层),合成器按 ARGB 直接混合,不走普通 toplevel
//      的 GTK 主题背景填充路径 → 透明根治;
//   3. blit:draw 回调把 RGBA 转成 cairo ARGB32(预乘)一次贴图。
//
// layer-shell 集成(2026-08-09 落地):
//   gtk-layer-shell 0.8.2 与 gtk 0.18 兼容;系统库 libgtk-layer-shell
//   0.10.1 已装(VM, pkg-config gtk-layer-shell-0)。
//   niri 26.04 原生支持 zwlr_layer_surface_v1:Overlay 层悬浮所有
//   窗口之上(含全屏),桌面宠物正确形态;KeyboardMode::None 不抢键盘。
//   注:0.8.2 无 set_keep_above(wlr-layer-shell 协议无此概念,Overlay
//   层天然置顶);GTK 层 window.set_keep_above(true) 保留作兜底。
// ============================================================

/// 子模块声明(渲染器协议 / 精灵图渲染 / 协议解析 / 工厂分发)
pub mod factory;
pub mod manifest;
pub mod renderer;
pub mod sprite;

use glib::ControlFlow;
use gtk::prelude::*;
use gtk_layer_shell::LayerShell; // layer-shell trait(init_layer_shell/set_layer 等)
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 动画帧间隔(ms)≈ 60fps。Rust 侧时钟驱动,不依赖前端。
const FRAME_MS: u32 = 16;

/// 演示状态池(证明 set_state + 动画循环工作;后续由面板事件驱动)。
/// 每项 = (状态名, 权重)。自然节奏设计(2026-08-09 修复「切换太快」):
/// - **idle 为主**:每次发呆 8~15s 随机,30% 概率再延长 4~8s(像真宠物
///   偶尔长时间不动);动作只是偶发点缀。
/// - **动作短暂**:2~4s 随机,waving 固定 1.8s(挥手一下)。
/// - **权重 = 切换的「理由」**:thinking 最高(发呆→思考最像真宠物)、
///   working 次之(认真工作)、waving/jumping 最低(互动与兴奋,偶发)。
/// - 旧逻辑缺陷(已修):非 idle 到期后无回退分支(只有 waving 单独处理),
///   切到 thinking/working/jumping 会永久卡住;且 idle 仅 3~5s 就切,
///   节奏机械偏快 → 观感「状态切换太快」。
const DEMO_STATES: [(&str, u32); 4] = [
    ("thinking", 3), // 思考:权重最高
    ("working", 2),  // 工作:次之
    ("waving", 1),   // 挥手:互动表现,偶发
    ("jumping", 1),  // 跳跃:兴奋表现,偶发
];

/// 简单 LCG 随机数(零依赖,状态机节奏随机化用)。
fn demo_next_u64(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

/// 生成一次 idle 停留时长(ms):基础 8~15s 随机,30% 概率再「发呆」
/// 4~8s(避免机械固定节奏,像真宠物偶尔长时间不动)。
fn idle_duration_ms(seed: &mut u64) -> f64 {
    let base = 8000.0 + (demo_next_u64(seed) % 7000) as f64; // 8.0 ~ 14.999 s
    if demo_next_u64(seed) % 100 < 30 {
        base + 4000.0 + (demo_next_u64(seed) % 4000) as f64 // +4 ~ 8 s
    } else {
        base
    }
}

/// 按权重挑一个动作状态,返回 (状态名, 播放时长 ms)。
/// 动作 2~4s 随机;waving 固定 1.8s(挥手一下)。
fn pick_action(seed: &mut u64) -> (&'static str, f64) {
    let total: u32 = DEMO_STATES.iter().map(|(_, w)| w).sum();
    let mut roll = (demo_next_u64(seed) % total as u64) as u32;
    // DEMO_STATES 是 Copy 数组,按值迭代出 (&str, u32),无需解引用
    for (name, w) in DEMO_STATES {
        if roll < w {
            let dur = if name == "waving" {
                1800.0 // 挥手一下(短暂)
            } else {
                2000.0 + (demo_next_u64(seed) % 2000) as f64 // 2 ~ 4 s
            };
            return (name, dur);
        }
        roll -= w;
    }
    ("thinking", 3000.0) // 兜底(权重求和必命中,不会走到)
}

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

    // ── 2.5 layer-shell 提升(2026-08-09 集成,透明根治)──
    // 普通 toplevel 窗口在 GTK3 下会被主题 CSS 预填背景(阶段2 实测
    // (80,80,80) 灰/薄荷绿,Pitfall 35;CSS provider 强制透明实测无效,
    // 红色测试都不生效)。layer-shell 表面由合成器按 ARGB 直接混合,
    // 不走主题填充路径 → 根治。niri 26.04 原生支持 zwlr_layer_surface_v1:
    // Overlay 层悬浮所有窗口之上(含全屏),桌面宠物正确形态。
    // 参考 poe2-overlay 先例;启动需 GDK_BACKEND=wayland(运行命令已带)。
    // 注:0.8.2 无 set_keep_above(wlr-layer-shell 协议无此概念,Overlay
    // 层天然置顶);window.set_keep_above(true)(上方)保留作 X11 兜底。
    {
        // 提升前先确保 RGBA visual 已设置(layer 表面 ARGB 混合的前提)
        let layer_window = window.upcast_ref::<gtk::Window>();
        LayerShell::init_layer_shell(layer_window); // 提升为 layer 表面(0.8.x 无 for_window,用 init_layer_shell)
        layer_window.set_layer(gtk_layer_shell::Layer::Overlay);
        // 锚定四边 = 铺满屏幕(poe2-overlay 全屏 overlay 做法);
        // 桌面宠物用「左上锚 + margin」定位,位置确定便于验证/截图。
        layer_window.set_anchor(gtk_layer_shell::Edge::Left, true);
        layer_window.set_anchor(gtk_layer_shell::Edge::Top, true);
        layer_window.set_layer_shell_margin(gtk_layer_shell::Edge::Left, 24);
        layer_window.set_layer_shell_margin(gtk_layer_shell::Edge::Top, 24);
        // 自动排除键盘:桌面宠物不抢焦点/输入(KeyboardMode::None 默认,
        // 显式声明语义);exclusive_zone=0 不占布局空间(不推挤其他窗口)。
        layer_window.set_keyboard_mode(gtk_layer_shell::KeyboardMode::None);
        layer_window.set_exclusive_zone(0);
        log::info!("[pet_render] layer-shell 提升完成:Overlay 层,左上锚+24px margin");
    }

    // ── 3. DrawingArea:内容层(ARGB 像素贴图)──
    let area = gtk::DrawingArea::new();
    area.set_size_request(w as i32, h as i32);
    window.add(&area);

    // ── 3.5 背景透明说明(2026-08-09 更新)──
    // 阶段2 曾尝试 CSS provider(window/drawingarea background: transparent)
    // 强制透明——实测无效(红色测试都不生效,Pitfall 35,已移除)。
    // 透明由两条保证:① layer-shell Overlay 表面按 ARGB 混合(上方 2.5);
    // ② draw 回调开头 Operator::Source 全透明清底(下方),抹掉 GTK 主题
    // 在 widget 绘制前可能填的背景。两者缺一不可。

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

        // ── 诊断(阶段2 bug 排查):每 30 帧打印渲染统计 + area 实际尺寸 ──
        {
            use std::sync::atomic::{AtomicU32, Ordering};
            static DIAG_N: AtomicU32 = AtomicU32::new(0);
            let n = DIAG_N.fetch_add(1, Ordering::Relaxed);
            if n % 30 == 0 {
                let mut opaque = 0u32;
                let mut min_x = fw;
                let mut min_y = fh;
                let mut max_x = -1i32;
                let mut max_y = -1i32;
                for y in 0..fh {
                    for x in 0..fw {
                        let si = ((y * fw + x) * 4) as usize;
                        if frame.pixels[si + 3] > 0 {
                            opaque += 1;
                            if x < min_x { min_x = x; }
                            if x > max_x { max_x = x; }
                            if y < min_y { min_y = y; }
                            if y > max_y { max_y = y; }
                        }
                    }
                }
                let alloc = _area.allocation();
                eprintln!(
                    "[DIAG] frame={}x{} opaque_px={} bbox=({},{})-({},{}) area_alloc={}x{} area_req={}x{}",
                    fw, fh, opaque, min_x, min_y, max_x, max_y,
                    alloc.width(), alloc.height(),
                    _area.size_request().0, _area.size_request().1
                );
            }
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

    // ── 5. 演示状态机(1s 粒度):自然节奏随机切换业务状态 ──
    // 证明 set_state + 动画循环工作;后续阶段由面板窗口发 tauri
    // 事件驱动真实状态(CPU 负载 → working/overload 等)。
    // 节奏(2026-08-09 修复「切换太快」):idle 为主(8~15s 随机,30% 概率
    // 再发呆),动作偶发(2~4s,waving 1.8s);动作到期必回 idle(旧逻辑缺
    // 此分支会永久卡在动作状态)。idle→动作按权重挑(thinking 最常)。
    let r_demo = Arc::clone(&renderer);
    let mut demo_now: f64 = 0.0;
    let mut idle_until: f64 = 4000.0; // 启动先定格 idle 4s 便于观察(后续 8~15s 随机)
    let mut action_until: f64 = -1.0; // 动作状态到期时间(-1 = 当前不在动作)
    let mut current: &str = "idle";
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
    glib::timeout_add_local(Duration::from_millis(1000), move || {
        demo_now += 1000.0;

        // ① 动作状态到期 → 必回 idle(旧逻辑只有 waving 单独回收,
        //    切到 thinking/working/jumping 会永久卡住,已统一处理)
        if current != "idle" && demo_now >= action_until {
            let prev = current;
            current = "idle";
            let mut r = r_demo.lock().unwrap();
            r.set_state("idle");
            idle_until = demo_now + idle_duration_ms(&mut seed);
            log::info!(
                "[pet_render] {} 结束 → idle(下次发呆 {:.1}s)",
                prev,
                (idle_until - demo_now) / 1000.0
            );
        }

        // ② idle 到期 → 按权重挑动作(思考最常,跳跃/挥手偶发)
        if current == "idle" && demo_now >= idle_until {
            let (next, dur) = pick_action(&mut seed);
            current = next;
            action_until = demo_now + dur;
            let mut r = r_demo.lock().unwrap();
            r.set_state(next);
            log::info!(
                "[pet_render] 演示状态切换 → {}(播放 {:.1}s)",
                next,
                dur / 1000.0
            );
        }

        ControlFlow::Continue
    });

    window.show_all();
    log::info!("[pet_render] Rust 宠物窗口已显示({}x{})", w, h);

    // 保留 app 句柄(托盘等 tauri 能力仍可用)
    let _ = app;
    Ok(())
}
