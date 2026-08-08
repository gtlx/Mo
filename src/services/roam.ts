// ============================================================
// 桌面漫游服务 —— 宠物窗口在桌面范围内自动走动(桌面体验优化)
//
// 设计:
//   - Tauri 环境:每帧按固定步长增量调用 move_window(dx, dy) 移动窗口;
//     在桌面范围内随机选目标点、平滑步进、到达后停留 5~15s 再换目标;
//   - web 调试(mock):没有窗口可移动,改用 CSS transform 平移宠物元素,
//     视觉上等效「桌面漫游」;
//   - 用户拖拽时暂停:Pet.tsx 在 pointerdown/pointerup 调用 pause/resume,
//     避免「漫游移动窗口 + 拖拽移动宠物」叠加冲突。
// ============================================================
import {
  isTauri,
  moveWindow,
  getWindowPosition,
  getScreenSize,
  getWindowSize,
} from "./system";

/** 屏幕内边距(px):目标点与边界保持距离,宠物不会贴边/越界 */
const EDGE_MARGIN = 60;
/** 每帧移动步长(px):≈48px/s(60fps),缓慢自然 */
const STEP_PER_FRAME = 0.8;
/** 到达目标后的停留时间范围(ms):5~15s 随机 */
const REST_MIN = 5000;
const REST_MAX = 15000;

/** 屏幕坐标点(px) */
interface Point {
  x: number;
  y: number;
}

// ---- 单例状态(本模块仅一个漫游控制器,Pet 组件挂载时启动/卸载时停止) ----

let running = false;
let paused = false;
let rafId: number | null = null;
let element: HTMLElement | null = null; // mock 模式下被 transform 平移的元素
let base: Point = { x: 0, y: 0 }; // mock:元素初始位置(transform 相对位移基准)
let current: Point = { x: 0, y: 0 }; // 当前窗口/元素屏幕位置
let target: Point | null = null; // 当前目标点(null = 停留中或待选点)
let restUntil = 0; // 允许再次移动的时间戳(到达后进入停留期)
let winW = 200;
let winH = 200;
let screenW = 0;
let screenH = 0;

/** 在屏幕内(留边距)随机选一个目标点 */
function pickTarget(): Point {
  const minX = EDGE_MARGIN;
  const minY = EDGE_MARGIN;
  const maxX = Math.max(minX, screenW - winW - EDGE_MARGIN);
  const maxY = Math.max(minY, screenH - winH - EDGE_MARGIN);
  return {
    x: minX + Math.random() * (maxX - minX),
    y: minY + Math.random() * (maxY - minY),
  };
}

/** 边界 clamp:当前位置越界时拉回可视范围。返回 true 表示发生过越界 */
function clampToBounds(): boolean {
  let bounced = false;
  const minX = EDGE_MARGIN;
  const minY = EDGE_MARGIN;
  const maxX = Math.max(minX, screenW - winW - EDGE_MARGIN);
  const maxY = Math.max(minY, screenH - winH - EDGE_MARGIN);
  if (current.x < minX) {
    current.x = minX;
    bounced = true;
  }
  if (current.x > maxX) {
    current.x = maxX;
    bounced = true;
  }
  if (current.y < minY) {
    current.y = minY;
    bounced = true;
  }
  if (current.y > maxY) {
    current.y = maxY;
    bounced = true;
  }
  return bounced;
}

/** 执行一次位置移动:Tauri 走 move_window 增量命令,mock 走 CSS transform */
function applyMove(dx: number, dy: number): void {
  if (isTauri) {
    void moveWindow(dx, dy);
  } else if (element) {
    // 相对元素初始位置(base)的平移,等效「窗口在桌面漫游」
    element.style.transform = `translate(${current.x - base.x}px, ${current.y - base.y}px)`;
  }
}

/** 单帧漫游逻辑:停留 → 选点 → 平滑步进 → 到达停留,循环往复 */
function tick(now: number): void {
  if (running) {
    rafId = requestAnimationFrame(tick);
  }
  if (!running || paused) return;

  // 到达后的停留期:原地等待,restUntil 之后才重新选点
  if (!target && now < restUntil) return;

  // 无目标且过了停留期:选一个新目标,下一帧开始移动
  if (!target) {
    target = pickTarget();
    return;
  }

  const dx = target.x - current.x;
  const dy = target.y - current.y;
  const dist = Math.hypot(dx, dy);

  if (dist < 1) {
    // 已到达:进入 5~15s 随机停留
    restUntil = now + REST_MIN + Math.random() * (REST_MAX - REST_MIN);
    target = null;
    return;
  }

  // 平滑步进:按固定步长朝目标移动(最后一帧不足一步则只走剩余距离)
  const step = Math.min(STEP_PER_FRAME, dist);
  const vx = (dx / dist) * step;
  const vy = (dy / dist) * step;
  current.x += vx;
  current.y += vy;

  // 越界拉回(如窗口被系统/用户移出范围):重选目标,下个点自然在屏幕内侧
  if (clampToBounds()) {
    target = null;
  }
  applyMove(vx, vy);
}

/**
 * 启动漫游循环(幂等,重复调用无副作用)。
 * @param el mock 模式下被 CSS transform 平移的宠物元素(Tauri 模式忽略)
 */
export async function startRoam(el?: HTMLElement | null): Promise<void> {
  if (running) return;
  element = el ?? null;

  const screen = await getScreenSize();
  screenW = screen.width;
  screenH = screen.height;

  if (isTauri) {
    // 真实窗口:以当前窗口位置为基准,窗口尺寸参与目标点计算
    const pos = await getWindowPosition();
    current = { x: pos.x, y: pos.y };
    const win = await getWindowSize();
    winW = win.width;
    winH = win.height;
  } else {
    // mock:以宠物元素当前位置为基准,transform 只做相对位移
    const rect = element?.getBoundingClientRect();
    base = rect ? { x: rect.left, y: rect.top } : { x: 0, y: 0 };
    current = { ...base };
    winW = element?.offsetWidth ?? 160;
    winH = element?.offsetHeight ?? 200;
  }

  target = null;
  restUntil = 0;
  paused = false;
  running = true;
  rafId = requestAnimationFrame(tick);
}

/** 停止漫游循环(组件卸载时调用) */
export function stopRoam(): void {
  running = false;
  paused = false;
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
  target = null;
}

/** 暂停漫游(用户拖拽宠物时调用,避免窗口移动与拖拽叠加) */
export function pauseRoam(): void {
  paused = true;
}

/**
 * 恢复漫游。mock 模式下会重新同步基准位置:
 * 拖拽期间宠物元素被 left/top 移动,恢复时以新位置为基准继续漫游,避免位置跳变。
 */
export function resumeRoam(): void {
  paused = false;
  if (!isTauri && element) {
    const rect = element.getBoundingClientRect();
    base = { x: rect.left, y: rect.top };
    current = { ...base };
  }
}
