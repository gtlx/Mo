// ============================================================
// 宠物组件 —— 从「CSS 五官 + 5 态切换」改造为「渲染器驱动」
//
// P1-3 精灵图改造后:
//   - 本组件只负责「业务状态 → 渲染器」的桥接,不再绘制五官;
//   - 精灵图 / 帧动画 / 呼吸眨眼等动效全部收敛到渲染器内部;
//   - 换宠物 = 换 manifest(见 src/assets/pets/),组件零改动;
//   - 将来 Live2D/Spine:新增渲染器实现 PetRenderer 接口即可,
//     Pet.tsx 无需感知渲染技术差异。
// ============================================================

import { useCallback, useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useCpuUsage } from "../hooks/useSystemInfo";
import type { PetStatus } from "../types";
import { createRenderer } from "../renderers";
import type { PetRenderer } from "../renderers/types";
import { qqpetCodexManifest } from "../assets/pets/qqpet-codex";

/** 宠物组件对外接口:点击 / 右键均触发同一回调(切换信息面板) */
interface PetProps {
  onClick: () => void;
}

/**
 * 根据 CPU 使用率映射宠物状态(阈值与 types 中 PetStatus 注释一致)
 * 负载越低越安静:overload → working → thinking → idle → sleeping
 */
function getStatus(cpu: number): PetStatus {
  if (cpu > 80) return "overload";
  if (cpu > 50) return "working";
  if (cpu > 20) return "thinking";
  if (cpu > 5) return "idle";
  return "sleeping";
}

export default function Pet({ onClick }: PetProps) {
  const { t } = useTranslation();
  const cpuUsage = useCpuUsage(1000);
  const status = useMemo(() => getStatus(cpuUsage), [cpuUsage]);

  // 渲染器挂载点(容器 div)+ 渲染器实例(与 React 生命周期解耦)
  const mountRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<PetRenderer | null>(null);

  // 挂载渲染器:仅一次;卸载时销毁,避免内存泄漏
  useEffect(() => {
    const renderer = createRenderer(qqpetCodexManifest);
    if (mountRef.current) {
      renderer.mount(mountRef.current);
      renderer.play(status);
    }
    rendererRef.current = renderer;
    return () => {
      renderer.destroy();
      rendererRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 业务状态变化 → 通知渲染器切换状态(渲染器内部处理过渡动画)
  useEffect(() => {
    rendererRef.current?.play(status);
  }, [status]);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      // 点击交互反馈:挥手一下(素材有 waving 行),再触发面板切换
      rendererRef.current?.greet?.();
      onClick();
    },
    [onClick],
  );

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      onClick();
    },
    [onClick],
  );

  return (
    <div
      className="pet"
      onClick={handleClick}
      onContextMenu={handleContextMenu}
      title={t(`pet.${status}`)}
    >
      <div className="pet-sprite" ref={mountRef} />
      <div className="pet-bubble">{cpuUsage.toFixed(0)}%</div>
    </div>
  );
}
