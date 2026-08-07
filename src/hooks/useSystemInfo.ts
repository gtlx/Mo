// ============================================================
// 系统信息 hooks —— 统一轮询 + 对外保持原 API 不变
// 内部实现收敛到通用 usePolling,三个业务 hook 只做「取数 + 兜底初值」
// ============================================================
import { useState, useEffect, useCallback, useRef } from "react";
import { getSystemInfo, getCpuUsage, getMemoryInfo } from "../services/system";
import type { SystemInfo, PetStatus } from "../types";
import { getStatus } from "../utils/status";

/** usePolling 的返回结构:数据 / 错误 / 加载中 / 手动刷新 */
interface PollingResult<T> {
  data: T;
  error: string | null;
  loading: boolean;
  refetch: () => Promise<void>;
}

/**
 * 通用轮询 hook:按固定间隔执行异步取数任务。
 * 立即执行一次 + setInterval 周期轮询;失败静默保留上一次有效数据,
 * 卸载时自动清理定时器,避免内存泄漏。
 *
 * @param fetcher    异步取数函数(services 层封装,如 getCpuUsage)
 * @param intervalMs 轮询间隔(毫秒)
 * @param initial    初始兜底值(首次加载完成前的默认数据)
 * @param isEqual    可选:新旧值相等性判断。传入后「值相等不 setState」,
 *                   调用方可在数值频繁变化但语义状态未变时避免无谓 re-render
 *                   (P1-4 状态渲染解耦的关键机制)
 */
function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial: T,
  isEqual?: (a: T, b: T) => boolean,
): PollingResult<T> {
  const [data, setData] = useState<T>(initial);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // 用 ref 持有最新 fetcher:调用方即使传内联函数,也不会导致 effect 反复重建
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  // isEqual 同样用 ref 持有,避免内联比较函数导致 refetch/effect 重建
  const isEqualRef = useRef(isEqual);
  isEqualRef.current = isEqual;

  /** 执行一次取数并更新状态(供定时器与手动 refetch 复用) */
  const refetch = useCallback(async () => {
    try {
      const value = await fetcherRef.current();
      // 传入 isEqual 且新旧相等时保留旧引用,不触发 re-render
      setData((prev) =>
        isEqualRef.current && isEqualRef.current(prev, value) ? prev : value,
      );
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  // 挂载立即拉一次,之后按 intervalMs 周期轮询
  useEffect(() => {
    refetch();
    const interval = setInterval(refetch, intervalMs);
    return () => clearInterval(interval);
  }, [refetch, intervalMs]);

  return { data, error, loading, refetch };
}

/**
 * 系统信息面板数据:CPU + 内存(默认 2s 轮询)
 * 对外 API 保持 { info, loading, error, refetch } 不变
 */
export function useSystemInfo(intervalMs: number = 2000) {
  const { data, error, loading, refetch } = usePolling<SystemInfo | null>(
    getSystemInfo,
    intervalMs,
    null,
  );
  return { info: data, loading, error, refetch };
}

/**
 * CPU 使用率(默认 2s 轮询 —— P1-4 降频,原 1s)
 * 驱动 CPU 气泡;对外 API 保持返回 number 不变
 */
export function useCpuUsage(intervalMs: number = 2000): number {
  const { data } = usePolling(getCpuUsage, intervalMs, 0);
  return data;
}

/**
 * 宠物状态轮询(默认 2s 轮询)—— P1-4 状态渲染解耦核心:
 * 轮询 CPU → getStatus 映射为离散 PetStatus,配合 usePolling 的 isEqual
 * 做到「状态值相等不 setState」:CPU 数值在同一状态区间内波动时,
 * 宠物主体(Pet.tsx)零 re-render,只有真正跨阈值切换状态才更新。
 */
export function usePetStatus(intervalMs: number = 2000): PetStatus {
  const { data } = usePolling<PetStatus>(
    async () => getStatus(await getCpuUsage()),
    intervalMs,
    "idle",
    (a, b) => a === b,
  );
  return data;
}

/**
 * 内存信息 [已用, 总量, 使用率](默认 2s 轮询)
 * 对外 API 保持返回元组不变
 */
export function useMemoryInfo(intervalMs: number = 2000): [number, number, number] {
  // 初始兜底值:0 已用 / 0 总量 / 0 使用率(断言为元组以匹配返回类型)
  const { data } = usePolling(getMemoryInfo, intervalMs, [0, 0, 0] as [number, number, number]);
  return data;
}
