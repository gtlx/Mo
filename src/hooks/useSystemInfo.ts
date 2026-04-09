import { useState, useEffect, useCallback } from "react";
import { getSystemInfo, getCpuUsage, getMemoryInfo } from "../services/system";
import type { SystemInfo } from "../types";

export function useSystemInfo(intervalMs: number = 2000) {
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchInfo = useCallback(async () => {
    try {
      const data = await getSystemInfo();
      setInfo(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to fetch system info");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchInfo();
    const interval = setInterval(fetchInfo, intervalMs);
    return () => clearInterval(interval);
  }, [fetchInfo, intervalMs]);

  return { info, loading, error, refetch: fetchInfo };
}

export function useCpuUsage(intervalMs: number = 1000) {
  const [usage, setUsage] = useState(0);

  useEffect(() => {
    const fetch = async () => {
      try {
        const val = await getCpuUsage();
        setUsage(val);
      } catch {
        // ignore
      }
    };
    fetch();
    const interval = setInterval(fetch, intervalMs);
    return () => clearInterval(interval);
  }, [intervalMs]);

  return usage;
}

export function useMemoryInfo(intervalMs: number = 2000) {
  const [memory, setMemory] = useState<[number, number, number]>([0, 0, 0]);

  useEffect(() => {
    const fetch = async () => {
      try {
        const val = await getMemoryInfo();
        setMemory(val);
      } catch {
        // ignore
      }
    };
    fetch();
    const interval = setInterval(fetch, intervalMs);
    return () => clearInterval(interval);
  }, [intervalMs]);

  return memory;
}
