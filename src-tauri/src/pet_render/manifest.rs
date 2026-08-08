// ============================================================
// pet_render/manifest.rs —— pet.json 协议解析(serde)
//
// 与前端 src/renderers/types.ts 的 PetManifest 对齐,字段名
// camelCase(与 pet.json 一致),全部可选字段带默认值兜底,
// 与前端 SpriteRenderer 的默认规格保持一致。
// ============================================================

use serde::Deserialize;
use std::collections::HashMap;

/// 宠物清单(对应 assets/pets/<id>/pet.json)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    /// 宠物唯一 id(如 qqpet-codex)
    pub id: String,
    /// 展示名
    pub display_name: String,
    /// 描述
    pub description: Option<String>,
    /// 渲染类型:渲染器工厂按此分发(sprite/live2d/spine)
    #[serde(rename = "type")]
    pub render_type: String,
    /// 精灵图路径(相对宠物目录;内嵌模式下忽略)
    pub spritesheet_path: Option<String>,
    /// 单帧宽度(px),默认 192
    pub frame_width: Option<u32>,
    /// 单帧高度(px),默认 208
    pub frame_height: Option<u32>,
    /// 精灵图每行帧数(列数),默认 8
    pub frames_per_row: Option<u32>,
    /// 每个状态最多播放帧数,默认取 framesPerRow
    pub frames_per_state: Option<u32>,
    /// 状态 → 行号映射,默认 Codex 九行分类法(缺省时按空表处理)
    pub state_rows: Option<HashMap<String, u32>>,
    /// 单个状态完整循环时长(ms),默认 1100
    pub loop_ms: Option<u64>,
    /// 显示缩放(相对原始帧尺寸),默认 0.33
    pub scale: Option<f64>,
}

impl PetManifest {
    pub fn frame_width(&self) -> u32 {
        self.frame_width.unwrap_or(192)
    }
    pub fn frame_height(&self) -> u32 {
        self.frame_height.unwrap_or(208)
    }
    pub fn frames_per_row(&self) -> u32 {
        self.frames_per_row.unwrap_or(8)
    }
    pub fn frames_per_state(&self) -> u32 {
        self.frames_per_state
            .unwrap_or_else(|| self.frames_per_row())
    }
    pub fn loop_ms(&self) -> u64 {
        self.loop_ms.unwrap_or(1100)
    }
    pub fn scale(&self) -> f64 {
        self.scale.unwrap_or(0.33)
    }
    /// 状态行映射(无声明时返回 None,渲染器查不到行回退行号 0)
    pub fn state_rows(&self) -> Option<&HashMap<String, u32>> {
        self.state_rows.as_ref()
    }
    /// 渲染尺寸(帧尺寸 × 缩放,向上取整)
    pub fn render_size(&self) -> (u32, u32) {
        (
            (self.frame_width() as f64 * self.scale()).round() as u32,
            (self.frame_height() as f64 * self.scale()).round() as u32,
        )
    }
}

/// 从 JSON 字符串解析宠物清单
pub fn parse_manifest(json: &str) -> Result<PetManifest, String> {
    serde_json::from_str(json).map_err(|e| format!("pet.json 解析失败: {e}"))
}
