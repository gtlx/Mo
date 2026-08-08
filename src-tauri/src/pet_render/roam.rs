// ============================================================
// pet_render/roam.rs —— 弹出窗桌面漫游(2026-08-09,方案D 最后一项)
// ============================================================
// 移植前端 src/services/roam.ts 逻辑到 Rust 侧;漫游对象 = 弹出窗
// (普通 toplevel,可被 niri IPC 移动)。行为对齐 roam.ts:
//   - 随机目标点:屏幕内留 60px 边距(屏幕尺寸查 `niri msg outputs`);
//   - 平滑步进:每 160ms 一片,固定步长 ≈50px/s(对齐 roam.ts 的
//     60fps×0.8px/帧 ≈ 48px/s,缓慢自然);
//   - 到达目标停留 5~15s 随机,再选新目标;
//   - 边界 clamp:位置越界拉回可视范围并重选目标;
//   - 拖拽暂停:弹出窗 begin_move_drag 期间 paused=true,释放恢复
//     (恢复后下个 tick 从 niri 重新查询当前位置,无跳变)。
// 移动唯一途径 = niri IPC `move-floating-window`(增量式):
//   - Wayland 下 GTK window.move_ 是 no-op,tauri move_window 同理
//     (阶段3 侦察实测,Pitfall 32);
//   - 主窗是 layer-shell Overlay 表面,不在 `niri msg windows` 列表,
//     查不到位置/ID → 无法被 niri 移动 → **漫游只作用于弹出窗**;
//   - 每片先 `niri msg windows` 查询弹出窗当前坐标(应用内拿不到
//     窗口位置),再算增量发 `move-floating-window`。
// ============================================================

use super::{demo_next_u64, move_popup_by, now_millis, query_niri_window, POPUP_TITLE};
use std::sync::Mutex;

/// 屏幕内边距(逻辑 px):目标点与边界保持距离,宠物不会贴边/越界(对齐 roam.ts)
const EDGE_MARGIN: f64 = 60.0;
/// 每片移动步长(px):160ms 一片 ≈ 50px/s(对齐 roam.ts 0.8px/帧)
const STEP_PX: f64 = 8.0;
/// 漫游时钟间隔(ms):任务要求 150~200ms 一步,取 160ms
pub const ROAM_INTERVAL_MS: u64 = 160;
/// 到达目标后的停留时间范围(ms):5~15s 随机(对齐 roam.ts)
const REST_MIN_MS: u64 = 5000;
const REST_MAX_MS: u64 = 15000;
/// 连续查询不到弹出窗的次数上限(≈1.3s):窗口被关闭/外部销毁时停止漫游,
/// 避免每片空跑 niri IPC;show 后映射期(几百 ms)的查询失败不计数到上限。
const QUERY_FAIL_LIMIT: u32 = 8;

/// 漫游内部状态(单 Mutex 整体保护:启停/暂停回调与漫游时钟都在
/// GTK 主线程,临界区极短,锁即够,不引入原子拆分)。
struct RoamInner {
    active: bool,               // 弹出窗显示中 → 漫游运行
    paused: bool,               // 拖拽中 → 暂停漫游
    target: Option<(f64, f64)>, // 当前目标点(逻辑坐标;None = 停留中或待选点)
    rest_until_ms: u64,         // 到达后停留截止(epoch ms;0 = 未进入停留)
    screen: (f64, f64),         // 屏幕逻辑分辨率(查 niri msg outputs)
    win: (f64, f64),            // 弹出窗尺寸(目标点计算:屏幕 - 窗口 - 边距)
    seed: u64,                  // LCG 随机种子(选目标/停留时长)
    fail_count: u32,            // 连续查询不到弹出窗的次数
}

/// 漫游状态句柄(Arc 共享:mod.rs 的 toggle/拖拽回调与漫游时钟共同持有)
pub struct RoamState {
    inner: Mutex<RoamInner>,
}

impl RoamState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RoamInner {
                active: false,
                paused: false,
                target: None,
                rest_until_ms: 0,
                screen: (1920.0, 1080.0), // 兜底,start 时重新查询
                win: (0.0, 0.0),
                seed: 0xDEAD_BEEF_CAFE_F00D,
                fail_count: 0,
            }),
        }
    }

    /// 弹出窗显示 → 启动漫游:重置目标/停留,重新同步屏幕与窗口尺寸。
    /// 窗口 show_all 后立即调用即可——映射期(几百 ms)查询失败由
    /// fail_count 机制兜底,不会误停。
    pub fn start(&self, win_w: f64, win_h: f64) {
        let mut st = self.inner.lock().unwrap();
        st.active = true;
        st.paused = false;
        st.target = None;
        st.rest_until_ms = 0; // 下个 tick 立即选目标
        st.win = (win_w, win_h);
        st.screen = query_screen_size();
        st.fail_count = 0;
        eprintln!(
            "[pet_render] 漫游启动:屏幕 {:.0}x{:.0},窗口 {:.0}x{:.0}(边距 {:.0}px)",
            st.screen.0, st.screen.1, win_w, win_h, EDGE_MARGIN
        );
    }

    /// 弹出窗收起 → 停止漫游(active=false,tick 直接返回)
    pub fn stop(&self) {
        let mut st = self.inner.lock().unwrap();
        if st.active {
            st.active = false;
            st.target = None;
            eprintln!("[pet_render] 漫游停止(弹出窗收起)");
        }
    }

    /// 拖拽期间暂停(true)/释放恢复(false)
    pub fn set_paused(&self, paused: bool) {
        let mut st = self.inner.lock().unwrap();
        if st.paused != paused {
            st.paused = paused;
            eprintln!(
                "[pet_render] 漫游{}",
                if paused {
                    "暂停(拖拽中)"
                } else {
                    "恢复"
                }
            );
        }
    }

    /// 单步漫游(漫游时钟每 ROAM_INTERVAL_MS 调用一次):
    /// 查询当前位置 → 停留期判断 → 选目标 → 平滑步进 → 边界 clamp
    /// → niri IPC 增量移动。参考 roam.ts 的 tick 逻辑,Rust 侧每步
    /// 从 niri 实时同步窗口位置(应用内拿不到,拖拽恢复也不跳变)。
    pub fn tick(&self) {
        let mut st = self.inner.lock().unwrap();
        if !st.active || st.paused {
            return;
        }

        // ① 从 niri 查询弹出窗当前位置(Workspace-view position,逻辑坐标)
        let (cx, cy) = match query_niri_window(POPUP_TITLE) {
            Some((_id, cx, cy)) => (cx, cy),
            None => {
                st.fail_count += 1;
                if st.fail_count >= QUERY_FAIL_LIMIT {
                    eprintln!(
                        "[pet_render] 漫游停止:连续 {} 次查询不到弹出窗(窗口已关闭?)",
                        QUERY_FAIL_LIMIT
                    );
                    st.active = false;
                }
                return; // 窗口可能尚未映射完成,下个 tick 再试
            }
        };
        st.fail_count = 0;

        let now = now_millis();

        // ② 到达后的停留期:原地等待,rest_until 之后才重新选点
        if st.target.is_none() && now < st.rest_until_ms {
            return;
        }

        // ③ 无目标且过了停留期 → 屏幕内随机选一个目标点
        let (tx, ty) = match st.target {
            Some(t) => t,
            None => {
                st.target = Some(pick_target(st.screen, st.win, &mut st.seed));
                return; // 下一片开始移动
            }
        };

        // ④ 朝目标平滑步进(最后一小步只走剩余距离,不越过头)
        let (dx, dy) = (tx - cx, ty - cy);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1.0 {
            // 已到达 → 进入 5~15s 随机停留
            st.rest_until_ms = now + rest_duration_ms(&mut st.seed);
            st.target = None;
            eprintln!(
                "[pet_render] 漫游:到达目标 → 停留 {:.1}s",
                (st.rest_until_ms - now) as f64 / 1000.0
            );
            return;
        }
        let step = STEP_PX.min(dist);
        let (vx, vy) = (dx / dist * step, dy / dist * step);
        let (mut nx, mut ny) = (cx + vx, cy + vy);

        // ⑤ 边界 clamp:越界拉回可视范围并重选目标(下个点自然在屏内侧)
        let (min_x, min_y) = (EDGE_MARGIN, EDGE_MARGIN);
        let (max_x, max_y) = (
            EDGE_MARGIN.max(st.screen.0 - st.win.0 - EDGE_MARGIN),
            EDGE_MARGIN.max(st.screen.1 - st.win.1 - EDGE_MARGIN),
        );
        let mut bounced = false;
        if nx < min_x {
            nx = min_x;
            bounced = true;
        }
        if nx > max_x {
            nx = max_x;
            bounced = true;
        }
        if ny < min_y {
            ny = min_y;
            bounced = true;
        }
        if ny > max_y {
            ny = max_y;
            bounced = true;
        }
        if bounced {
            st.target = None;
            eprintln!("[pet_render] 漫游:越界 clamp → ({nx:.0}, {ny:.0}),重选目标");
        }

        // ⑥ 增量移动(clamp 修正后的实际位移,保证位置精确落在边界内);
        //    移动失败(窗口消失等)→ 停止漫游,避免空转
        if !move_popup_by(nx - cx, ny - cy) {
            st.active = false;
        }
    }
}

/// 在屏幕内(留 EDGE_MARGIN 边距)随机选一个目标点(逻辑坐标)。
/// 窗口尺寸参与计算:目标点 = 窗口左上角可到达范围(屏幕 - 窗口 - 边距)。
fn pick_target(screen: (f64, f64), win: (f64, f64), seed: &mut u64) -> (f64, f64) {
    let min_x = EDGE_MARGIN;
    let min_y = EDGE_MARGIN;
    let max_x = EDGE_MARGIN.max(screen.0 - win.0 - EDGE_MARGIN);
    let max_y = EDGE_MARGIN.max(screen.1 - win.1 - EDGE_MARGIN);
    // LCG 取 0..10000/10000 → 0..1 随机比例,映射到 [min, max]
    let x = min_x + (demo_next_u64(seed) % 10000) as f64 / 10000.0 * (max_x - min_x);
    let y = min_y + (demo_next_u64(seed) % 10000) as f64 / 10000.0 * (max_y - min_y);
    (x, y)
}

/// 到达后停留时长:5~15s 随机(ms)
fn rest_duration_ms(seed: &mut u64) -> u64 {
    REST_MIN_MS + demo_next_u64(seed) % (REST_MAX_MS - REST_MIN_MS)
}

/// 从 `niri msg outputs` 文本解析第一个输出的逻辑分辨率(逻辑坐标,
/// 与 niri msg windows 的 Workspace-view position 同一坐标系)。
/// 多显示器只取第一个输出(主屏)——目标点/边界约束在首屏内;
/// niri 不可用或解析失败 → 回退 1920x1080(桌面常见分辨率,保守边界)。
/// 文本格式(niri 26.04 实测):
///   Output "..." (eDP-1)
///     ...
///     Logical size: 1920x1080
fn query_screen_size() -> (f64, f64) {
    let out = std::process::Command::new("niri")
        .args(["msg", "outputs"])
        .output();
    if let Ok(o) = out {
        let text = String::from_utf8_lossy(&o.stdout);
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("Logical size:") {
                let mut it = rest.trim().split('x');
                if let (Some(w), Some(h)) = (it.next(), it.next()) {
                    if let (Ok(w), Ok(h)) = (w.parse::<f64>(), h.parse::<f64>()) {
                        return (w, h);
                    }
                }
            }
        }
    }
    eprintln!("[pet_render] 查询屏幕尺寸失败,回退 1920x1080");
    (1920.0, 1080.0)
}
