// ============================================================
// pet_render/sprite.rs —— 精灵图渲染器(SpriteRenderer)
//
// 方案 D 核心:把 spritesheet.png 按 pet.json 协议裁帧,绘制到
// RGBA8 像素缓冲(透明窗口的内容层),完全绕开 WebKitGTK 的
// alpha 合成硬伤。行为与前端 src/renderers/sprite-renderer.ts
// 对齐(同一套「自然动效」四要素):
//   1. 分层状态机:业务状态 → 精灵图状态行 → 帧循环
//   2. 微动效叠加:呼吸(scaleY 正弦)+ 眨眼(周期压扁)
//   3. 平滑过渡:状态切换淡入 180ms,行不变只换节奏不重启
//   4. 帧节奏自然化:easeInOutSine 映射帧位置(起步/收步稍停)
//
// 时间模型:纯内部时钟(tick 喂 dt),不依赖外部时间源——
// 由 Rust 动画循环(glib timeout)驱动,解决之前前端 rAF 冻结问题。
// ============================================================

use crate::pet_render::manifest::PetManifest;
use crate::pet_render::renderer::{PetRenderer, RenderFrame};
use image::RgbaImage;
use std::collections::HashMap;

// ---------- 常量与配置(与前端 sprite-renderer.ts 对齐) ----------

/// 状态切换淡入时长(ms)
const FADE_IN_MS: f64 = 180.0;
/// 眨眼参数:周期随机区间 / 闭合时长
const BLINK_MIN_MS: f64 = 2600.0;
const BLINK_MAX_MS: f64 = 5200.0;
const BLINK_CLOSE_MS: f64 = 150.0;

/// 业务状态 → 精灵图状态行名(Codex 分类法,与前端 STATUS_TO_ROW 一致)。
/// 额外支持 "waving":对应前端 greet() 直切 waving 行(挥手演示/互动)。
const STATUS_TO_ROW: [(&str, &str); 6] = [
    ("sleeping", "idle"),
    ("idle", "idle"),
    ("thinking", "waiting"),
    ("working", "running"),
    ("overload", "jumping"),
    ("waving", "waving"),
];

/// 各业务状态的播放参数(loopScale:循环时长倍率;breatheAmp:呼吸幅度;
/// breatheMs:呼吸周期;blink:是否允许眨眼)——与前端 STATUS_TUNING 一致
#[derive(Clone, Copy)]
struct StatusTuning {
    loop_scale: f64,
    breathe_amp: f64,
    breathe_ms: f64,
    blink: bool,
}

const TUNING_IDLE: StatusTuning = StatusTuning { loop_scale: 1.0, breathe_amp: 0.022, breathe_ms: 2600.0, blink: true };
const TUNING_SLEEPING: StatusTuning = StatusTuning { loop_scale: 1.6, breathe_amp: 0.035, breathe_ms: 3600.0, blink: false };
const TUNING_THINKING: StatusTuning = StatusTuning { loop_scale: 1.2, breathe_amp: 0.016, breathe_ms: 3000.0, blink: true };
const TUNING_WORKING: StatusTuning = StatusTuning { loop_scale: 0.9, breathe_amp: 0.012, breathe_ms: 2200.0, blink: false };
const TUNING_OVERLOAD: StatusTuning = StatusTuning { loop_scale: 0.7, breathe_amp: 0.02, breathe_ms: 1600.0, blink: false };

fn tuning_for(status: &str) -> StatusTuning {
    match status {
        "sleeping" => TUNING_SLEEPING,
        "thinking" => TUNING_THINKING,
        "working" => TUNING_WORKING,
        "overload" => TUNING_OVERLOAD,
        _ => TUNING_IDLE,
    }
}

/// 业务状态 → 状态行名;未知状态回退 idle
fn row_name_for(status: &str) -> &'static str {
    for (k, v) in STATUS_TO_ROW {
        if k == status {
            return v;
        }
    }
    "idle"
}
/// 缓动曲线:easeInOutSine,用于帧位置映射(起步/收步放缓)
fn ease_in_out_sine(t: f64) -> f64 {
    -(f64::cos(std::f64::consts::PI * t) - 1.0) / 2.0
}

/// 线性插值
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// 极简 xorshift64 随机数(零依赖,用于眨眼/初相随机化)
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        // 种子 = 时间戳 + 固定盐,避免所有宠物同相位
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15)
            ^ 0xDEADBEEF;
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// 返回 [lo, hi) 区间随机浮点
    fn gen_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() as f64 / u64::MAX as f64) * (hi - lo)
    }
}

// ---------- 渲染器主体 ----------

pub struct SpriteRenderer {
    manifest: PetManifest,
    /// 解码后的精灵图(整张)
    spritesheet: RgbaImage,
    /// 渲染输出尺寸(帧尺寸 × 缩放)
    out_w: u32,
    out_h: u32,
    /// 行名 → 该行有效帧数(自动检测,遇到第一个空帧截断)
    valid_frames: HashMap<String, u32>,

    // 分层状态机
    /// 当前业务状态
    status: String,
    /// 当前精灵图状态行名
    row_name: String,
    /// 当前行号
    row_index: u32,
    /// 当前状态开始播放的内部时间(ms)
    state_start: f64,

    // 微动效
    /// 呼吸相位(随机初相避免同频)
    breathe_phase: f64,
    /// 下一次眨眼时间(内部时钟 ms)
    next_blink_at: f64,
    /// 眨眼进行中的起始时间(-1 = 未在眨眼)
    blink_start: f64,

    /// 内部时钟(ms,由 tick 累计)
    now_ms: f64,
    rng: Rng,
}

impl SpriteRenderer {
    /// 创建精灵图渲染器。
    /// `spritesheet` 为解码后的精灵图像素;`scale_override` 为
    /// 可选显示缩放覆盖(env MO_PET_SCALE,验证/调试用),None 用协议值。
    pub fn new(
        manifest: PetManifest,
        spritesheet: RgbaImage,
        scale_override: Option<f64>,
    ) -> Result<Self, String> {
        let scale = scale_override.unwrap_or(manifest.scale());
        let (out_w, out_h) = (
            (manifest.frame_width() as f64 * scale).round().max(1.0) as u32,
            (manifest.frame_height() as f64 * scale).round().max(1.0) as u32,
        );

        let mut rng = Rng::new();
        let mut renderer = Self {
            manifest,
            spritesheet,
            out_w,
            out_h,
            valid_frames: HashMap::new(),
            status: "idle".to_string(),
            row_name: "idle".to_string(),
            row_index: 0,
            state_start: 0.0,
            breathe_phase: rng.gen_range(0.0, std::f64::consts::TAU),
            next_blink_at: rng.gen_range(BLINK_MIN_MS, BLINK_MAX_MS),
            blink_start: -1.0,
            now_ms: 0.0,
            rng,
        };

        // 自动检测每行有效帧数(与前端 detectValidFrames 一致:
        // 越界或空帧即截断)
        renderer.detect_valid_frames();
        Ok(renderer)
    }

    /// 检测每行有效帧数:跳过空帧(素材行内帧数不足时自动适配)。
    /// 帧内容判定:帧区域内存在 alpha > 16 的像素即视为有效。
    fn detect_valid_frames(&mut self) {
        let fw = self.manifest.frame_width();
        let fh = self.manifest.frame_height();
        let sheet_w = self.spritesheet.width();
        let sheet_h = self.spritesheet.height();
        let limit = self
            .manifest
            .frames_per_state()
            .min(self.manifest.frames_per_row());

        let rows = self.manifest.state_rows().cloned().unwrap_or_default();
        for (name, row) in rows {
            let mut count = 0u32;
            'col: for c in 0..limit {
                let sx = c * fw;
                let sy = row * fh;
                // 内容超过帧边界视为空帧
                if sx + fw > sheet_w || sy + fh > sheet_h {
                    break;
                }
                // 全像素扫描 alpha(帧尺寸小,一次性成本可忽略)
                for py in sy..sy + fh {
                    for px in sx..sx + fw {
                        if self.spritesheet.get_pixel(px, py)[3] > 16 {
                            count += 1;
                            continue 'col;
                        }
                    }
                }
                break; // 遇到第一个空帧即截断(帧按顺序排列)
            }
            self.valid_frames.insert(name, count);
        }
    }

    /// 当前行的有效帧数(未检测到按上限)
    fn frame_count_for(&self, row_name: &str) -> u32 {
        match self.valid_frames.get(row_name) {
            Some(&n) if n > 0 => n,
            _ => self
                .manifest
                .frames_per_state()
                .min(self.manifest.frames_per_row()),
        }
    }

    /// 当前状态的行号(查不到行回退 0)
    fn row_index_for(&self, target: &str) -> u32 {
        self.manifest
            .state_rows()
            .and_then(|m| m.get(target).copied())
            .unwrap_or(0)
    }

    /// 双线性采样精灵图坐标 (x, y)(浮点坐标,越界返回 None)
    fn sample(&self, x: f64, y: f64) -> Option<(u8, u8, u8, u8)> {
        let w = self.spritesheet.width() as f64;
        let h = self.spritesheet.height() as f64;
        if x < 0.0 || y < 0.0 || x >= w || y >= h {
            return None;
        }
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(self.spritesheet.width() - 1);
        let y1 = (y0 + 1).min(self.spritesheet.height() - 1);
        let fx = x - x0 as f64;
        let fy = y - y0 as f64;

        let p00 = self.spritesheet.get_pixel(x0, y0);
        let p10 = self.spritesheet.get_pixel(x1, y0);
        let p01 = self.spritesheet.get_pixel(x0, y1);
        let p11 = self.spritesheet.get_pixel(x1, y1);

        let lerp2 = |a: &[u8; 4], b: &[u8; 4]| -> [f64; 4] {
            [0, 1, 2, 3].map(|i| lerp(a[i] as f64, b[i] as f64, fx))
        };
        let top = lerp2(&[p00[0], p00[1], p00[2], p00[3]], &[p10[0], p10[1], p10[2], p10[3]]);
        let bot = lerp2(&[p01[0], p01[1], p01[2], p01[3]], &[p11[0], p11[1], p11[2], p11[3]]);
        let r = lerp(top[0], bot[0], fy) as u8;
        let g = lerp(top[1], bot[1], fy) as u8;
        let b = lerp(top[2], bot[2], fy) as u8;
        let a = lerp(top[3], bot[3], fy) as u8;
        Some((r, g, b, a))
    }
}

impl PetRenderer for SpriteRenderer {
    fn size(&self) -> (u32, u32) {
        (self.out_w, self.out_h)
    }

    fn set_state(&mut self, state: &str) {
        if state == self.status {
            return;
        }
        self.status = state.to_string();
        let target = row_name_for(state);
        let row = self.row_index_for(target);
        // 行变化 → 切换状态行并重启循环;行相同(如 sleeping/idle
        // 都用 idle 行)只更新节奏参数,循环不中断,过渡更顺滑
        if row != self.row_index {
            self.row_index = row;
            self.row_name = target.to_string();
            self.state_start = self.now_ms;
        }
        // 状态切换不重置眨眼计时(否则频繁切换会无限推迟眨眼);
        // 但目标状态禁眨眼时,立即中止进行中的眨眼
        if !tuning_for(state).blink {
            self.blink_start = -1.0;
        }
    }

    fn tick(&mut self, dt_ms: f64) {
        self.now_ms += dt_ms.max(0.0);
    }

    fn render(&mut self) -> RenderFrame {
        let (w, h) = (self.out_w, self.out_h);
        let mut frame = RenderFrame::new(w, h);

        let fw = self.manifest.frame_width();
        let fh = self.manifest.frame_height();
        let scale = (w as f64) / fw as f64; // 实际生效缩放

        // ---- 1. 计算当前帧(缓动映射,起步/收步稍停) ----
        let tuning = tuning_for(&self.status);
        let frame_count = self.frame_count_for(&self.row_name);
        let loop_ms = (self.manifest.loop_ms() as f64 * tuning.loop_scale).max(1.0);
        let elapsed = self.now_ms - self.state_start;
        let t = ((elapsed % loop_ms) + loop_ms) % loop_ms / loop_ms;
        let eased = ease_in_out_sine(t);
        let mut frame_idx = (eased * frame_count as f64).floor() as u32;
        if frame_idx >= frame_count {
            frame_idx = frame_count - 1; // 边界保护
        }

        // ---- 2. 微动效变换(呼吸 + 眨眼) ----
        // 呼吸:scaleY 正弦起伏,幅度来自状态参数
        let breathe = 1.0
            + tuning.breathe_amp
                * f64::sin(
                    std::f64::consts::TAU * self.now_ms / tuning.breathe_ms + self.breathe_phase,
                );
        // 眨眼:到点触发,闭合期内 scaleY 压扁(瞬间闭眼又睁开)
        let mut blink = 1.0;
        if self.blink_start >= 0.0 {
            let bp = (self.now_ms - self.blink_start) / BLINK_CLOSE_MS;
            if bp >= 1.0 {
                self.blink_start = -1.0; // 眨眼结束
                self.next_blink_at = self.rng.gen_range(BLINK_MIN_MS, BLINK_MAX_MS) + self.now_ms;
            } else {
                // 前半闭眼、后半睁眼,整体 150ms
                blink = if bp < 0.5 {
                    lerp(1.0, 0.08, bp * 2.0)
                } else {
                    lerp(0.08, 1.0, (bp - 0.5) * 2.0)
                };
            }
        } else if tuning.blink && self.now_ms >= self.next_blink_at {
            self.blink_start = self.now_ms;
        }

        // ---- 3. 状态切换淡入(从 0 渐显,不硬切) ----
        let since_switch = self.now_ms - self.state_start;
        let alpha = if since_switch < FADE_IN_MS {
            since_switch / FADE_IN_MS
        } else {
            1.0
        };

        // ---- 4. 逐像素绘制(反向映射 + 双线性采样) ----
        // 目标像素 → 源帧坐标:先按缩放还原到帧内坐标,再按
        // 底部锚做 scaleY(呼吸/眨眼)反向压缩;双线性采样平滑。
        let src_x0 = frame_idx * fw;
        let src_y0 = self.row_index * fh;
        let scale_y = breathe * blink; // 组合垂直变换(底部锚)
        let pixels = &mut frame.pixels;

        for y in 0..h {
            let y_un = y as f64 / scale; // 未缩放帧内坐标
            // 底部锚压缩:底部(y=fh)不动,顶部按 1/scale_y 拉伸源区域
            let y_src = fh as f64 - (fh as f64 - y_un) / scale_y;
            for x in 0..w {
                let x_un = x as f64 / scale;
                let sx = x_un + src_x0 as f64;
                let sy = y_src + src_y0 as f64;
                if let Some((r, g, b, a)) = self.sample(sx, sy) {
                    let i = ((y * w + x) * 4) as usize;
                    let out_a = (a as f64 * alpha) as u8;
                    if out_a > 0 {
                        pixels[i] = r;
                        pixels[i + 1] = g;
                        pixels[i + 2] = b;
                        pixels[i + 3] = out_a;
                    }
                }
            }
        }

        frame
    }
}
