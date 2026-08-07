// ============================================================
// 渲染器工厂 —— 按 manifest.type 分发对应渲染器
//
// 为 Live2D / Spine 预留的口子:
//   将来新增 live2d-renderer.ts / spine-renderer.ts,实现同一
//   PetRenderer 接口,在此工厂的 switch 中注册即可;
//   Pet.tsx 只依赖 createRenderer + PetRenderer,无需任何改动。
//
// 分发策略:
//   - type === "sprite" → SpriteRenderer(本阶段唯一实现)
//   - type === "live2d" / "spine" → 尚未实现,告警并回退 sprite
//     (保证未来 manifest 升级时旧版应用不崩)
//   - 未知 type → 告警并回退 sprite
// ============================================================

import type { PetManifest, PetRenderer } from "./types";
import { SpriteRenderer } from "./sprite-renderer";

/**
 * 按 manifest.type 创建对应渲染器。
 * @param manifest 宠物清单(对应 pet.json,已解析为 PetManifest)
 * @returns 渲染器实例(未知/未实现类型回退到 SpriteRenderer,带告警)
 */
export function createRenderer(manifest: PetManifest): PetRenderer {
  switch (manifest.type) {
    case "sprite":
      return new SpriteRenderer(manifest);
    case "live2d":
    case "spine":
      // 预留:接入 Live2D / Spine 引擎后在此实例化对应渲染器
      console.warn(
        `[renderer] ${manifest.type} 渲染器尚未实现,暂时回退到精灵图渲染器(${manifest.id})`,
      );
      return new SpriteRenderer(manifest);
    default: {
      // 未知类型兜底:不崩溃,回退 sprite 并提示
      const unknown = (manifest as { type?: string }).type ?? "<未声明>";
      console.warn(`[renderer] 未知渲染类型 "${unknown}",回退到精灵图渲染器`);
      return new SpriteRenderer(manifest);
    }
  }
}
