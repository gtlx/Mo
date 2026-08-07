// ============================================================
// 共享类型定义
// 说明:前后端分层 —— 类型统一从这里导出,组件 / hooks / services 均引用本模块
// ============================================================

/**
 * 系统信息快照(与 Rust 端 get_system_info 命令返回结构一一对应)
 * - cpu_usage:      CPU 使用率,百分比 0~100
 * - memory_used:    已用内存,单位字节
 * - memory_total:   物理内存总量,单位字节
 * - memory_percent: 内存使用率,百分比 0~100
 */
export interface SystemInfo {
  cpu_usage: number;
  memory_used: number;
  memory_total: number;
  memory_percent: number;
}

/**
 * 宠物状态(由 CPU 使用率阈值映射而来,驱动宠物 5 态动画)
 * - sleeping: 低负载休眠(≤5%)      - idle: 空闲(≤20%)
 * - thinking: 思考中(≤50%)         - working: 工作中(≤80%)
 * - overload: 过载警示(>80%)
 */
export type PetStatus = "sleeping" | "idle" | "thinking" | "working" | "overload";
