// ============================================================
// 系统信息 hooks —— 统一轮询 + 对外保持原 API 不变
// 内部实现收敛到通用 usePolling,三个业务 hook 只做「取数 + 兜底初值」
// ============================================================
import { useState, useEffect, useCallback, useRef } from "react";
import { getSystemInfo, getCpuUsage, getMemoryInfo } from "../services/system";
import type { SystemInfo } from "../types";

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
 */
function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial: T,
): PollingResult<T> {
  const [data, setData] = useState<T>(initial);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // 用 ref 持有最新 fetcher:调用方即使传内联函数,也不会导致 effect 反复重建
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  /** 执行一次取数并更新状态(供定时器与手动 refetch 复用) */
  const refetch = useCallback(async () => {
    try {
      const value = await fetcherRef.current();
      setData(value);
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
 * CPU 使用率(默认 1s 轮询,驱动宠物状态机)
 * 对外 API 保持返回 number 不变
 */
export function useCpuUsage(intervalMs: number = 1000): number {
  const { data } = usePolling(getCpuUsage, intervalMs, 0);
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
