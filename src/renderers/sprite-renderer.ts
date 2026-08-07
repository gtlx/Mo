// ============================================================
// 精灵图渲染器(SpriteRenderer)
//
// 实现 PetRenderer 接口,用 canvas 绘制精灵图帧动画。
// 设计要点(对应「自然动效」核心诉求):
//   1. 分层状态机:业务状态(PetStatus) → 精灵图状态行 → 帧循环,
//      上层只管「业务状态」,行号/帧号/节奏全在渲染器内部解耦。
//   2. 微动效叠加:呼吸(scaleY 起伏)+ 眨眼(周期压扁),叠加在
//      基础帧动画之上,让宠物有「活物感」,不死板。
//   3. 平滑过渡:状态切换时淡入淡出 + 帧循环从首帧重启,不硬切。
//   4. 帧节奏自然化:用缓动曲线映射帧位置(起步/收步稍停,
//      中间帧流畅),而非机械均匀步进。
//
// 帧尺寸 / 状态行 / 循环时间 / 缩放全部来自 PetManifest(协议),
// 不硬编码任何宠物长相 —— 换宠物只换 manifest + 素材目录。
// ============================================================

import type { PetManifest, PetRenderer } from "./types";
import type { PetStatus } from "../types";

// ---------- 常量与配置 ----------

/** 精灵图默认规格(与 Hermes Codex 协议一致,manifest 缺省时兜底) */
const DEFAULT_FRAME_W = 192;
const DEFAULT_FRAME_H = 208;
const DEFAULT_FRAMES_PER_ROW = 8;
const DEFAULT_LOOP_MS = 1100;
const DEFAULT_SCALE = 0.33;

/** 状态切换淡入时长(ms) */
const FADE_IN_MS = 180;

/**
 * 业务状态(PetStatus) → 精灵图状态行名
 * Codex 分类法行名见 pet.json 的 stateRows:idle / running-right /
 * running-left / waving / jumping / failed / waiting / running / review。
 * 映射原则:让「业务语义」对上「动作语义」:
 * - sleeping → idle 行(循环放慢 + 呼吸加大,表现沉睡)
 * - idle     → idle 行(标准待机)
 * - thinking → waiting 行(小幅等待/思考动作)
 * - working  → running 行(忙碌跑动)
 * - overload → jumping 行(跳动警示)
 */
const STATUS_TO_ROW: Record<PetStatus, string> = {
  sleeping: "idle",
  idle: "idle",
  thinking: "waiting",
  working: "running",
  overload: "jumping",
};

/**
 * 各业务状态的播放参数(让同一行在不同业务语义下节奏不同)
 * - loopScale:  循环时长倍率(>1 放慢,<1 加快)
 * - breatheAmp: 呼吸起伏幅度(scaleY 波动比例,0 为不呼吸)
 * - breatheMs:  呼吸周期
 * - blink:      是否允许眨眼
 */
interface StatusTuning {
  loopScale: number;
  breatheAmp: number;
  breatheMs: number;
  blink: boolean;
}

const STATUS_TUNING: Record<PetStatus, StatusTuning> = {
  sleeping: { loopScale: 1.6, breatheAmp: 0.035, breatheMs: 3600, blink: false },
  idle:     { loopScale: 1.0, breatheAmp: 0.022, breatheMs: 2600, blink: true },
  thinking: { loopScale: 1.2, breatheAmp: 0.016, breatheMs: 3000, blink: true },
  working:  { loopScale: 0.9, breatheAmp: 0.012, breatheMs: 2200, blink: false },
  overload: { loopScale: 0.7, breatheAmp: 0.02,  breatheMs: 1600, blink: false },
};

/** 眨眼参数:周期随机区间 / 闭合时长 */
const BLINK_MIN_MS = 2600;
const BLINK_MAX_MS = 5200;
const BLINK_CLOSE_MS = 150;

// ---------- 工具函数 ----------

/** 缓动曲线:easeInOutSine,用于帧位置映射(起步/收步放缓) */
function easeInOutSine(t: number): number {
  return -(Math.cos(Math.PI * t) - 1) / 2;
}

/** 线性插值 */
function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** 采样检测一张帧图是否包含可见像素(隔 4px 采样,足够判断空帧) */
function frameHasContent(
  img: HTMLImageElement,
  sx: number,
  sy: number,
  fw: number,
  fh: number,
): boolean {
  const probe = document.createElement("canvas");
  probe.width = fw;
  probe.height = fh;
  const pctx = probe.getContext("2d", { willReadFrequently: true });
  if (!pctx) return true; // 拿不到 context 时保守认为有内容
  pctx.drawImage(img, sx, sy, fw, fh, 0, 0, fw, fh);
  try {
    const data = pctx.getImageData(0, 0, fw, fh).data;
    for (let i = 3; i < data.length; i += 4 * 16) {
      if (data[i] > 16) return true;
    }
    return false;
  } catch {
    return true; // 跨域等异常时保守认为有内容
  }
}

// ---------- 渲染器主体 ----------

export class SpriteRenderer implements PetRenderer {
  private manifest: PetManifest;
  /** 精灵图 Image 对象(异步加载) */
  private image: HTMLImageElement | null = null;
  /** canvas 与 2d context */
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;

  // 分层状态机字段
  /** 当前业务状态 */
  private status: PetStatus = "idle";
  /** 当前精灵图状态行名 */
  private rowName: string = "idle";
  /** 当前行号(从 stateRows 解析) */
  private rowIndex = 0;
  /** 当前状态开始播放的时间戳(ms,用于循环计时) */
  private stateStart = 0;

  // 有效帧缓存:行名 → 该行有效帧数(自动检测)
  private validFrames = new Map<string, number>();

  // 微动效字段
  /** 呼吸相位(随机初相避免所有宠物同频) */
  private breathePhase = Math.random() * Math.PI * 2;
  /** 下一次眨眼时间戳 */
  private nextBlinkAt = 0;
  /** 眨眼进行中的起始时间戳(-1 表示未在眨眼) */
  private blinkStart = -1;

  /** 挥手互动:挥手动画结束时间戳(-1 未在挥手) */
  private greetUntil = -1;
  /** 挥手前的业务状态(挥手结束后恢复) */
  private statusBeforeGreet: PetStatus = "idle";

  /** requestAnimationFrame id */
  private rafId = 0;
  /** 已销毁标记 */
  private destroyed = false;

  constructor(manifest: PetManifest) {
    this.manifest = manifest;
    // 预取行名(保证 rowName 有效;无效行名回退 idle)
    const rows = manifest.stateRows ?? {};
    if (rows[this.rowName] === undefined) this.rowName = "idle";
    this.rowIndex = rows[this.rowName] ?? 0;
  }

  // ---------- 协议读取辅助 ----------

  private get frameWidth(): number {
    return this.manifest.frameWidth ?? DEFAULT_FRAME_W;
  }

  private get frameHeight(): number {
    return this.manifest.frameHeight ?? DEFAULT_FRAME_H;
  }

  private get framesPerRow(): number {
    return this.manifest.framesPerRow ?? DEFAULT_FRAMES_PER_ROW;
  }

  /** 当前行的有效帧数(自动检测,未检测前按上限) */
  private frameCountFor(rowName: string): number {
    const detected = this.validFrames.get(rowName);
    if (detected !== undefined && detected > 0) return detected;
    return Math.min(this.manifest.framesPerState ?? this.framesPerRow, this.framesPerRow);
  }

  private loopMsFor(): number {
    const base = this.manifest.loopMs ?? DEFAULT_LOOP_MS;
    return Math.round(base * STATUS_TUNING[this.status].loopScale);
  }

  private scale(): number {
    return this.manifest.scale ?? DEFAULT_SCALE;
  }

  // ---------- PetRenderer 接口实现 ----------

  mount(container: HTMLElement): void {
    const fw = this.frameWidth;
    const fh = this.frameHeight;
    const scale = this.scale();
    const dpr = window.devicePixelRatio || 1;

    // 创建 canvas:逻辑尺寸 = 帧尺寸 × 缩放;物理像素按 dpr 放大保证清晰
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(fw * scale * dpr);
    canvas.height = Math.round(fh * scale * dpr);
    canvas.style.width = `${Math.round(fw * scale)}px`;
    canvas.style.height = `${Math.round(fh * scale)}px`;
    canvas.style.transformOrigin = "50% 100%"; // 呼吸/眨眼从底部缩放
    canvas.style.pointerEvents = "none";
    canvas.className = "pet-sprite-canvas";
    container.appendChild(canvas);
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.ctx?.scale(dpr, dpr);

    // 加载精灵图
    const src = this.manifest.spritesheetPath;
    if (!src) {
      console.warn(`[SpriteRenderer] ${this.manifest.id} 缺少 spritesheetPath`);
      return;
    }
    const img = new Image();
    img.onload = () => {
      if (this.destroyed) return;
      this.image = img;
      this.detectValidFrames(img); // 自动检测每行有效帧数
      this.stateStart = performance.now();
      this.scheduleNextBlink();
      this.startLoop();
    };
    img.onerror = () => {
      console.warn(`[SpriteRenderer] ${this.manifest.id} 精灵图加载失败:${src}`);
    };
    img.src = src;
  }

  play(state: PetStatus): void {
    if (this.destroyed) return;
    // 挥手动画期间忽略业务状态切换,挥手结束后恢复最新状态
    if (performance.now() < this.greetUntil) {
      this.statusBeforeGreet = state;
      return;
    }
    this.setStatus(state);
  }

  /** 交互反馈:播放 waving 行一个循环(约等于挥手),结束后恢复原状态 */
  greet(): void {
    if (this.destroyed || !this.image) return;
    const rows = this.manifest.stateRows ?? {};
    const waveRow = rows["waving"];
    if (waveRow === undefined) return; // 素材没有 waving 行,跳过
    this.statusBeforeGreet = this.status;
    const now = performance.now();
    this.rowIndex = waveRow;
    this.stateStart = now;
    // 挥手时长 = 该行一个循环
    this.greetUntil = now + (this.manifest.loopMs ?? DEFAULT_LOOP_MS);
  }

  destroy(): void {
    this.destroyed = true;
    cancelAnimationFrame(this.rafId);
    this.canvas?.remove();
    this.canvas = null;
    this.ctx = null;
    this.image = null;
  }

  // ---------- 状态机内部逻辑 ----------

  /** 设置业务状态:更新行/参数/循环起点(带淡入,不硬切) */
  private setStatus(status: PetStatus): void {
    if (status === this.status) return;
    this.status = status;
    const rows = this.manifest.stateRows ?? {};
    const target = STATUS_TO_ROW[status] ?? "idle";
    const row = rows[target] ?? 0;
    // 行变化 → 切换精灵图状态行;行相同(如 sleeping/idle 都用 idle 行)
    // 只更新节奏参数,循环不中断,过渡更顺滑
    if (row !== this.rowIndex) {
      this.rowIndex = row;
      this.rowName = target;
      this.stateStart = performance.now();
    }
    // 状态切换不重置眨眼计时(否则频繁切换会无限推迟眨眼);
    // 但目标状态禁眨眼时,立即中止进行中的眨眼
    if (!STATUS_TUNING[status].blink) {
      this.blinkStart = -1;
    }
  }

  /** 随机安排下一次眨眼时间 */
  private scheduleNextBlink(): void {
    this.nextBlinkAt =
      performance.now() + BLINK_MIN_MS + Math.random() * (BLINK_MAX_MS - BLINK_MIN_MS);
  }

  /** 检测每行有效帧数:跳过空帧(素材行内帧数不足时自动适配) */
  private detectValidFrames(img: HTMLImageElement): void {
    const rows = this.manifest.stateRows ?? {};
    const fw = this.frameWidth;
    const fh = this.frameHeight;
    const limit = Math.min(this.manifest.framesPerState ?? this.framesPerRow, this.framesPerRow);
    for (const [name, row] of Object.entries(rows)) {
      let count = 0;
      for (let c = 0; c < limit; c++) {
        const sx = c * fw;
        const sy = row * fh;
        // 内容超过帧边界视为空帧
        if (sx + fw > img.naturalWidth || sy + fh > img.naturalHeight) break;
        if (frameHasContent(img, sx, sy, fw, fh)) count++;
        else break; // 遇到第一个空帧即截断(帧按顺序排列)
      }
      this.validFrames.set(name, count);
    }
    if (this.validFrames.size === 0) {
      console.warn(`[SpriteRenderer] ${this.manifest.id} 未检测到有效帧,使用默认帧数`);
    }
  }

  // ---------- 渲染循环 ----------

  private startLoop(): void {
    const tick = (now: number) => {
      if (this.destroyed) return;
      this.draw(now);
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  }

  /** 单帧绘制:计算帧位置 + 微动效变换,绘制到 canvas */
  private draw(now: number): void {
    const ctx = this.ctx;
    const img = this.image;
    if (!ctx || !img || !this.canvas) return;

    const fw = this.frameWidth;
    const fh = this.frameHeight;
    const scale = this.scale();

    // ---- 1. 挥手动画处理:到期恢复业务状态 ----
    if (this.greetUntil >= 0 && now >= this.greetUntil) {
      this.greetUntil = -1;
      this.setStatus(this.statusBeforeGreet);
    }

    // ---- 2. 计算当前帧 ----
    const frameCount = this.frameCountFor(this.rowName);
    const loopMs = this.loopMsFor();
    const elapsed = now - this.stateStart;
    // 循环进度 0~1(平滑取模)
    const t = ((elapsed % loopMs) + loopMs) % loopMs / loopMs;
    // 缓动映射:起步/收步稍停,中间流畅 —— 比机械步进自然
    const eased = easeInOutSine(t);
    let frame = Math.floor(eased * frameCount);
    if (frame >= frameCount) frame = frameCount - 1; // 边界保护

    // ---- 3. 计算微动效变换(呼吸 + 眨眼) ----
    const tuning = STATUS_TUNING[this.status];
    // 呼吸:scaleY 正弦起伏,幅度来自状态参数
    const breathe =
      1 + tuning.breatheAmp * Math.sin((2 * Math.PI * now) / tuning.breatheMs + this.breathePhase);
    // 眨眼:到点触发,闭合期内 scaleY 压扁(瞬间闭眼又睁开)
    let blink = 1;
    if (this.blinkStart >= 0) {
      const bp = (now - this.blinkStart) / BLINK_CLOSE_MS;
      if (bp >= 1) {
        this.blinkStart = -1; // 眨眼结束
        this.scheduleNextBlink();
      } else {
        // 前半闭眼、后半睁眼,整体 150ms
        blink = bp < 0.5 ? lerp(1, 0.08, bp * 2) : lerp(0.08, 1, (bp - 0.5) * 2);
      }
    } else if (tuning.blink && now >= this.nextBlinkAt) {
      this.blinkStart = now;
    }

    // ---- 4. 状态切换淡入(从 0 渐显,不硬切) ----
    const sinceSwitch = now - this.stateStart;
    const alpha = sinceSwitch < FADE_IN_MS ? sinceSwitch / FADE_IN_MS : 1;

    // ---- 5. 绘制 ----
    ctx.clearRect(0, 0, fw * scale, fh * scale);
    ctx.save();
    ctx.globalAlpha = alpha;
    // 呼吸 + 眨眼:从底部(脚底)缩放,表现「肚子起伏 / 闭眼」的活物感
    ctx.translate((fw * scale) / 2, fh * scale);
    ctx.scale(1, breathe * blink);
    ctx.translate(-(fw * scale) / 2, -(fh * scale));
    // 透明窗口:绘制时跳过纯透明像素(由素材 alpha 决定)
    ctx.drawImage(
      img,
      frame * fw,      // 源 x(列 = 帧)
      this.rowIndex * fh, // 源 y(行 = 状态)
      fw,
      fh,
      0,
      0,
      fw * scale,
      fh * scale,
    );
    ctx.restore();
  }
}
