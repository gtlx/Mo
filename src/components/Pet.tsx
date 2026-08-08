// ============================================================
// 宠物组件 —— 渲染器驱动 + 完整手势表(P1-2)+ 状态渲染解耦(P1-4)
//              + 行为序列自然化 + 拖拽修复(2026-08-09)
//
// 手势表(P1-2):
//   - 拖拽移动:pointer 事件(pointerdown/move/up/cancel),位移阈值
//     > DRAG_THRESHOLD(5px)判定为拖拽;拖拽不触发单击,位置写入
//     localStorage(刷新后保留);拖出屏幕边界自动 clamp 回可视区。
//   - 单击/双击分离:单击延迟 CLICK_DELAY(250ms)判定(等待双击),
//     双击时取消挂起单击;单击切换信息面板、双击触发挥手(greet)。
//   - 右键:阻止浏览器默认菜单,把坐标上报给 App 层弹出自定义菜单。
//
// 拖拽修复(2026-08-09,用户反馈「只能上下拖动」):
//   - 根因:主窗仅 200x200,宠物本体 ~77x120,窗口内元素拖拽的
//     横向空间只剩 ~46px;叠加首次拖拽的 translateX(-50%) 补偿
//     bug(起始 x 记成中心而非左边缘,positioned 后瞬间横向跳半个
//     身位)→ 横向几乎拖不动,纵向稍好 = 「只能上下」观感。
//   - 修复:桌面版(Tauri)拖拽改为**移动整个窗口**(QQ 宠物行为),
//     增量 moveWindow 全方向自由,不再受窗口内空间限制;web 调试
//     仍移动窗口内元素,但修掉补偿 bug(起始 x = 显示左边缘,无跳变)。
//   - 事件通道:pointermove/up 挂 window 级监听(不依赖
//     setPointerCapture —— WebKitGTK 对 capture 支持不稳定,
//     指针移出元素后仍能收到事件)。
//
// 状态渲染解耦(P1-4)+ 行为序列自然化(2026-08-09):
//   - CPU 基础状态来自 usePetStatus(2s 轮询,仅状态切换才 setState);
//   - 展示状态 = useBehaviorSequence(cpuStatus):安静时(idle/sleeping)
//     叠加 QQ 宠物式行为序列(发呆 10~25s → 偶发小动作 thinking →
//     回发呆),CPU 活跃时真实状态优先;双击挥手(greet)互动打断待机,
//     挥手结束回发呆。状态优先级:overload > 互动 > 走路(working +
//     漫游)> 自发小动作 > idle/sleeping。
// ============================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { usePetStatus } from "../hooks/useSystemInfo";
import { useBehaviorSequence } from "../hooks/useBehaviorSequence";
import { createRenderer } from "../renderers";
import type { PetRenderer } from "../renderers/types";
import { qqpetCodexManifest } from "../assets/pets/qqpet-codex";
import {
  startRoam,
  stopRoam,
  pauseRoam,
  resumeRoam,
  setPetStatus,
} from "../services/roam";
import {
  isTauri,
  moveWindow,
  getWindowPosition,
  setWindowPosition,
} from "../services/system";
import CpuBubble from "./CpuBubble";

/** 拖拽与单击判定的移动阈值(px):位移超过则视为拖拽,不再算单击 */
const DRAG_THRESHOLD = 5;
/** 单击判定延迟(ms):等待双击窗口,期间再来一次点击则交给双击处理 */
const CLICK_DELAY = 250;
/** 宠物位置持久化的 localStorage key(web 调试:窗口内元素 left/top) */
const POSITION_KEY = "mo.pet.position";
/** 窗口位置持久化的 localStorage key(桌面版:整个窗口的物理坐标) */
const WINDOW_POSITION_KEY = "mo.window.position";

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

/** 宠物位置(px):web = 窗口内元素 left/top;桌面 = 窗口物理坐标 */
interface PetPosition {
  x: number;
  y: number;
}

export default function Pet({ onClick, onDoubleClick, onContextMenu }: PetProps) {
  const { t } = useTranslation();
  // P1-4:CPU 基础状态(跨阈值才 re-render)+ 行为序列调度器(展示状态)
  const cpuStatus = usePetStatus(2000);
  const { status, reset: resetBehavior } = useBehaviorSequence(cpuStatus);

  // 位置状态:null = 未拖拽过(走 CSS 默认底部居中);非 null = 绝对定位且已持久化。
  // 桌面版(Tauri)元素永远居中 —— 拖拽移动的是整个窗口,位置存 WINDOW_POSITION_KEY,
  // 不读元素位置(否则会残留 web 调试的旧值,把元素拖到窗口角落)。
  const [position, setPosition] = useState<PetPosition | null>(() => {
    if (isTauri) return null;
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
  /** 桌面版拖窗口:上一次指针位置(用于增量 moveWindow) */
  const lastPointerRef = useRef<{ x: number; y: number } | null>(null);

  // 挂载渲染器:仅一次;卸载时销毁,避免内存泄漏
  useEffect(() => {
    const renderer = createRenderer(qqpetCodexManifest);
    if (mountRef.current) {
      renderer.mount(mountRef.current);
      renderer.play(status);
    }
    rendererRef.current = renderer;
    // 桌面漫游:先恢复持久化的窗口位置,再以最终位置为基准启动漫游
    // (Tauri 移动窗口 / mock 平移元素),卸载停止
    void (async () => {
      if (isTauri) {
        try {
          const raw = localStorage.getItem(WINDOW_POSITION_KEY);
          if (raw) {
            const parsed: unknown = JSON.parse(raw);
            if (
              typeof parsed === "object" &&
              parsed !== null &&
              typeof (parsed as PetPosition).x === "number" &&
              typeof (parsed as PetPosition).y === "number"
            ) {
              await setWindowPosition((parsed as PetPosition).x, (parsed as PetPosition).y);
            }
          }
        } catch {
          // 存储损坏/不可用时保持系统默认位置
        }
      }
      await startRoam(petRef.current);
    })();
    return () => {
      renderer.destroy();
      rendererRef.current = null;
      stopRoam();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 展示状态变化 → 通知渲染器切换状态 + 同步给漫游控制器
  // (渲染器内部处理过渡动画;漫游据此实现「仅走路状态才移动」)
  useEffect(() => {
    rendererRef.current?.play(status);
    setPetStatus(status);
  }, [status]);

  /**
   * 读取宠物当前实际屏幕位置。
   * 修复(2026-08-09):返回**显示左边缘**(rect.left)而非补偿后的中心 ——
   * `.pet.positioned` 已设 transform:none,inline left 就是左边缘;
   * 旧代码记成中心(rect.left + width/2),首次拖拽固化后元素瞬间
   * 横向跳半个身位,基准错位 → 横向「拖不动」的直接原因之一。
   */
  const readCurrentPosition = useCallback((): PetPosition => {
    const el = petRef.current;
    if (!el) return { x: 0, y: 0 };
    const rect = el.getBoundingClientRect();
    return { x: rect.left, y: rect.top };
  }, []);

  // ---------- 拖拽:window 级监听(不依赖 setPointerCapture) ----------

  /**
   * pointermove(挂 window):位移超阈值判定拖拽后,
   * - 桌面版:增量移动整个窗口(moveWindow)——QQ 宠物行为,全方向自由;
   * - web 调试:更新窗口内元素 left/top,并 clamp 在可视区内。
   */
  const handleWindowPointerMove = useCallback((e: PointerEvent) => {
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
    if (isTauri) {
      // 桌面版:拖拽 = 移动整个窗口。基于上次指针位置的增量移动,
      // 不需要窗口起始坐标(避免异步获取),合成器/tauri 负责位置。
      const last = lastPointerRef.current ?? { x: drag.startPointerX, y: drag.startPointerY };
      const mx = e.clientX - last.x;
      const my = e.clientY - last.y;
      lastPointerRef.current = { x: e.clientX, y: e.clientY };
      void moveWindow(mx, my);
    } else {
      // web 调试:边界 clamp,保证宠物主体始终在可视区内,不会被拖丢
      const el = petRef.current;
      const width = el?.offsetWidth ?? 100;
      const height = el?.offsetHeight ?? 120;
      const x = Math.min(Math.max(0, drag.startLeft + dx), window.innerWidth - width);
      const y = Math.min(Math.max(0, drag.startTop + dy), window.innerHeight - height);
      setPosition({ x, y });
    }
  }, []);

  /** pointerup/cancel(挂 window):拖拽结束,持久化位置,恢复漫游 */
  const handleWindowPointerEnd = useCallback(() => {
    window.removeEventListener("pointermove", handleWindowPointerMove);
    window.removeEventListener("pointerup", handleWindowPointerEnd);
    window.removeEventListener("pointercancel", handleWindowPointerEnd);
    lastPointerRef.current = null;
    if (isDraggingRef.current) {
      // 先清显示态(ref 保持 true,留给拖拽后派发的 click 消费,见 handleClick)
      setDragging(false);
      if (isTauri) {
        // 桌面版:持久化窗口位置(启动时恢复)
        void getWindowPosition().then((pos) => {
          try {
            localStorage.setItem(WINDOW_POSITION_KEY, JSON.stringify(pos));
          } catch {
            // 存储不可用(如隐私模式)时静默,不影响本次会话
          }
        });
      } else {
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
    }
    dragRef.current = null;
    // 拖拽结束恢复桌面漫游(Tauri 下内部会重新查询窗口位置,从新位置续走)
    void resumeRoam();
  }, [handleWindowPointerMove]);

  /** pointerdown:记录拖拽快照,立即固化当前位置(脱离 CSS 居中布局) */
  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return; // 仅左键拖拽
      // 点击按下时若有挂起的单击判定,先取消(避免拖拽后误触发面板切换)
      if (pendingClickRef.current) {
        clearTimeout(pendingClickRef.current);
        pendingClickRef.current = null;
      }
      let startLeft: number;
      let startTop: number;
      if (isTauri) {
        // 桌面版:元素不动,拖拽基准用不到(增量移动窗口),仅为结构完整性
        const start = position ?? readCurrentPosition();
        startLeft = start.x;
        startTop = start.y;
      } else {
        // web:以「当前显示位置」为拖拽基准 —— 先读(可能含漫游 transform
        // 残留偏移),固化到 left/top,再清 transform。顺序不能反:先清会
        // 让元素瞬间跳回 left/top(残留偏移丢失),先固化则清除瞬间
        // 显示位置不变,按下无跳变,拖拽基准 = 用户看到的实际位置。
        const start = readCurrentPosition();
        setPosition(start);
        if (petRef.current) {
          petRef.current.style.transform = ""; // 漫游残留 transform 清除,left/top 接管
        }
        startLeft = start.x;
        startTop = start.y;
      }
      dragRef.current = {
        startPointerX: e.clientX,
        startPointerY: e.clientY,
        startLeft,
        startTop,
      };
      lastPointerRef.current = { x: e.clientX, y: e.clientY };
      isDraggingRef.current = false;
      setDragging(false);
      // 用户按下即暂停桌面漫游,避免「漫游移动窗口 + 拖拽移动宠物」叠加冲突
      pauseRoam();
      // window 级监听:指针移出元素后仍能收到 move/up(不依赖
      // setPointerCapture —— WebKitGTK 对 capture 支持不稳定)
      window.addEventListener("pointermove", handleWindowPointerMove);
      window.addEventListener("pointerup", handleWindowPointerEnd);
      window.addEventListener("pointercancel", handleWindowPointerEnd);
    },
    [position, readCurrentPosition, handleWindowPointerMove, handleWindowPointerEnd],
  );

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

  /** 双击:取消挂起单击,触发挥手动画 + 重置行为序列回待机(互动打断) */
  const handleDoubleClick = useCallback(() => {
    if (pendingClickRef.current) {
      clearTimeout(pendingClickRef.current);
      pendingClickRef.current = null;
    }
    rendererRef.current?.greet?.(); // 双击:挥手动画
    resetBehavior(); // 挥手结束后回一段完整待机,而不是接续被打断的小动作
    onDoubleClick?.();
  }, [onDoubleClick, resetBehavior]);

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
