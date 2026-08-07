// ============================================================
// CPU 数字气泡组件 —— 状态渲染解耦(P1-4)
//
// 与宠物主体(Pet.tsx)分离:
//   - 本组件自己 2s 轮询 CPU(降频,原 1s),数据变化只更新气泡;
//   - 宠物主体改用 usePetStatus,仅「状态切换」才 re-render;
//   - 因此 CPU 数值在同一个状态区间内波动时,宠物主体零 re-render,
//     动画(呼吸/眨眼/帧循环)完全不受 React 更新影响。
// ============================================================
import { useCpuUsage } from "../hooks/useSystemInfo";

/** CPU 数字气泡:宠物头顶实时显示 CPU%(2s 轮询) */
export default function CpuBubble() {
  const cpuUsage = useCpuUsage(2000);

  return <div className="pet-bubble">{cpuUsage.toFixed(0)}%</div>;
}
