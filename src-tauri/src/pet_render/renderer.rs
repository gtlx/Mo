// ============================================================
// pet_render/renderer.rs —— 宠物渲染器统一接口
//
// 方案 D 的渲染抽象层(与前端 src/renderers/types.ts 的
// PetRenderer 接口对齐):宠物 = 素材目录 + pet.json 协议,
// 渲染器只认协议,不硬编码宠物长相。
//
// 演进预留:
//   type: "sprite"  → SpriteRenderer(本阶段实现,RGBA 像素缓冲)
//   type: "live2d"  → Live2dRenderer(将来实现同 PetRenderer 接口)
//   type: "spine"   → SpineRenderer(将来实现同 PetRenderer 接口)
// 窗口/动画循环只依赖 PetRenderer,新增渲染器无需改窗口代码。
// ============================================================

/// 单帧渲染输出:RGBA8 像素缓冲(straight alpha,非预乘)。
/// 由窗口层负责转换成 GTK/cairo 需要的 ARGB32 预乘格式。
/// Clone:交叉淡入需要缓存「切换前最后一帧」的快照。
#[derive(Clone)]
pub struct RenderFrame {
    /// 帧宽度(px)
    pub width: u32,
    /// 帧高度(px)
    pub height: u32,
    /// 像素数据,长度 = width * height * 4,顺序 R,G,B,A
    pub pixels: Vec<u8>,
}

impl RenderFrame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0u8; (width * height * 4) as usize],
        }
    }

    /// 清空为全透明(透明窗口的关键:未绘制区域必须 alpha=0)
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }
}

/// 宠物渲染器统一接口(Live2D / Spine 将来实现同一接口)。
///
/// 线程模型:渲染器由 GTK 主线程的 draw 回调与动画 timeout 串行访问,
/// 外层用 Arc<Mutex<>> 包裹,接口本身不要求内部加锁。
pub trait PetRenderer: Send {
    /// 渲染输出尺寸(逻辑像素,窗口尺寸由此决定)。
    /// 注意:尺寸是固定的(由素材帧尺寸 × 缩放决定),渲染中不改变。
    fn size(&self) -> (u32, u32);

    /// 设置业务状态(如 "idle"/"working"/"overload"),内部映射到
    /// 精灵图状态行并处理平滑过渡;未知状态回退 idle。
    fn set_state(&mut self, state: &str);

    /// 推进时间(dt_ms:距上一帧的毫秒数)。
    /// 动画循环每帧调用;渲染器内部用累计时间驱动帧循环/呼吸/眨眼,
    /// 不依赖任何外部时钟源(解决之前前端 rAF 冻结的问题)。
    fn tick(&mut self, dt_ms: f64);

    /// 渲染当前帧到 RGBA 缓冲并返回。
    /// 透明像素 alpha=0;窗口层直接 blit 到 ARGB 表面。
    fn render(&mut self) -> RenderFrame;
}
