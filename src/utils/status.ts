// ============================================================
// 业务状态机纯函数 —— CPU 阈值 → 宠物状态(PetStatus)
// 纯函数、无平台依赖,与 DEV.md 7.3「状态机纯函数」方向一致,
// 后续迁入 mo-core 核心层时原样搬走即可。
// ============================================================
import type { PetStatus } from "../types";

/**
 * 根据 CPU 使用率映射宠物状态(阈值与 types 中 PetStatus 注释一致)
 * 负载越低越安静:overload → working → thinking → idle → sleeping
 */
export function getStatus(cpu: number): PetStatus {
  if (cpu > 80) return "overload";
  if (cpu > 50) return "working";
  if (cpu > 20) return "thinking";
  if (cpu > 5) return "idle";
  return "sleeping";
}
