// ============================================================
// 宠物组件 —— 渲染器驱动 + 完整手势表(P1-2)+ 状态渲染解耦(P1-4)
//
// 手势表(P1-2):
//   - 拖拽移动:pointer 事件(pointerdown/move/up/cancel),位移阈值
//     > DRAG_THRESHOLD(5px)判定为拖拽;拖拽不触发单击,位置写入
//     localStorage(刷新后保留);拖出屏幕边界自动 clamp 回可视区。
//   - 单击/双击分离:单击延迟 CLICK_DELAY(250ms)判定(等待双击),
//     双击时取消挂起单击;单击切换信息面板、双击触发挥手(greet,
//     独立窗口占位,联动 P1-1)。
//   - 右键:阻止浏览器默认菜单,把坐标上报给 App 层弹出自定义菜单
//     (设置/退出),不再切换面板。
//
// 状态渲染解耦(P1-4):
//   - 状态来自 usePetStatus(2s 轮询,仅状态切换才 setState,CPU 数值
//     在同一状态区间内波动时本组件零 re-render);
//   - CPU 数字气泡独立为 CpuBubble 组件,自行 2s 轮询,数据变化只
//     更新气泡,与宠物主体互不影响。
// ============================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePetStatus } from "../hooks/useSystemInfo";
import { createRenderer } from "../renderers";
import type { PetRenderer } from "../renderers/types";
import { qqpetCodexManifest } from "../assets/pets/qqpet-codex";
import { startRoam, stopRoam, pauseRoam, resumeRoam } from "../services/roam";
import CpuBubble from "./CpuBubble";

/** 拖拽与单击判定的移动阈值(px):位移超过则视为拖拽,不再算单击 */
const DRAG_THRESHOLD = 5;
/** 单击判定延迟(ms):等待双击窗口,期间再来一次点击则交给双击处理 */
const CLICK_DELAY = 250;
/** 宠物位置持久化的 localStorage key */
const POSITION_KEY = "mo.pet.position";

/** 宠物组件对外接口:P1-2 手势表事件,坐标上报用 clientX/clientY 而非 DOM 事件 */
interface PetProps {
  /** 单击:切换信息面板 */
  onClick: () => void;
  /** 双击:触发挥手(独立窗口占位,P1-1 未落地时为 no-op) */
  onDoubleClick?: () => void;
  /** 右键:上报屏幕坐标,由上层弹出自定义菜单 */
  onContextMenu?: (x: number, y: number) => void;
}

/** 拖拽进行中的快照:起始指针坐标 + 起始宠物位置(px) */
interface DragSnapshot {
  startPointerX: number;
  startPointerY: number;
  startLeft: number;
  startTop: number;
}

/** 宠物位置(localStorage 持久化内容,单位 px) */
interface PetPosition {
  x: number;
  y: number;
}

export default function Pet({ onClick, onDoubleClick, onContextMenu }: PetProps) {
  const { t } = useTranslation();
  // P1-4:状态来自独立 hook,只有跨阈值切换状态才触发本组件 re-render
  const status = usePetStatus(2000);

  // 位置状态:null = 未拖拽过(走 CSS 默认底部居中);非 null = 绝对定位且已持久化
  const [position, setPosition] = useState<PetPosition | null>(() => {
    try {
      const raw = localStorage.getItem(POSITION_KEY);
      if (!raw) return null;
      const parsed: unknown = JSON.parse(raw);
      if (
        typeof parsed === "object" &&
        parsed !== null &&
        typeof (parsed as PetPosition).x === "number" &&
        typeof (parsed as PetPosition).y === "number"
      ) {
        return parsed as PetPosition;
      }
      return null;
    } catch {
      // 存储损坏/不可用时回退默认位置
      return null;
    }
  });

  // 渲染器挂载点(容器 div)+ 渲染器实例(与 React 生命周期解耦)
  const mountRef = useRef<HTMLDivElement>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<PetRenderer | null>(null);

  // 手势状态
  // - dragging(state):驱动「dragging」类显示,pointerup 时清除触发 re-render
  // - isDraggingRef(ref):同步标记,供拖拽后的 click 抑制判断(click 在
  //   pointerup 之后派发,必须用 ref 保持到 click 消费,不能提前重置)
  const [dragging, setDragging] = useState(false);
  const dragRef = useRef<DragSnapshot | null>(null);
  const isDraggingRef = useRef(false);
  const pendingClickRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 挂载渲染器:仅一次;卸载时销毁,避免内存泄漏
  useEffect(() => {
    const renderer = createRenderer(qqpetCodexManifest);
    if (mountRef.current) {
      renderer.mount(mountRef.current);
      renderer.play(status);
    }
    rendererRef.current = renderer;
    // 桌面漫游:挂载即启动(Tauri 移动窗口 / mock 平移元素),卸载停止
    void startRoam(petRef.current);
    return () => {
      renderer.destroy();
      rendererRef.current = null;
      stopRoam();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 业务状态变化 → 通知渲染器切换状态(渲染器内部处理过渡动画)
  useEffect(() => {
    rendererRef.current?.play(status);
  }, [status]);

  /**
   * 读取宠物当前实际屏幕位置。
   * 默认布局为 bottom 居中 + translateX(-50%),需补偿水平位移,
   * 得到可写入 inline left/top 的绝对坐标。
   */
  const readCurrentPosition = useCallback((): PetPosition => {
    const el = petRef.current;
    if (!el) return { x: 0, y: 0 };
    const rect = el.getBoundingClientRect();
    // 补偿 CSS translateX(-50%):实际 left = rect.left + 元素宽度的一半
    return { x: rect.left + rect.width / 2, y: rect.top };
  }, []);

  /** pointerdown:记录拖拽快照,立即固化当前位置(脱离 CSS 居中布局) */
  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return; // 仅左键拖拽
      // 点击按下时若有挂起的单击判定,先取消(避免拖拽后误触发面板切换)
      if (pendingClickRef.current) {
        clearTimeout(pendingClickRef.current);
        pendingClickRef.current = null;
      }
      const start = position ?? readCurrentPosition();
      if (!position) setPosition(start); // 首次拖拽:把居中位置固化为绝对坐标
      dragRef.current = {
        startPointerX: e.clientX,
        startPointerY: e.clientY,
        startLeft: start.x,
        startTop: start.y,
      };
      isDraggingRef.current = false;
      setDragging(false);
      // 用户按下即暂停桌面漫游,避免「漫游移动窗口 + 拖拽移动宠物」叠加冲突
      pauseRoam();
      // 捕获指针:鼠标移出元素后仍能收到 move/up(合成事件环境可能抛错,忽略即可)
      try {
        e.currentTarget.setPointerCapture(e.pointerId);
      } catch {
        // 无活跃指针(如自动化注入的合成事件)时跳过捕获,不影响拖拽逻辑
      }
    },
    [position, readCurrentPosition],
  );

  /** pointermove:位移超过阈值判定为拖拽,实时更新位置并 clamp 在可视区内 */
  const handlePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    const dx = e.clientX - drag.startPointerX;
    const dy = e.clientY - drag.startPointerY;
    // 超过阈值 → 判定为拖拽(此后不再视为单击)
    if (!isDraggingRef.current && Math.hypot(dx, dy) > DRAG_THRESHOLD) {
      isDraggingRef.current = true;
      setDragging(true); // 更新 dragging 类显示
    }
    if (!isDraggingRef.current) return;
    // 边界 clamp:保证宠物主体始终在可视区内,不会被拖丢
    const el = petRef.current;
    const width = el?.offsetWidth ?? 100;
    const height = el?.offsetHeight ?? 120;
    const x = Math.min(Math.max(0, drag.startLeft + dx), window.innerWidth - width);
    const y = Math.min(Math.max(0, drag.startTop + dy), window.innerHeight - height);
    setPosition({ x, y });
  }, []);

  /** pointerup/cancel:拖拽结束,把最终位置持久化到 localStorage */
  const handlePointerEnd = useCallback(() => {
    if (isDraggingRef.current) {
      // 先清显示态(ref 保持 true,留给拖拽后派发的 click 消费,见 handleClick)
      setDragging(false);
      setPosition((prev) => {
        if (prev) {
          try {
            localStorage.setItem(POSITION_KEY, JSON.stringify(prev));
          } catch {
            // 存储不可用(如隐私模式)时静默,不影响本次会话
          }
        }
        return prev;
      });
    }
    dragRef.current = null;
    // 拖拽结束恢复桌面漫游(mock 下内部会重同步基准位置,避免位置跳变)
    resumeRoam();
  }, []);

  /** 单击:延迟 CLICK_DELAY 判定;期间再来一次点击则取消,交给双击处理 */
  const handleClick = useCallback(() => {
    // 拖拽结束后的 click 事件(浏览器在拖拽释放后仍会派发)→ 忽略
    if (isDraggingRef.current) {
      isDraggingRef.current = false;
      return;
    }
    if (pendingClickRef.current) {
      // 第二次点击:取消挂起的单击,等待随后的 dblclick 触发双击
      clearTimeout(pendingClickRef.current);
      pendingClickRef.current = null;
      return;
    }
    pendingClickRef.current = setTimeout(() => {
      pendingClickRef.current = null;
      onClick(); // 单击:切换信息面板
    }, CLICK_DELAY);
  }, [onClick]);

  /** 双击:取消挂起单击,触发挥手动画并通知上层(独立窗口占位) */
  const handleDoubleClick = useCallback(() => {
    if (pendingClickRef.current) {
      clearTimeout(pendingClickRef.current);
      pendingClickRef.current = null;
    }
    rendererRef.current?.greet?.(); // 双击:挥手动画
    onDoubleClick?.();
  }, [onDoubleClick]);

  /** 右键:阻止浏览器默认菜单,上报坐标由 App 层弹出自定义菜单 */
  const handleContextMenu = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      e.preventDefault();
      onContextMenu?.(e.clientX, e.clientY);
    },
    [onContextMenu],
  );

  return (
    <div
      ref={petRef}
      className={`pet${position ? " positioned" : ""}${dragging ? " dragging" : ""}`}
      style={position ? { left: position.x, top: position.y } : undefined}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerEnd}
      onPointerCancel={handlePointerEnd}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
      title={t(`pet.${status}`)}
    >
      <div className="pet-sprite" ref={mountRef} />
      {/* P1-4:气泡独立组件,自己轮询,与宠物主体渲染解耦 */}
      <CpuBubble />
    </div>
  );
}
