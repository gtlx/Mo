// ============================================================
// 行为序列调度器 —— QQ 宠物式「行为序列」而非随机切换动作
//
// 背景(用户 2026-08-09 反馈):加权随机切换动作仍不够自然,
// QQ 宠物是**行为序列**(发呆→眨眼→小动作→偶尔走路,一段自然
// 脚本)而非单动作随机跳。本 hook 在 CPU 基础状态之上叠加
// 「自发行为序列」,让宠物在安静时呈现出自然的待机节奏:
//
//   发呆 10~25s(呼吸 + 眨眼,渲染器内已有)→ 偶发小动作
//   (thinking 2~4s,东张西望)→ 回发呆,循环往复。
//
// 状态优先级(高 → 低):
//   overload(CPU 警示)> 用户互动(双击挥手,渲染器 greet 打断)>
//   walking(CPU working + 漫游移动)> 自发小动作(仅 idle/sleeping
//   安静基础态下)> CPU 基础状态(idle/sleeping)。
//
// 触发式走路说明:走路状态(working)完全由 CPU 映射决定,漫游
// 控制器(roam.ts)只在该状态移动窗口 —— 「有目标才走、到达停留、
// 非走路原地」,本 hook 不干预 walking。
//
// 伸懒腰素材说明:qqpet-codex 精灵图 stateRows 没有 stretch 行
// (idle/running-right/running-left/waving/jumping/failed/waiting/
// running/review),「小动作」用 thinking(waiting 行,小幅等待/
// 思考动作)承载,其中 1/3 概率拉长到 3.5~5s 表现「伸懒腰」式
// 舒展;未来素材补 stretch 行后,在此处映射即可。
// ============================================================

import { useCallback, useEffect, useRef, useState } from "react";
import type { PetStatus } from "../types";

/** 发呆时长范围(ms):10~25s —— 比旧实现(8~15s)更慢,QQ 宠物主基调 */
const REST_MIN_MS = 10000;
const REST_MAX_MS = 25000;
/** 小动作(东张西望/思考)时长范围(ms):2~4s */
const ACTION_MIN_MS = 2000;
const ACTION_MAX_MS = 4000;
/** 拉长版小动作(伸懒腰式舒展)时长范围(ms):3.5~5s */
const STRETCH_MIN_MS = 3500;
const STRETCH_MAX_MS = 5000;
/** 发呆结束后进入小动作的概率(0~1),其余继续发呆 —— 小动作「偶发」 */
const ACTION_CHANCE = 0.28;
/** 小动作中「拉长版(伸懒腰)」占比 */
const STRETCH_CHANCE = 0.33;

/** 是否安静基础态:只有 idle/sleeping 才允许自发行为序列(其余状态真实驱动) */
function isQuietBase(status: PetStatus): boolean {
  return status === "idle" || status === "sleeping";
}

/** 发呆时长(10~25s 随机) */
function nextRestMs(): number {
  return REST_MIN_MS + Math.random() * (REST_MAX_MS - REST_MIN_MS);
}

/** 小动作时长:普通 2~4s,偶发拉长版(伸懒腰观感)3.5~5s */
function nextActionMs(): number {
  if (Math.random() < STRETCH_CHANCE) {
    return STRETCH_MIN_MS + Math.random() * (STRETCH_MAX_MS - STRETCH_MIN_MS);
  }
  return ACTION_MIN_MS + Math.random() * (ACTION_MAX_MS - ACTION_MIN_MS);
}

/**
 * 行为序列调度器。
 *
 * @param cpuStatus CPU 映射的基础状态(usePetStatus 输出)
 * @returns status  展示状态(喂给渲染器 play 与漫游 setPetStatus)
 *          reset   互动打断后重置回待机(双击挥手时调用)
 */
export function useBehaviorSequence(cpuStatus: PetStatus): {
  status: PetStatus;
  reset: () => void;
} {
  const [status, setStatus] = useState<PetStatus>(cpuStatus);

  /** 调度阶段:rest = 发呆中 / action = 小动作中 */
  const phaseRef = useRef<"rest" | "action">("rest");
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** 最新 CPU 基础状态(定时器回调里读取,避免闭包过期) */
  const cpuStatusRef = useRef<PetStatus>(cpuStatus);
  cpuStatusRef.current = cpuStatus;

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  /** 调度下一阶段:发呆到点 → 决定继续发呆 or 小动作;小动作到点 → 回发呆 */
  const scheduleNext = useCallback(() => {
    timerRef.current = null;
    const base = cpuStatusRef.current;

    // CPU 已离开安静区:不再自发调度,交给「CPU 变化 effect」接管
    if (!isQuietBase(base)) return;

    if (phaseRef.current === "rest") {
      // 发呆结束:偶发小动作(thinking 东张西望),多数时候继续发呆。
      // sleeping(沉睡)不发小动作 —— 睡觉的宠物不应突然「东张西望」
      if (base !== "sleeping" && Math.random() < ACTION_CHANCE) {
        phaseRef.current = "action";
        setStatus("thinking");
        timerRef.current = setTimeout(scheduleNext, nextActionMs());
      } else {
        setStatus(base); // 保持安静态(sleeping 或 idle)
        timerRef.current = setTimeout(scheduleNext, nextRestMs());
      }
    } else {
      // 小动作结束:回发呆(QQ 宠物动作是「偶发点缀」,不是主基调)
      phaseRef.current = "rest";
      setStatus(base);
      timerRef.current = setTimeout(scheduleNext, nextRestMs());
    }
  }, []);

  // CPU 基础状态变化:安静区内外切换 → 接管/让出自发调度
  useEffect(() => {
    if (isQuietBase(cpuStatus)) {
      // 进入安静区:若尚无调度,启动发呆;已有调度(发呆/小动作)则保持,
      // 不打断进行中的行为(避免每次 CPU 波动都重置发呆计时)
      if (timerRef.current === null) {
        phaseRef.current = "rest";
        setStatus(cpuStatus);
        timerRef.current = setTimeout(scheduleNext, nextRestMs());
      }
    } else {
      // CPU 活跃(thinking/working/overload):取消自发调度,真实状态优先
      clearTimer();
      phaseRef.current = "rest";
      setStatus(cpuStatus);
    }
    // 注意:scheduleNext 稳定(空依赖),仅 cpuStatus 变化触发本 effect
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cpuStatus]);

  // 卸载清理定时器
  useEffect(() => clearTimer, [clearTimer]);

  /**
   * 互动打断后重置回待机(双击挥手时由 Pet.tsx 调用):
   * 挥手动画由渲染器 greet 机制播放(期间忽略 play),挥手结束后
   * 渲染器恢复挥手前状态;这里把自发调度重置回「发呆」起点,
   * 让挥手结束后自然进入一段完整待机,而不是接续被挥手打断的小动作。
   */
  const reset = useCallback(() => {
    clearTimer();
    phaseRef.current = "rest";
    setStatus(cpuStatus);
    if (isQuietBase(cpuStatus)) {
      timerRef.current = setTimeout(scheduleNext, nextRestMs());
    }
  }, [clearTimer, cpuStatus, scheduleNext]);

  return { status, reset };
}
