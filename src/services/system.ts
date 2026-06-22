import { invoke } from "@tauri-apps/api/core";
import type { SystemInfo } from "../types";

export async function getSystemInfo(): Promise<SystemInfo> {
  return await invoke<SystemInfo>("get_system_info");
}

export async function getCpuUsage(): Promise<number> {
  return await invoke<number>("get_cpu_usage");
}

export async function getMemoryInfo(): Promise<[number, number, number]> {
  return await invoke<[number, number, number]>("get_memory_info");
}

export async function setWindowVisible(visible: boolean): Promise<void> {
  await invoke("set_window_visible", { visible });
}

/** 最小化到系统托盘 */
export async function minimizeToTray(): Promise<void> {
  await invoke("set_window_visible", { visible: false });
}

export async function toggleAlwaysOnTop(): Promise<boolean> {
  return await invoke<boolean>("toggle_always_on_top");
}

export async function closeApp(): Promise<void> {
  await invoke("close_app");
}
