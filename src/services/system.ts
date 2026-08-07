// ============================================================
// 系统服务层 —— 前端唯一的 Tauri invoke 封装
// 铁律:组件 / hooks 禁止直接调用 @tauri-apps/api,一律经本层转发
// ============================================================
import { invoke } from "@tauri-apps/api/core";
import type { SystemInfo } from "../types";

// ---------- 环境检测与 web 调试 mock ----------

/**
 * 是否为 Tauri 运行时。
 * 浏览器直接访问 vite devUrl(http://localhost:1420)时,window.__TAURI_INTERNALS__
 * 不存在(invoke 会报 Cannot read properties of undefined),此时走 mock 数据,
 * 保证 web 调试下界面「活」起来;桌面运行时自动切换回真实命令,调用方零改动。
 */
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * 生成 0~100 随机 CPU 模拟值。
 * 注意:必须随机变化(而非固定值),否则宠物状态机永远停在单一状态,看不出 5 态动画。
 */
function mockCpuUsage(): number {
  return Math.round(Math.random() * 100);
}

/**
 * 生成模拟内存信息 [已用, 总量, 使用率]:
 * 固定 16 GB 总量 + 30%~80% 随机使用率,让信息面板有真实感的数据。
 */
function mockMemoryInfo(): [number, number, number] {
  const total = 16 * 1024 * 1024 * 1024; // 固定 16 GB(字节)
  const percent = 0.3 + Math.random() * 0.5; // 30%~80% 随机使用率
  const used = Math.round(total * percent);
  return [used, total, Math.round(percent * 100)];
}

// ---------- 数据读取命令 ----------

/** 获取系统信息快照(CPU / 内存) */
export async function getSystemInfo(): Promise<SystemInfo> {
  // web 调试:无 Tauri 运行时,返回模拟数据
  if (!isTauri) {
    const [memory_used, memory_total, memory_percent] = mockMemoryInfo();
    return {
      cpu_usage: mockCpuUsage(),
      memory_used,
      memory_total,
      memory_percent,
    };
  }
  return await invoke<SystemInfo>("get_system_info");
}

/** 获取 CPU 使用率(百分比 0~100) */
export async function getCpuUsage(): Promise<number> {
  // web 调试:无 Tauri 运行时,返回随机 CPU
  if (!isTauri) {
    return mockCpuUsage();
  }
  return await invoke<number>("get_cpu_usage");
}

/** 获取内存信息 [已用, 总量, 使用率](单位字节) */
export async function getMemoryInfo(): Promise<[number, number, number]> {
  // web 调试:无 Tauri 运行时,返回模拟内存
  if (!isTauri) {
    return mockMemoryInfo();
  }
  return await invoke<[number, number, number]>("get_memory_info");
}

// ---------- 窗口控制命令 ----------

/** 设置窗口可见性(最小化 / 恢复) */
export async function setWindowVisible(visible: boolean): Promise<void> {
  // web 调试:无窗口可操作,no-op
  if (!isTauri) return;
  await invoke("set_window_visible", { visible });
}

/** 最小化到系统托盘 */
export async function minimizeToTray(): Promise<void> {
  // web 调试:no-op(浏览器无托盘概念)
  if (!isTauri) return;
  await invoke("set_window_visible", { visible: false });
}

/** 切换窗口置顶,返回切换后的置顶状态 */
export async function toggleAlwaysOnTop(): Promise<boolean> {
  // web 调试:no-op,返回 false 表示「未置顶」
  if (!isTauri) return false;
  return await invoke<boolean>("toggle_always_on_top");
}

/** 退出应用 */
export async function closeApp(): Promise<void> {
  // web 调试:no-op(浏览器内不应关闭页面)
  if (!isTauri) return;
  await invoke("close_app");
}
