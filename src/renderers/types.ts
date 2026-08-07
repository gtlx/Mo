// ============================================================
// 宠物渲染器抽象层 —— 类型定义
//
// 目的:为将来演进 Live2D / Spine 预留统一接口。
// 宠物 = 一个「目录(精灵图/模型 + pet.json 声明)」,渲染器只认协议,
// 不硬编码宠物长相;换宠物 = 换目录,不改代码。
//
// 协议演进路径:
//   type: "sprite"  → SpriteRenderer(本阶段实现,canvas 帧动画)
//   type: "live2d"  → Live2dRenderer(将来实现同 PetRenderer 接口)
//   type: "spine"   → SpineRenderer(将来实现同 PetRenderer 接口)
// Pet.tsx 只依赖 PetRenderer,新增渲染器无需改组件。
// ============================================================

import type { PetStatus } from "../types";

/**
 * 宠物渲染类型(渲染器工厂按此分发)
 * - sprite: 精灵图帧动画(canvas 绘制)
 * - live2d / spine: 预留,将来接入对应引擎
 */
export type PetRenderType = "sprite" | "live2d" | "spine";

/**
 * 宠物清单(对应 assets/pets/<id>/pet.json)
 * 协议字段与 Hermes Desktop 宠物协议对齐,并补充完整配置:
 * - 帧尺寸 / 每行帧数 / 状态行号 / 循环时间 / 缩放,全部可配置
 * - TexturePacker 导出的标准精灵图(等尺寸网格)可直接使用
 */
export interface PetManifest {
  /** 宠物唯一 id(如 qqpet-codex) */
  id: string;
  /** 展示名(如 QQpet-codex) */
  displayName: string;
  /** 描述 */
  description?: string;
  /** 渲染类型:渲染器工厂按此分发,未知类型回退到 sprite */
  type: PetRenderType;
  /** 精灵图路径(vite 下为打包后的资源 URL) */
  spritesheetPath?: string;
  /** 单帧宽度(px),默认 192 */
  frameWidth?: number;
  /** 单帧高度(px),默认 208 */
  frameHeight?: number;
  /** 精灵图每行帧数(列数),默认 8 */
  framesPerRow?: number;
  /**
   * 每个状态最多播放的帧数(默认取 framesPerRow)。
   * 渲染器会按实际内容自动检测有效帧数(空帧跳过),
   * 该字段是上限;行内有效帧不足时按有效帧循环。
   */
  framesPerState?: number;
  /** 状态 → 行号映射(行 = 状态,列 = 帧动画),默认 Codex 九行分类法 */
  stateRows?: Record<string, number>;
  /** 单个状态完整循环时长(ms),默认 1100 */
  loopMs?: number;
  /** 显示缩放(相对原始帧尺寸),默认 0.33 */
  scale?: number;
}

/**
 * 渲染器统一接口(Live2D / Spine 将来实现同一接口)
 * Pet.tsx 只调用这些方法,不感知具体渲染技术。
 */
export interface PetRenderer {
  /**
   * 挂载渲染器:创建 canvas 等 DOM 节点并插入容器,开始渲染循环。
   * @param container 宿主容器(通常是 <div>)
   */
  mount(container: HTMLElement): void;
  /**
   * 播放指定业务状态(分层状态机入口):
   * 内部会把 PetStatus 映射到精灵图状态行,并处理过渡动画。
   * @param state 业务状态(sleeping/idle/thinking/working/overload)
   */
  play(state: PetStatus): void;
  /**
   * 触发一次交互反馈动画(如点击时挥手),可选能力。
   * 接口层声明,具体渲染器按素材能力实现。
   */
  greet?(): void;
  /** 销毁渲染器:停止循环、移除 DOM、释放资源 */
  destroy(): void;
}
