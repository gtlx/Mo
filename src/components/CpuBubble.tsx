// ============================================================
// CPU 数字气泡组件 —— 状态渲染解耦(P1-4)+ 数值平滑(桌面体验优化)
//
// 与宠物主体(Pet.tsx)分离:
//   - 本组件自己 2s 轮询 CPU(降频,原 1s),数据变化只更新气泡;
//   - 宠物主体改用 usePetStatus,仅「状态切换」才 re-render;
//   - 因此 CPU 数值在同一个状态区间内波动时,宠物主体零 re-render,
//     动画(呼吸/眨眼/帧循环)完全不受 React 更新影响。
//
// 数值平滑(桌面体验优化):
//   - 对最近 SMOOTH_WINDOW(5) 次采样做滑动平均后再显示,数字稳定,
//     不随单次采样大幅跳动。
// ============================================================
import { useEffect, useRef, useState } from "react";
import { useCpuUsage } from "../hooks/useSystemInfo";

/** 滑动平均窗口大小:取最近 5 次采样求平均 */
const SMOOTH_WINDOW = 5;

/** CPU 数字气泡:宠物头顶实时显示 CPU%(2s 轮询 + 最近 5 次滑动平均) */
export default function CpuBubble() {
  const cpuUsage = useCpuUsage(2000);
  // 滑动窗口历史(最近 SMOOTH_WINDOW 次采样)
  const historyRef = useRef<number[]>([]);
  // 平滑后的展示值
  const [smoothCpu, setSmoothCpu] = useState(0);

  // 每次拿到新采样:入窗 → 超窗丢弃最旧 → 求平均 → 更新展示
  useEffect(() => {
    const history = historyRef.current;
    history.push(cpuUsage);
    if (history.length > SMOOTH_WINDOW) {
      history.shift();
    }
    const avg = history.reduce((a, b) => a + b, 0) / history.length;
    setSmoothCpu(avg);
  }, [cpuUsage]);

  return <div className="pet-bubble">{smoothCpu.toFixed(0)}%</div>;
}
