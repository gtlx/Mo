// ============================================================
// 首发宠物:qqpet-codex(QQ 企鹅 Codex 定制)
// 素材来源:~/.hermes/pets/qqpet-codex/(spritesheet.png + pet.json)
// 入口统一导出:渲染器只认「宠物清单 + 素材目录」,换宠物换目录即可。
// ============================================================

import rawManifest from "./pet.json";
import spritesheetUrl from "./spritesheet.png";
import type { PetManifest } from "../../../renderers/types";

/** qqpet-codex 宠物清单(pet.json 已补全完整协议字段) */
export const qqpetCodexManifest: PetManifest = {
  ...rawManifest,
  // 把 pet.json 里的相对路径解析成 vite 打包后的资源 URL
  spritesheetPath: spritesheetUrl,
} as PetManifest;
