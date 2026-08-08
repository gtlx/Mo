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
pub mod roam;
pub mod sprite;

use glib::ControlFlow;
use gtk::prelude::*;
use gtk_layer_shell::LayerShell; // layer-shell trait(init_layer_shell/set_layer 等)
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::monitor::MonitorEvent;

/// 动画帧间隔(ms)≈ 60fps。Rust 侧时钟驱动,不依赖前端。
const FRAME_MS: u32 = 16;

/// P1-6 抚摸反馈参数(2026-08-09):
/// - 单击互动总时长:撒娇(waving)状态 + 飘爱心,共 1.5s;
/// - 双击 greet 挥手:时长短一些(1s),沿用 waving 状态行;
/// - 爱心:3 颗错峰冒出,每颗生命 800ms,上升 110px + 左右摇摆,
///   半透明淡入淡出——纯 cairo 叠加在 blit 之上,渲染器核心零改动。
const PET_INTERACT_MS: f64 = 1500.0;
const PET_GREET_MS: f64 = 1000.0;
const HEART_COUNT: usize = 3;
const HEART_LIFE_MS: f64 = 800.0;
const HEART_STAGGER_MS: f64 = 300.0;
const HEART_RISE_PX: f64 = 110.0;

/// ── P1-1 弹出覆盖窗参数(2026-08-09)──
/// 弹出窗 = 第二个透明无边框窗口(普通 toplevel,**可拖拽**):
/// - 标题带 "Popup" 后缀,便于 `niri msg windows` 文本解析定位
///   (layer 表面不在该列表,普通 toplevel 在;解析见 query_niri_window);
/// - 位置持久化到 ~/.local/share/mo/popup-pos.json(逻辑坐标,与
///   niri Workspace-view position 一致):拖拽结束延迟查询实际坐标
///   写入,下次弹出时经 niri IPC(move-floating-window 增量)恢复;
/// - 与主窗共用渲染器实例(同一宠物,动画/爱心/红边同步),见 PetShared。
const POPUP_TITLE: &str = "Mo Pet (Rust) Popup";
/// 位置文件目录(相对 XDG_DATA_HOME;未设置时 ~/.local/share)
const POPUP_POS_DIR: &str = "mo";
const POPUP_POS_FILE: &str = "popup-pos.json";
/// 默认弹出位置 = 主窗(layer 左上锚 + 24px margin)右侧
const POPUP_DEFAULT_X: f64 = 24.0;
const POPUP_DEFAULT_GAP: f64 = 12.0;

/// 漫游(roam.rs):弹出窗自动在桌面走动——随机目标/平滑步进/边界 clamp/
/// 到达停留 5~15s/拖拽暂停,移动走 niri IPC move-floating-window,
/// 详见 roam.rs 模块头注释(漫游只作用于弹出窗,主窗 layer 无法被移动)。

/// 演示状态池(证明 set_state + 动画循环工作;`MO_DEMO=1` 时启用)。
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

/// 低负载权重池(真实状态驱动,cpu < MO_CPU_MID_THR):宠物保持悠闲,
/// 思考为主、工作偶发——负载低时「没什么可忙的」。
const LOW_LOAD_STATES: [(&str, u32); 4] = [
    ("thinking", 3),
    ("working", 1),
    ("waving", 1),
    ("jumping", 1),
];

/// 中负载权重池(cpu >= MO_CPU_MID_THR):宠物「认真起来」,
/// thinking 与 working 并重(负载上来了就多做工作动作)。
const MID_LOAD_STATES: [(&str, u32); 4] = [
    ("thinking", 3),
    ("working", 3),
    ("waving", 1),
    ("jumping", 1),
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

/// 按权重表挑一个动作状态,返回 (状态名, 播放时长 ms)。
/// 动作 2~4s 随机;waving 固定 1.8s(挥手一下)。
/// `states` 为权重池(演示/低负载/中负载共用同一套逻辑);
/// 权重池是编译期 'static 常量(数组元素 &'static str),故签名
/// 显式 'static——若写成 `&[(&str, u32)]` 返回的 name 生命周期
/// 会被收窄到切片引用,编译报「lifetime may not live long enough」。
fn pick_action_from(
    seed: &mut u64,
    states: &'static [(&'static str, u32)],
) -> (&'static str, f64) {
    let total: u32 = states.iter().map(|(_, w)| w).sum();
    let mut roll = (demo_next_u64(seed) % total as u64) as u32;
    // states 元素是 Copy(&str, u32),按引用解构出 &str/u32,无需解引用
    for &(name, w) in states {
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

/// 演示模式动作选择(权重池 = DEMO_STATES)
fn pick_action(seed: &mut u64) -> (&'static str, f64) {
    pick_action_from(seed, &DEMO_STATES)
}

/// 真实状态驱动动作选择:权重池随负载档位切换——
/// 低负载(cpu < MO_CPU_MID_THR)用 LOW_LOAD_STATES(工作偶发),
/// 中负载用 MID_LOAD_STATES(思考/工作并重)。过载不进此函数
/// (由 OverloadStarted 事件直接驱动 overload 状态)。
fn pick_action_for_load(seed: &mut u64, mid_load: bool) -> (&'static str, f64) {
    if mid_load {
        pick_action_from(seed, &MID_LOAD_STATES)
    } else {
        pick_action_from(seed, &LOW_LOAD_STATES)
    }
}

/// 当前时间戳(epoch 毫秒)。P1-6 抚摸反馈用「截止时间戳」在按钮回调
/// (写入)与状态驱动/draw 回调(读取)之间共享——三者都在 GTK 主线程,
/// 原子读写即够,不引入锁。
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// draw 帧计数器(诊断每 30 帧打印 + 红边脉冲相位共用)。
/// 模块级 static:主窗与弹出窗的 draw 回调都调用 draw_pet_frame,
/// 共用同一计数(两窗各画各的,计数只是诊断节流,不要求独立)。
static DIAG_N: AtomicU32 = AtomicU32::new(0);

/// ── P1-1 共享状态(2026-08-09)──
/// 主窗与弹出窗共用同一组 Arc:渲染器(同一宠物、动画时钟同步)+
/// overload 红边标志 + 抚摸反馈截止时间戳(爱心/撒娇)。两窗口的
/// draw 回调都读这里,GTK 主线程串行访问,原子/锁即够。
pub struct PetShared {
    pub renderer: Arc<Mutex<Box<dyn renderer::PetRenderer>>>,
    pub overload_flag: Arc<AtomicBool>,
    pub interact_until_ms: Arc<AtomicU64>,
    pub hearts_until_ms: Arc<AtomicU64>,
}

/// 弹出窗句柄 + 显示状态(跨多次双击共享;句柄必须长期持有,
/// 否则 gtk::Window 引用计数归零会销毁窗口)。
struct PopupState {
    window: Option<gtk::Window>,
    visible: bool,
}

/// 弹出窗持久化位置(逻辑坐标,与 niri Workspace-view position 一致)
#[derive(serde::Serialize, serde::Deserialize)]
struct PopupPos {
    x: f64,
    y: f64,
}

/// cairo 画一颗心形(两瓣圆弧 + 尖角),中心 (cx, cy)、半径 r、透明度 alpha。
/// 用于抚摸反馈:单击后宠物上方飘起的爱心(半透明、随动画进度淡入淡出)。
/// 纯 cairo 叠加在 blit 之上,渲染器核心(sprite.rs)零改动。
/// 形状:左瓣圆心偏左上、右瓣圆心偏右上(半径 0.55r),两瓣从 π 扫到 0
/// (cairo 默认坐标系下角度顺时针增大,即从左过顶部到右),再连到底部尖角。
fn draw_heart(cr: &cairo::Context, cx: f64, cy: f64, r: f64, alpha: f64) {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    cr.set_source_rgba(1.0, 0.35, 0.55, a); // 粉红爱心
    cr.arc(cx - r * 0.55, cy - r * 0.55, r * 0.55, std::f64::consts::PI, 0.0); // 左瓣
    cr.arc(cx + r * 0.55, cy - r * 0.55, r * 0.55, std::f64::consts::PI, 0.0); // 右瓣
    cr.line_to(cx, cy + r * 1.1); // 右瓣终点连到底部尖角
    cr.close_path(); // 闭合回左瓣起点
    let _ = cr.fill();
}

/// 创建并显示 Rust 原生宠物窗口。
/// 必须在 GTK 主线程调用(tauri setup 满足);窗口完全脱离 WebKit。
/// `rx`:monitor 监测事件接收端(真实状态驱动;MO_DEMO=1 时忽略,
/// 走演示状态机)。
pub fn spawn_pet_window(
    app: &tauri::App,
    rx: mpsc::Receiver<MonitorEvent>,
) -> Result<(), Box<dyn Error>> {
    // ── 1. 渲染器(工厂按 pet.json type 分发)──
    let mut renderer = factory::create_renderer().map_err(|e| format!("渲染器创建失败: {e}"))?;
    renderer.set_state("idle"); // 初始待机(构造默认已是 idle,显式设置保证语义清晰)
    let (w, h) = renderer.size();
    log::info!("[pet_render] 渲染器就绪: {}x{}, 透明自绘窗口", w, h);

    // ── 1.5 共享状态(P1-1:主窗与弹出窗共用)──
    // 渲染器 + overload 红边标志 + 抚摸反馈截止时间戳打包成 PetShared;
    // 弹出窗 draw 回调读取同一组 Arc → 两个窗口显示同一宠物,动画/
    // 爱心/红边同步(共享渲染器只 tick 一次,两窗各画各的帧)。
    let shared = Arc::new(PetShared {
        renderer: Arc::new(Mutex::new(renderer)),
        overload_flag: Arc::new(AtomicBool::new(false)),
        interact_until_ms: Arc::new(AtomicU64::new(0)),
        hearts_until_ms: Arc::new(AtomicU64::new(0)),
    });

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

    // ── 3.6 P1-1 弹出窗状态与动画集合 ──
    // popup_state:弹出窗句柄 + 可见性,跨多次双击共享(首次双击懒创建);
    // areas:主窗 + 弹出窗的 DrawingArea 集合,动画循环每帧统一 queue_draw
    // (共享渲染器只 tick 一次,两窗口显示同一宠物、动画同步)。
    let popup_state = Arc::new(Mutex::new(PopupState {
        window: None,
        visible: false,
    }));
    let areas: Arc<Mutex<Vec<gtk::DrawingArea>>> =
        Arc::new(Mutex::new(vec![area.clone()]));

    // ── 3.65 漫游状态(P1-1 最后一项:弹出窗桌面漫游)──
    // 漫游只作用于弹出窗(普通 toplevel,可被 niri IPC 移动);主窗是
    // layer-shell Overlay 表面,不在 niri msg windows 列表,查不到
    // 位置/ID → 无法被 niri 移动(见 roam.rs 模块头注释)。这里只创建
    // 实例,供 toggle(启停)/拖拽(暂停恢复)/漫游时钟(注册)接线。
    let roam_state = Arc::new(roam::RoamState::new());

    // draw 回调:渲染一帧 → RGBA(straight)→ ARGB32(预乘)→ blit
    // P1-1:绘制逻辑抽到 draw_pet_frame,主窗与弹出窗共用(同一渲染器
    // 实例,两窗画面同步;GTK 主线程串行,无锁竞争)。
    let shared_draw = Arc::clone(&shared);
    area.connect_draw(move |_area, cr| {
        draw_pet_frame(cr, &shared_draw, _area);
        glib::Propagation::Proceed
    });

    // ── 3.7 抚摸反馈事件(P1-6)+ 弹出窗 toggle(P1-1)──
    // 单击 → 爱心+撒娇;双击 → greet 挥手 + 弹出/收起覆盖窗(toggle)。
    // 单击立即触发(不做延迟区分):GTK 双击会先派发 click_count=1 再
    // 派发 2,首次单击的爱心/撒娇与随后的挥手共存,观感自然。
    // 互动期强制 waving,到期由状态驱动的「动作到期必回 idle」统一回收。
    let interact_btn = Arc::clone(&shared.interact_until_ms);
    let hearts_btn = Arc::clone(&shared.hearts_until_ms);
    // P1-1:双击分支 toggle 弹出窗(popup_state 跨双击共享,首次懒创建)
    let popup_btn = Arc::clone(&popup_state);
    let shared_btn = Arc::clone(&shared);
    let areas_btn = Arc::clone(&areas);
    // P1-1 漫游:双击 toggle 时启停弹出窗漫游(传入同一 RoamState)
    let roam_btn = Arc::clone(&roam_state);
    area.connect_button_press_event(move |_a, ev| {
        if ev.button() != 1 {
            return glib::Propagation::Proceed; // 只响应左键(右键留给未来菜单)
        }
        let now = now_millis();
        let cc = ev.click_count().unwrap_or(0);
        if cc >= 2 {
            // 双击 → greet 挥手(沿用 waving 状态行,时长短一些)+ 弹窗 toggle
            interact_btn.store(now + PET_GREET_MS as u64, Ordering::Relaxed);
            eprintln!(
                "[pet_render] 双击 → greet 挥手({:.0}ms) + 弹出窗 toggle",
                PET_GREET_MS
            );
            toggle_popup(
                &popup_btn,
                &shared_btn,
                &areas_btn,
                &roam_btn,
                w as i32,
                h as i32,
            );
        } else {
            // 单击 → 抚摸反馈:撒娇 waving + 飘爱心,共 1.5s
            interact_btn.store(now + PET_INTERACT_MS as u64, Ordering::Relaxed);
            hearts_btn.store(now + PET_INTERACT_MS as u64, Ordering::Relaxed);
            eprintln!(
                "[pet_render] 单击 → 抚摸反馈(爱心 + 撒娇 {:.0}ms)",
                PET_INTERACT_MS
            );
        }
        glib::Propagation::Stop
    });

    // ── 3.8 漫游时钟(P1-1 最后一项:弹出窗桌面漫游)──
    // 独立于动画循环(16ms):每 ROAM_INTERVAL_MS(160ms)驱动一次漫游
    // 状态机(tick)。弹出窗未显示时 active=false,tick 直接返回,开销
    // 可忽略;显示期间:查询位置 → 选目标/步进 → niri IPC 增量移动。
    let roam_loop = Arc::clone(&roam_state);
    glib::timeout_add_local(Duration::from_millis(roam::ROAM_INTERVAL_MS), move || {
        roam_loop.tick();
        ControlFlow::Continue
    });

    // ── 4. 动画循环:glib timeout 驱动 tick + 所有窗口 queue_draw ──
    // P1-1:areas 集合含主窗 + 弹出窗的 DrawingArea——共享渲染器只 tick
    // 一次,循环里对所有 area queue_draw,两窗口各画各的(同一帧,同步)。
    let r_frame = Arc::clone(&shared.renderer);
    let areas_loop = Arc::clone(&areas);
    glib::timeout_add_local(Duration::from_millis(FRAME_MS as u64), move || {
        let mut r = r_frame.lock().unwrap();
        r.tick(FRAME_MS as f64); // 推进内部时钟(帧循环/呼吸/眨眼)
        drop(r);
        for a in areas_loop.lock().unwrap().iter() {
            a.queue_draw(); // 触发各窗口 draw 回调渲染一帧
        }
        ControlFlow::Continue
    });

    // ── 5. 状态驱动(1s 粒度):monitor 事件驱动真实状态(P1-5)──
    // 替换原演示状态机:消费 monitor.rs 的告警/采样事件——
    //   - OverloadStarted(滞回确认:CPU>85% 或 内存>90% 持续 3s)→ 切
    //     overload 警示动画(jumping 行快速循环 + 红边脉冲,紧张/冒汗观感),
    //     overload 期间不受演示节奏干扰,直到解除;
    //   - OverloadEnded(持续低于阈值 5s)→ 回 idle;
    //   - Sample(每秒)→ 更新负载档位:低负载保持 idle 为主的悠闲节奏
    //     (thinking 为主、工作偶发),中负载 working 权重提高(宠物「认真
    //     起来」)——保留自然节奏,不因数据微波动频繁切换。
    // 自然节奏保留(2026-08-09 修复「切换太快」):idle 为主(8~15s 随机,
    // 30% 概率再发呆),动作偶发(2~4s,waving 1.8s),动作到期必回 idle。
    // 调试开关:MO_DEMO=1 回退旧演示状态机(不看真实数据,权重池固定),
    // 便于纯动画调试。
    let demo_mode = std::env::var("MO_DEMO").as_deref() == Ok("1");

    let r_drive = Arc::clone(&shared.renderer);
    let overload_drive = Arc::clone(&shared.overload_flag);
    let interact_drive = Arc::clone(&shared.interact_until_ms);
    let mut drive_now: f64 = 0.0;
    let mut idle_until: f64 = 4000.0; // 启动先定格 idle 4s 便于观察(后续 8~15s 随机)
    let mut action_until: f64 = -1.0; // 动作状态到期时间(-1 = 当前不在动作)
    let mut current: &str = "idle";
    let mut mid_load = false; // 中负载档位(cpu >= MO_CPU_MID_THR,权重池切换用)
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
    glib::timeout_add_local(Duration::from_millis(1000), move || {
        drive_now += 1000.0;

        // ① 消费 monitor 事件(真实状态驱动;MO_DEMO=1 时不消费,走演示节奏)
        if !demo_mode {
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    MonitorEvent::OverloadStarted { cpu, mem } => {
                        if current != "overload" {
                            current = "overload";
                            overload_drive.store(true, Ordering::Relaxed);
                            r_drive.lock().unwrap().set_state("overload");
                            eprintln!(
                                "[pet_render] ⚠ 过载告警:CPU {:.0}% / 内存 {:.0}% → overload 警示(红边)",
                                cpu, mem
                            );
                        }
                    }
                    MonitorEvent::OverloadEnded { cpu, mem } => {
                        if current == "overload" {
                            current = "idle";
                            overload_drive.store(false, Ordering::Relaxed);
                            r_drive.lock().unwrap().set_state("idle");
                            idle_until = drive_now + idle_duration_ms(&mut seed);
                            eprintln!(
                                "[pet_render] 过载解除(CPU {:.0}% / 内存 {:.0}%)→ idle",
                                cpu, mem
                            );
                        }
                    }
                    MonitorEvent::Sample { level, .. } => {
                        // 负载档位(瞬时):中负载 → working 权重提高;
                        // 过载档位由上面的告警事件驱动,这里只更新档位。
                        mid_load = level == crate::monitor::LoadLevel::Mid;
                    }
                }
            }
        }

        // ①.5 抚摸互动(P1-6):互动期内强制 waving 撒娇
        // 单击/双击回调写入 interact_until_ms 截止时间戳;这里(1s 粒度)
        // 检测到互动未到期就切到 waving,并按剩余时长续期 action_until,
        // 到期由下方「动作到期必回 idle」统一回收——用户连点可延长撒娇。
        // 过载优先:overload 期间不打断警示(爱心仍与红边共存,互不冲突)。
        let now_epoch = now_millis();
        let interact_end = interact_drive.load(Ordering::Relaxed);
        if interact_end > now_epoch {
            if current != "overload" && current != "waving" {
                current = "waving";
                r_drive.lock().unwrap().set_state("waving");
                eprintln!(
                    "[pet_render] 抚摸互动 → waving(撒娇,剩余 {:.0}ms)",
                    (interact_end - now_epoch) as f64
                );
            }
            if current == "waving" {
                // 互动持续期间不断续期,保证到期才回 idle
                action_until = drive_now + (interact_end - now_epoch) as f64;
            }
        }

        // ② 非过载时的自然节奏(过载优先:overload 状态不做随机切换,
        //    持续警示直到 OverloadEnded 事件)
        if current != "overload" {
            // 动作状态到期 → 必回 idle(旧逻辑只有 waving 单独回收,
            // 切到 thinking/working/jumping 会永久卡住,已统一处理)
            if current != "idle" && drive_now >= action_until {
                let prev = current;
                current = "idle";
                r_drive.lock().unwrap().set_state("idle");
                idle_until = drive_now + idle_duration_ms(&mut seed);
                log::info!(
                    "[pet_render] {} 结束 → idle(下次发呆 {:.1}s)",
                    prev,
                    (idle_until - drive_now) / 1000.0
                );
            }

            // idle 到期 → 按权重挑动作:真实模式权重随负载档位切换,
            // 演示模式用固定 DEMO_STATES(思考最常,跳跃/挥手偶发)
            if current == "idle" && drive_now >= idle_until {
                let (next, dur) = if demo_mode {
                    pick_action(&mut seed)
                } else {
                    pick_action_for_load(&mut seed, mid_load)
                };
                current = next;
                action_until = drive_now + dur;
                r_drive.lock().unwrap().set_state(next);
                log::info!(
                    "[pet_render] {}状态切换 → {}(播放 {:.1}s)",
                    if demo_mode { "演示" } else { "真实" },
                    next,
                    dur / 1000.0
                );
            }
        }

        ControlFlow::Continue
    });

    window.show_all();
    log::info!("[pet_render] Rust 宠物窗口已显示({}x{})", w, h);

    // 保留 app 句柄(托盘等 tauri 能力仍可用)
    let _ = app;
    Ok(())
}

// ============================================================
// P1-1 弹出覆盖窗(2026-08-09)
// ============================================================
// 设计(与主窗的差异是刻意的):
//   - 主窗 = layer-shell Overlay 表面(透明根治,但无 xdg move 请求,
//     不能交互拖拽;也不在 niri msg windows 列表,查不到位置);
//   - 弹出窗 = 普通 toplevel(keep_above+skip_taskbar 天然浮层):
//     ① begin_move_drag 拖拽(合成器接管,Wayland 正统);
//     ② 在 niri msg windows 列表,拖拽结束可查实际坐标 → 持久化;
//     ③ 显示状态独立于主窗(主窗是 layer 表面无最小化概念,两窗
//        各自存活,互不影响)。
//   透明:RGBA visual + draw 回调 Operator::Source 清底(阶段2 方案),
//   与主窗内容层同一套绘制逻辑(draw_pet_frame)。

/// 绘制一帧宠物(主窗与弹出窗共用):
/// 清透明底 → 渲染器 render → RGBA→ARGB32 预乘 blit → overload 红边
/// → 抚摸爱心。渲染器是共享实例,两窗口在同一 GTK 主线程串行执行,
/// 显示同一帧、动画同步;诊断计数(模块级 DIAG_N)两窗共用。
fn draw_pet_frame(cr: &cairo::Context, shared: &PetShared, _area: &gtk::DrawingArea) {
    // ── 阶段2(2026-08-08):清除 GTK 主题背景填充 ──
    // GTK3 在 draw 回调前已按主题 CSS 渲染 widget 背景(实测默认主题
    // (80,80,80) 灰;GTK_THEME=Adwaita:light 变 (78,201,176),实锤主题
    // 背景填充,与 WebKitGTK alpha 无关)。cairo 默认 Over 混合会把企鹅
    // blit 在主题背景之上 → 内容层不透明。修法:Operator::Source(直接
    // 覆盖目标含 alpha,不做混合)+ 全透明 paint 抹掉主题背景,再恢复
    // Over 正常混合贴企鹅像素(企鹅边缘有半透明像素,须保持混合)。
    // 弹出窗(普通 toplevel)同样靠这一步保证透明。
    cr.set_operator(cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0); // 全透明,直接覆盖(含 alpha)
    let _ = cr.paint(); // 清空整个 DrawingArea,主题背景不复存在
    cr.set_operator(cairo::Operator::Over); // 恢复默认混合,继续正常 blit

    let mut r = shared.renderer.lock().unwrap();
    let frame = r.render();
    let (fw, fh) = (frame.width as i32, frame.height as i32);
    if fw <= 0 || fh <= 0 {
        return;
    }

    // ── 诊断(阶段2 bug 排查):每 30 帧打印渲染统计 + area 实际尺寸 ──
    {
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
        Err(_) => return,
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
        Err(_) => return,
    };
    let _ = cr.set_source_surface(&surface, 0.0, 0.0);
    let _ = cr.paint();

    // ── overload 警示红边(P1-5):过载时宠物外圈画红色脉冲边框 ──
    // 两窗共用 overload_flag → 弹出窗同步警示(同一宠物)。
    if shared.overload_flag.load(Ordering::Relaxed) {
        let n = DIAG_N.load(Ordering::Relaxed);
        let alpha = 0.35 + 0.4 * (n as f64 / 8.0).sin().abs();
        cr.set_source_rgba(1.0, 0.15, 0.15, alpha);
        cr.set_line_width(4.0);
        cr.rectangle(2.0, 2.0, fw as f64 - 4.0, fh as f64 - 4.0);
        let _ = cr.stroke();
    }

    // ── 抚摸反馈爱心(P1-6):单击后宠物上方飘 3 颗爱心 ──
    // 两窗共用 hearts_until_ms → 弹出窗同步飘爱心(同一宠物)。
    let hearts_end = shared.hearts_until_ms.load(Ordering::Relaxed);
    if hearts_end > 0 {
        let now = now_millis();
        if now < hearts_end {
            let t_elapsed = (PET_INTERACT_MS - (hearts_end - now) as f64).max(0.0); // 已过 ms
            let mut min_x = fw;
            let mut min_y = fh;
            let mut max_x = -1i32;
            let mut max_y = -1i32;
            for y in 0..fh {
                for x in 0..fw {
                    if frame.pixels[((y * fw + x) * 4 + 3) as usize] > 0 {
                        if x < min_x { min_x = x; }
                        if x > max_x { max_x = x; }
                        if y < min_y { min_y = y; }
                        if y > max_y { max_y = y; }
                    }
                }
            }
            if max_x >= 0 {
                let cx = (min_x + max_x) as f64 / 2.0;
                let top_y = min_y as f64;
                for i in 0..HEART_COUNT {
                    let birth = 150.0 + i as f64 * HEART_STAGGER_MS;
                    let p = (t_elapsed - birth) / HEART_LIFE_MS; // 0..1 生命进度
                    if p < 0.0 || p >= 1.0 {
                        continue;
                    }
                    let sway = (p * std::f64::consts::TAU * 2.0 + i as f64 * 1.7).sin() * 14.0;
                    let y = top_y - 24.0 - p * HEART_RISE_PX;
                    let alpha = 0.9 * (p * std::f64::consts::PI).sin();
                    let r = 8.0 + i as f64 * 1.5;
                    draw_heart(cr, cx + sway + (i as f64 - 1.0) * 20.0, y, r, alpha);
                }
            }
        } else {
            shared.hearts_until_ms.store(0, Ordering::Relaxed); // 过期清理
        }
    }
}

/// 创建并显示弹出覆盖窗(第二个透明无边框窗口)。
/// 与主窗差异(刻意):**不提升 layer-shell**——layer 表面没有
/// xdg_toplevel move 请求,begin_move_drag 拖拽无法工作;普通 toplevel
/// 才能在 niri msg windows 里查到坐标(位置持久化的前提)。
/// 共享 PetShared 渲染器:两窗显示同一宠物,动画/爱心/红边同步。
/// 返回窗口句柄(调用方存入 PopupState 长期持有,防止 drop 销毁)。
fn spawn_popup_window(
    shared: &Arc<PetShared>,
    areas: &Arc<Mutex<Vec<gtk::DrawingArea>>>,
    roam: &Arc<roam::RoamState>,
    w: i32,
    h: i32,
) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(POPUP_TITLE);
    window.set_decorated(false); // 无边框
    window.set_skip_taskbar_hint(true); // 不出现在任务栏
    window.set_keep_above(true); // 置顶悬浮(配合 skip_taskbar → niri 判浮层,可 move-floating-window)
    window.set_app_paintable(true); // 应用自绘背景
    window.set_resizable(false);
    window.set_default_size(w, h);

    // RGBA visual:透明的前提(X11 下 GTK3 会强制 CSD 白标题栏,
    // 验证以 GDK_BACKEND=wayland 为准;与主窗一致)
    if let Some(screen) = gtk::prelude::GtkWindowExt::screen(&window) {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }

    let area = gtk::DrawingArea::new();
    area.set_size_request(w, h);
    window.add(&area);

    // draw 回调:与主窗共用绘制逻辑(共享渲染器,画面同步)
    let shared_draw = Arc::clone(shared);
    area.connect_draw(move |a, cr| {
        draw_pet_frame(cr, &shared_draw, a);
        glib::Propagation::Proceed
    });

    // 拖拽:按下左键 → 合成器接管(begin_move_drag,Wayland 正统)。
    // 弹出窗无单击/双击交互(拖拽是唯一用途),按下即拖,不区分
    // 单击/双击(参考阶段3 侦察结论:Pitfall 33 设计取舍)。
    // 漫游共存:拖拽开始暂停漫游(paused=true),释放恢复——避免
    // 「漫游 IPC 移动 + 合成器拖拽」叠加冲突(对齐 roam.ts 的
    // pointerdown/pointerup pause/resume 设计)。
    let win_drag = window.clone();
    let roam_drag = Arc::clone(roam);
    area.connect_button_press_event(move |_a, ev| {
        if ev.button() == 1 {
            let (rx, ry) = ev.root();
            win_drag.begin_move_drag(1, rx as i32, ry as i32, ev.time());
            roam_drag.set_paused(true); // 拖拽中暂停漫游
            eprintln!("[pet_render] 弹出窗拖拽开始(begin_move_drag),漫游暂停");
        }
        glib::Propagation::Stop
    });

    // 拖拽结束 → 恢复漫游 + 延迟查询实际位置并持久化(合成器接管
    // 移动后,niri 需要一点时间更新坐标;button-release 时位置已定,
    // 延迟 500ms 再查更稳)。漫游恢复后下个 tick 从 niri 重新查询
    // 当前位置,以拖拽后的新位置为起点继续走,无跳变。
    let roam_rel = Arc::clone(roam);
    area.connect_button_release_event(move |_a, _ev| {
        roam_rel.set_paused(false); // 拖拽结束恢复漫游
        save_popup_pos_delayed();
        glib::Propagation::Stop
    });

    // 注册进动画循环集合(主窗 timeout 每帧统一 queue_draw)
    areas.lock().unwrap().push(area);

    window.show_all();
    log::info!("[pet_render] 弹出窗已创建并显示({}x{})", w, h);
    window
}

/// 双击主窗触发的弹出窗切换(toggle):首次双击创建并显示
/// (恢复持久化位置);之后 show/hide 切换。窗口句柄保存在
/// popup_state 中长期持有(gtk::Window drop 引用归零会销毁窗口)。
fn toggle_popup(
    state: &Arc<Mutex<PopupState>>,
    shared: &Arc<PetShared>,
    areas: &Arc<Mutex<Vec<gtk::DrawingArea>>>,
    roam: &Arc<roam::RoamState>,
    w: i32,
    h: i32,
) {
    let mut st = state.lock().unwrap();
    match &st.window {
        Some(win) => {
            if st.visible {
                win.hide();
                st.visible = false;
                roam.stop(); // 收起 → 停止漫游
                eprintln!("[pet_render] 弹出窗收起(hide),漫游停止");
            } else {
                win.show_all();
                st.visible = true;
                eprintln!("[pet_render] 弹出窗重新显示");
                restore_popup_pos(w);
                roam.start(w as f64, h as f64); // 显示 → 启动漫游
            }
        }
        None => {
            let win = spawn_popup_window(shared, areas, roam, w, h);
            st.window = Some(win.clone());
            st.visible = true;
            restore_popup_pos(w);
            roam.start(w as f64, h as f64); // 首次弹出 → 启动漫游
        }
    }
}

/// 弹出窗显示后恢复持久化位置(逻辑坐标):
/// 有 popup-pos.json → 移动到保存位置;无 → 主窗右侧默认位置。
/// Wayland 下程序化移动唯一途径 = niri IPC move-floating-window
/// (增量式):先查询当前实际坐标(niri msg windows),算增量再移动。
/// 延迟 400ms 等窗口映射 + niri 完成初始定位(首次显示有异步时序)。
fn restore_popup_pos(main_w: i32) {
    let target = load_popup_pos().unwrap_or(PopupPos {
        x: POPUP_DEFAULT_X + main_w as f64 + POPUP_DEFAULT_GAP,
        y: POPUP_DEFAULT_X,
    });
    glib::timeout_add_local(Duration::from_millis(400), move || {
        move_popup_to(&target);
        glib::ControlFlow::Break
    });
}

/// 拖拽结束后延迟查询弹出窗实际位置并写入持久化文件。
fn save_popup_pos_delayed() {
    glib::timeout_add_local(Duration::from_millis(500), || {
        if let Some((_id, x, y)) = query_niri_window(POPUP_TITLE) {
            save_popup_pos(PopupPos { x, y });
            eprintln!("[pet_render] 弹出窗位置已持久化: ({x:.1}, {y:.1})");
        } else {
            eprintln!("[pet_render] 保存位置失败:未在 niri msg windows 找到弹出窗");
        }
        glib::ControlFlow::Break
    });
}

/// 弹出窗位置文件路径:~/.local/share/mo/popup-pos.json
/// (XDG_DATA_HOME 优先;未设置时默认 ~/.local/share)
fn popup_pos_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local").join("share")
        });
    base.join(POPUP_POS_DIR).join(POPUP_POS_FILE)
}

/// 写位置文件(失败仅告警,不影响主流程)
fn save_popup_pos(pos: PopupPos) {
    let path = popup_pos_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(&pos) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("[pet_render] 写入弹出窗位置失败: {e}");
            }
        }
        Err(e) => eprintln!("[pet_render] 序列化弹出窗位置失败: {e}"),
    }
}

/// 读位置文件(不存在/解析失败 → None,调用方用默认位置)
fn load_popup_pos() -> Option<PopupPos> {
    let path = popup_pos_path();
    let json = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<PopupPos>(&json) {
        Ok(pos) => Some(pos),
        Err(e) => {
            eprintln!("[pet_render] 解析弹出窗位置失败: {e}");
            None
        }
    }
}

/// 从 `niri msg windows` 文本输出解析指定标题窗口的
/// (niri 窗口 id, 逻辑 x, 逻辑 y)。
/// 仅浮动窗口有 Workspace-view position 字段(弹出窗带
/// keep_above+skip_taskbar 天然浮层);layer 表面(主窗)不在列表。
/// 文本输出格式(niri 26.04 实测):
///   Window ID 27:
///     Title: "Mo Pet (Rust) Popup"
///     ...
///     Workspace-view position: 858.0, 462.8
fn query_niri_window(title: &str) -> Option<(u64, f64, f64)> {
    let out = std::process::Command::new("niri")
        .args(["msg", "windows"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut cur_id: Option<u64> = None;
    let mut in_target = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Window ID ") {
            // "Window ID 27:" → 27
            cur_id = rest
                .trim_end_matches(':')
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok());
            in_target = false;
        } else if t.starts_with("Title:") && t.contains(title) {
            in_target = true;
        } else if in_target {
            if let Some(rest) = t.strip_prefix("Workspace-view position:") {
                let mut it = rest.split(',');
                let x: f64 = it.next()?.trim().parse().ok()?;
                let y: f64 = it.next()?.trim().parse().ok()?;
                return Some((cur_id?, x, y));
            }
            if t.is_empty() {
                in_target = false; // 窗口块结束(下一个窗口)
            }
        }
    }
    None
}

/// 经 niri IPC 把弹出窗增量移动到目标位置(绝对坐标换算增量)。
/// 实际 IPC 走 move_popup_by(漫游与位置恢复共用同一增量移动通道)。
fn move_popup_to(target: &PopupPos) {
    if let Some((_id, cx, cy)) = query_niri_window(POPUP_TITLE) {
        move_popup_by(target.x - cx, target.y - cy);
    } else {
        eprintln!("[pet_render] 未在 niri msg windows 找到弹出窗(可能尚未映射完成)");
    }
}

/// 经 niri IPC 增量移动弹出窗(dx/dy 为逻辑坐标增量,可为负)。
/// move-floating-window 仅对浮层窗口有效(弹出窗天然浮层 ✓);
/// --id 精确指定(缺省=焦点窗口,弹出窗未必有焦点)。
/// 返回是否成功(false = 窗口不在列表/命令失败,漫游据此决定是否停止)。
/// 漫游每片调用一次(增量语义天然适配「平滑步进」模型)。
fn move_popup_by(dx: f64, dy: f64) -> bool {
    if dx.abs() < 0.1 && dy.abs() < 0.1 {
        return true; // 无位移,跳过 IPC
    }
    let (id, _, _) = match query_niri_window(POPUP_TITLE) {
        Some(v) => v,
        None => {
            eprintln!("[pet_render] 未在 niri msg windows 找到弹出窗(可能尚未映射完成)");
            return false;
        }
    };
    let dx_s = format!("{dx:+.1}");
    let dy_s = format!("{dy:+.1}");
    let id_s = id.to_string();
    let out = std::process::Command::new("niri")
        .args([
            "msg",
            "action",
            "move-floating-window",
            "--id",
            id_s.as_str(),
            "-x",
            dx_s.as_str(),
            "-y",
            dy_s.as_str(),
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            eprintln!("[pet_render] 弹出窗增量移动 ({dx:+.1}, {dy:+.1})");
            true
        }
        Ok(o) => {
            eprintln!(
                "[pet_render] 移动弹出窗失败({}): {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            );
            false
        }
        Err(e) => {
            eprintln!("[pet_render] 移动弹出窗失败: {e}");
            false
        }
    }
}
