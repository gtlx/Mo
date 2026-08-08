// ============================================================
// pet_render/factory.rs —— 渲染器工厂(按 pet.json 的 type 分发)
//
// 与前端 src/renderers/index.ts 的 createRenderer 对齐:
//   type: "sprite"  → SpriteRenderer(本阶段实现)
//   type: "live2d"  → 未实现,报错(不再像前端那样静默回退,
//                     因为 Rust 侧没有「画布」概念,错了就该暴露)
//   type: "spine"   → 同上
//
// 素材加载优先级:
//   1. env MO_PET_DIR=<目录> → 从磁盘读 <目录>/pet.json + spritesheet.png
//      (素材可更换:换目录不改代码,呼应「宠物 = 一个目录」设计)
//   2. 默认 → 编译期内嵌 qqpet-codex(include_str!/include_bytes!,
//      发布后素材随二进制走,不依赖运行时路径)
// ============================================================

use crate::pet_render::manifest::{parse_manifest, PetManifest};
use crate::pet_render::renderer::PetRenderer;
use crate::pet_render::sprite::SpriteRenderer;
use image::RgbaImage;
use std::path::PathBuf;

/// 内嵌默认宠物:qqpet-codex(首发宠物,素材与前端 src/assets/pets 同源)
const EMBEDDED_MANIFEST: &str = include_str!("../../../src/assets/pets/qqpet-codex/pet.json");
const EMBEDDED_SHEET: &[u8] = include_bytes!("../../../src/assets/pets/qqpet-codex/spritesheet.png");

/// 创建渲染器:解析 manifest + 解码精灵图 + 按 type 分发。
/// 显示缩放可被 env MO_PET_SCALE 覆盖(验证/调试用)。
pub fn create_renderer() -> Result<Box<dyn PetRenderer>, String> {
    // 素材来源:env MO_PET_DIR 优先,否则内嵌
    let (manifest, sheet) = match std::env::var("MO_PET_DIR") {
        Ok(dir) => load_from_dir(&dir)?,
        Err(_) => {
            let manifest = parse_manifest(EMBEDDED_MANIFEST)?;
            let sheet = decode_png(EMBEDDED_SHEET, "内嵌 spritesheet.png")?;
            (manifest, sheet)
        }
    };

    // env MO_PET_SCALE 覆盖显示缩放(验证/调试用,None 用协议值)
    let scale_override = std::env::var("MO_PET_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok());

    match manifest.render_type.as_str() {
        "sprite" => Ok(Box::new(SpriteRenderer::new(
            manifest,
            sheet,
            scale_override,
        )?)),
        other => Err(format!(
            "渲染类型 \"{other}\" 尚未实现(Rust 侧当前仅支持 sprite;\
             live2d/spine 属预留协议)"
        )),
    }
}

/// 从磁盘目录加载宠物素材(素材可更换入口)
fn load_from_dir(dir: &str) -> Result<(PetManifest, RgbaImage), String> {
    let base = PathBuf::from(dir);
    let json_path = base.join("pet.json");
    let sheet_path = base.join("spritesheet.png");

    let json = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("读取 {} 失败: {e}", json_path.display()))?;
    let manifest = parse_manifest(&json)?;

    let png = std::fs::read(&sheet_path)
        .map_err(|e| format!("读取 {} 失败: {e}", sheet_path.display()))?;
    let sheet = decode_png(&png, &sheet_path.display().to_string())?;
    Ok((manifest, sheet))
}

/// PNG 解码为 RGBA8 图像
fn decode_png(bytes: &[u8], label: &str) -> Result<RgbaImage, String> {
    image::load_from_memory(bytes)
        .map(|img| img.to_rgba8())
        .map_err(|e| format!("精灵图解码失败({label}): {e}"))
}
