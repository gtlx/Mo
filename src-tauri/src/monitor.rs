// ============================================================
// monitor.rs —— 系统监测与阈值告警(2026-08-09,P1-5 从 app.rs 抽出)
//
// 背景:PLAN.md 遗留的 app.rs 拆分建议(P1-5 一并做)——
// app.rs 原本承载命令/监测线程/托盘/窗口控制四件事,本文件承接
// **monitor 部分**(系统信息采集 + 共享缓存 + 阈值告警),app.rs
// 只保留窗口控制命令与注册(托盘/窗口后续可再拆 tray.rs/window.rs)。
//
// 职责:
//   1. 系统信息采集:后台线程每秒轮询 CPU/内存(sysinfo),
//      写入共享缓存 AppState(前端 Tauri 命令只读缓存,不做同步 IO);
//   2. 阈值判断:CPU > 85%(env MO_CPU_OVERLOAD_THR 可配)或
//      内存 > 90%(env MO_MEM_OVERLOAD_THR)→ 判定「过载」;
//   3. 滞回(防数据微波动抖动):持续超阈值 ENTER_MS(默认 3s)才
//      确认进入过载,持续低于阈值 EXIT_MS(默认 5s)才确认退出,
//      进入/退出各发一次告警事件(OverloadStarted/OverloadEnded);
//   4. 事件通知:经 std mpsc channel 发给订阅方(Rust 宠物渲染器
//      → 切 overload 警示动画)。订阅方 drop receiver 后 send 返回
//      Err,线程忽略继续跑(WebKit 路径不订阅,无堆积问题)。
//
// 系统通知(可选,后续):OverloadStarted 时可发 Linux 桌面通知
// (notify-rust crate / tauri-plugin-notification)。本阶段宠物警示
// 动画已闭环,通知待确认本机通知 daemon 后再接(见 PLAN.md P1-5)。
// ============================================================

use serde::Serialize;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::System;
use tauri::Manager; // try_state(读 tauri 管理的共享状态)

// ── 系统信息结构(前端 get_system_info / get_memory_info 读取)──

#[derive(Serialize, Clone)]
pub struct SystemInfo {
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_percent: f32,
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_used: 0,
            memory_total: 0,
            memory_percent: 0.0,
        }
    }
}

// ── 后台缓存状态(manage 进 tauri;命令只读缓存,不做同步 IO)──

pub struct AppState {
    pub cpu_usage: Mutex<f32>,
    pub system_info: Mutex<SystemInfo>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            cpu_usage: Mutex::new(0.0),
            system_info: Mutex::new(SystemInfo::default()),
        }
    }
}

// ── 阈值与滞回参数 ──

/// 阈值配置。均可经 env 覆盖(便于调试/演示):
/// - MO_CPU_OVERLOAD_THR:CPU 过载阈值(%)默认 85
/// - MO_MEM_OVERLOAD_THR:内存过载阈值(%)默认 90
/// - MO_CPU_MID_THR:中负载分界(%)默认 40(供状态驱动区分
///   低负载 idle 节奏 / 中负载 working 增多,不触发告警)
/// - MO_ENTER_OVERLOAD_MS:进入过载需持续超阈值时长(ms)默认 3000
/// - MO_EXIT_OVERLOAD_MS:退出过载需持续低于阈值时长(ms)默认 5000
pub struct Thresholds {
    pub cpu_overload: f32,
    pub mem_overload: f32,
    pub cpu_mid: f32,
    pub enter_ms: u64,
    pub exit_ms: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_overload: 85.0,
            mem_overload: 90.0,
            cpu_mid: 40.0,
            enter_ms: 3000,
            exit_ms: 5000,
        }
    }
}

/// 读 env 覆盖阈值(解析失败/未设置 → 默认值)
fn env_thr(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

/// 启动时读取一次阈值配置(env 覆盖)
pub fn thresholds() -> Thresholds {
    Thresholds {
        cpu_overload: env_thr("MO_CPU_OVERLOAD_THR", 85.0),
        mem_overload: env_thr("MO_MEM_OVERLOAD_THR", 90.0),
        cpu_mid: env_thr("MO_CPU_MID_THR", 40.0),
        enter_ms: std::env::var("MO_ENTER_OVERLOAD_MS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(3000),
        exit_ms: std::env::var("MO_EXIT_OVERLOAD_MS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(5000),
    }
}

// ── 负载档位与监测事件 ──

/// 瞬时负载档位(每次采样的快照判定;供状态驱动选权重池)。
/// 过载档位 = 超阈值(与滞回告警事件分开:档位是瞬时的,事件是
/// 滞回确认后的)。宠物进入 overload 由 OverloadStarted 事件驱动。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadLevel {
    /// 低负载(cpu < MO_CPU_MID_THR):宠物保持 idle 为主的悠闲节奏
    Low,
    /// 中负载(cpu >= MO_CPU_MID_THR):thinking/working 权重提高
    Mid,
    /// 过载(超 CPU/内存阈值):宠物切 overload 警示
    Overload,
}

/// 监测事件(经 channel 通知订阅方;pet_render 每秒消费一次)。
#[derive(Clone, Debug)]
pub enum MonitorEvent {
    /// 周期采样(每秒一条):含瞬时负载档位,供真实状态驱动
    /// 把「中负载」映射为 thinking/working 动作(不触发告警)。
    Sample { cpu: f32, mem: f32, level: LoadLevel },
    /// 进入过载(滞回确认后,一次性):驱动宠物切 overload 警示。
    OverloadStarted { cpu: f32, mem: f32 },
    /// 退出过载(滞回确认后,一次性):宠物回 idle。
    OverloadEnded { cpu: f32, mem: f32 },
}

// ── Tauri 命令(系统信息读取;只读共享缓存,无同步 IO)──

#[tauri::command]
pub fn get_system_info(state: tauri::State<'_, AppState>) -> SystemInfo {
    state.system_info.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_cpu_usage(state: tauri::State<'_, AppState>) -> f32 {
    *state.cpu_usage.lock().unwrap()
}

#[tauri::command]
pub fn get_memory_info(state: tauri::State<'_, AppState>) -> (u64, u64, f32) {
    let info = state.system_info.lock().unwrap();
    (info.memory_used, info.memory_total, info.memory_percent)
}

// ── 后台监测线程 ──

/// 系统桌面通知(P1-5):进入过载时发 Linux 桌面通知。
/// notify-rust 走 DBus(org.freedesktop.Notifications);本机 niri 环境
/// 由 noctalia-shell(qs)提供该服务。env MO_NOTIFY=0 关闭。
/// 通知失败(无 daemon/DBus 错误)静默忽略——宠物警示动画才是主通道,
/// 通知只是补充提醒。
fn notify_overload(cpu: f32, mem: f32) {
    if std::env::var("MO_NOTIFY").as_deref() == Ok("0") {
        return;
    }
    let _ = notify_rust::Notification::new()
        .summary("⚠ 系统负载过高")
        .body(&format!("CPU {:.0}% / 内存 {:.0}%,宠物进入警示状态", cpu, mem))
        .appname("Mo 桌面宠物")
        .icon("dialog-warning")
        .timeout(notify_rust::Timeout::Milliseconds(8000))
        .show();
}

/// 启动后台监测线程(每秒轮询 CPU/内存 → 写共享缓存 + 发事件),
/// 返回事件接收端。订阅方(宠物渲染器)持有 receiver 消费事件;
/// 不订阅(WebKit 路径)时 drop receiver,线程的 send 返回 Err 被
/// 忽略,线程继续正常写缓存。
pub fn start_monitor(app: tauri::AppHandle) -> mpsc::Receiver<MonitorEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let thr = thresholds();
        let mut sys = System::new_all();

        // 过载告警状态机(Normal ↔ Overload,带滞回):
        // - Normal:持续超阈值 enter_ms → Overload(发 OverloadStarted)
        // - Overload:持续低于阈值 exit_ms → Normal(发 OverloadEnded)
        // 期间 any 一次采样不满足条件即重置计时,防数据微波动抖动。
        #[derive(PartialEq)]
        enum Alarm {
            Normal,
            Overload,
        }
        let mut alarm = Alarm::Normal;
        let mut since: Option<Instant> = None; // 当前判定方向的持续起点

        loop {
            // sysinfo 的 CPU 使用率是「距上次刷新」的差值,首次刷新
            // 无数据,故先 refresh 一次 + sleep 1s 再 refresh 取数
            // (原 app.rs 逻辑,保持 1s 轮询粒度)。
            sys.refresh_cpu_all();
            std::thread::sleep(Duration::from_millis(1000));
            sys.refresh_cpu_all();
            sys.refresh_memory();

            let cpu = sys.global_cpu_usage();
            let mem_used = sys.used_memory();
            let mem_total = sys.total_memory();
            let mem_pct = if mem_total > 0 {
                mem_used as f32 / mem_total as f32 * 100.0
            } else {
                0.0
            };

            // ① 写共享缓存(前端命令读)
            if let Some(state) = app.try_state::<AppState>() {
                *state.cpu_usage.lock().unwrap() = cpu;
                let mut info = state.system_info.lock().unwrap();
                info.cpu_usage = cpu;
                info.memory_used = mem_used;
                info.memory_total = mem_total;
                info.memory_percent = mem_pct;
            }

            // ② 阈值判定 + 滞回 → 告警事件
            let hot = cpu > thr.cpu_overload || mem_pct > thr.mem_overload;
            let level = if hot {
                LoadLevel::Overload
            } else if cpu >= thr.cpu_mid {
                LoadLevel::Mid
            } else {
                LoadLevel::Low
            };

            match alarm {
                Alarm::Normal => {
                    if hot {
                        let s = since.get_or_insert_with(Instant::now);
                        if s.elapsed().as_millis() as u64 >= thr.enter_ms {
                            alarm = Alarm::Overload;
                            since = None;
                            let _ = tx.send(MonitorEvent::OverloadStarted { cpu, mem: mem_pct });
                            // 系统桌面通知(失败静默忽略,宠物动画才是主通道)
                            notify_overload(cpu, mem_pct);
                        }
                    } else {
                        since = None;
                    }
                }
                Alarm::Overload => {
                    if !hot {
                        let s = since.get_or_insert_with(Instant::now);
                        if s.elapsed().as_millis() as u64 >= thr.exit_ms {
                            alarm = Alarm::Normal;
                            since = None;
                            let _ = tx.send(MonitorEvent::OverloadEnded { cpu, mem: mem_pct });
                        }
                    } else {
                        since = None;
                    }
                }
            }

            // ③ 周期采样事件(每秒;状态驱动选权重池用)
            let _ = tx.send(MonitorEvent::Sample { cpu, mem: mem_pct, level });
        }
    });
    rx
}
