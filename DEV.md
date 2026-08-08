# Mo 开发手册

跨平台桌面宠物 + 系统监控(Tauri 2 + React 18 + Rust)。本手册是 agent 与开发者协作的基准文档,新功能开发、重构、代码审查时先读这里。

---

## 1. 项目概览

| 项 | 说明 |
|---|---|
| 定位 | 桌面宠物 + CPU/内存监控,宠物随负载变换表情动画 |
| 技术栈 | Tauri 2 · React 18 · TypeScript · Rust · sysinfo · i18next · Vite |
| 前端入口 | `src/main.tsx` → `src/App.tsx` |
| 后端入口 | `src-tauri/src/main.rs` → `lib.rs` → `app::run()` |
| 窗口 | 单透明置顶无边框窗口(200×200),跳过任务栏 |

## 2. 架构与模块结构

```
src/                          # 前端(React + TS)
├── App.tsx                   主应用:状态协调、宠物/面板/设置开关
├── main.tsx                  入口
├── components/               UI 组件
│   ├── Pet.tsx               宠物(渲染器驱动 + 手势表:拖拽/单击/双击/右键,见 2.4)
│   ├── CpuBubble.tsx         CPU 数字气泡(独立 2s 轮询,与宠物主体渲染解耦,见 2.5)
│   ├── SystemInfoPanel.tsx   系统信息面板(进度条)
│   └── SettingsModal.tsx     设置弹窗(语言/置顶/最小化/退出)
├── renderers/                渲染器抽象层(P1-3 新增,为 Live2D/Spine 预留)
│   ├── types.ts              PetManifest(协议)+ PetRenderer 接口
│   ├── sprite-renderer.ts    精灵图渲染器:canvas 帧动画 + 分层状态机 + 自然动效
│   └── index.ts              createRenderer 工厂:按 manifest.type 分发
├── assets/pets/<pet-id>/     宠物素材库(每宠物一个目录:pet.json + spritesheet.png + index.ts)
├── hooks/useSystemInfo.ts    数据轮询 hooks(useCpuUsage / usePetStatus 等)
├── services/system.ts        Tauri 命令封装层(唯一调 invoke 的地方)
├── i18n/                     国际化(locales/zh.json · en.json)
├── types/index.ts            共享类型(PetStatus/SystemInfo 等)
├── utils/status.ts           纯函数:getStatus(CPU 阈值 → PetStatus)
└── styles.css                样式(布局/气泡/面板/右键菜单;宠物五官动画已迁入渲染器)

src-tauri/src/                # 后端(Rust)
├── app.rs                    命令 + 后台监测 + 托盘 + 窗口控制 + 入口
├── lib.rs                    库入口,声明 mod app
└── main.rs                   二进制入口
```

### 数据流

```
React 组件 → services/system.ts → Tauri invoke → Rust 命令
Rust:后台线程轮询 sysinfo → 写入 AppState(Mutex 缓存) → 命令读缓存返回
```

关键机制:Rust 后台线程每 1s 轮询 CPU/内存写入 `AppState` 的 `Mutex`;前端 2s 轮询读缓存(气泡 2s / 宠物状态 2s,见 2.5),前端零阻塞。

### 2.3 渲染器抽象层(P1-3 落地,精灵图 + 自然动效)

> P1-3 把宠物渲染从「CSS 五官 + 5 态切换」升级为「渲染器抽象层 + 精灵图」,并为将来演进 Live2D / Spine 预留了统一接口。素材与渲染技术彻底解耦:**换宠物 = 换素材目录,不改代码;换渲染技术 = 新增渲染器,组件零改动。**

#### 分层结构

```
src/renderers/
├── types.ts              PetManifest(宠物清单协议)+ PetRenderer(渲染器统一接口)
├── sprite-renderer.ts    SpriteRenderer:canvas 帧动画 + 分层状态机 + 微动效 + 平滑过渡
└── index.ts              createRenderer(manifest) 工厂:按 type 分发,未知/未实现回退 sprite 并告警

src/assets/pets/<pet-id>/   一个宠物 = 一个目录(素材可更换设计)
├── spritesheet.png       精灵图(等尺寸网格,TexturePacker 标准导出可直接用)
├── pet.json              完整协议声明(见下)
└── index.ts              把相对路径解析成 vite 资源 URL,导出 PetManifest
```

#### PetRenderer 接口(渲染技术无关)

```ts
interface PetRenderer {
  mount(container: HTMLElement): void;  // 创建 canvas 等 DOM 并插入容器,启动渲染循环
  play(state: PetStatus): void;         // 播放业务状态(分层状态机入口,内部映射状态行)
  greet?(): void;                       // 可选:交互反馈动画(如点击挥手)
  destroy(): void;                      // 停止循环、移除 DOM、释放资源
}
```

- `Pet.tsx` **只依赖 `createRenderer` + `PetRenderer`**,不感知渲染技术;`manifest.type` 为 `live2d` / `spine` 时工厂告警并回退 sprite(旧版应用面对升级后的 manifest 不崩)。
- 渲染循环用 `requestAnimationFrame` 驱动 canvas 原生绘制,**不触发 React re-render**。

#### pet.json 协议(完整字段)

| 字段 | 说明 | 默认值 |
|---|---|---|
| `id` / `displayName` / `description` | 宠物标识与展示信息 | — |
| `type` | 渲染类型:`"sprite"`(已实现)/ `"live2d"` / `"spine"`(预留) | `sprite` |
| `spritesheetPath` | 精灵图路径(vite 下解析为资源 URL) | — |
| `frameWidth` / `frameHeight` | 单帧尺寸(px) | 192 / 208 |
| `framesPerRow` | 精灵图每行帧数(列数) | 8 |
| `framesPerState` | 每状态最多播放帧数(上限;渲染器按像素内容自动检测有效帧,空帧自动截断) | 取 `framesPerRow` |
| `stateRows` | 状态 → 行号映射(行 = 状态,列 = 帧动画) | Codex 九行分类法 |
| `loopMs` | 单个状态完整循环时长(ms) | 1100 |
| `scale` | 显示缩放(相对原始帧) | 0.33 |

首发宠物 `qqpet-codex`:`frameWidth/frameHeight = 192×208`、`framesPerRow = 8`(8 列 × 9 行)、`stateRows` 九行(idle / running-right / running-left / waving / jumping / failed / waiting / running / review)、`loopMs = 1100`、`scale = 0.4`。

#### 分层状态机与自然动效(核心诉求「还原 QQ 宠物自然感」)

1. **分层状态机**:业务状态(`PetStatus`)→ 精灵图行名(`STATUS_TO_ROW`)→ 行号/帧号,上层只管业务状态。映射:`sleeping/idle → idle 行`、`thinking → waiting 行`、`working → running 行`、`overload → jumping 行`。
2. **微动效叠加**:呼吸(scaleY 正弦起伏,每状态独立幅度/周期/随机初相)+ 眨眼(2.6~5.2s 随机触发,150ms 压扁闭合;状态切换**不重置眨眼计时**,否则频繁切换会无限推迟眨眼)。
3. **平滑过渡**:状态切换淡入 180ms;行不变(如 sleeping/idle 同用 idle 行)只换节奏不重启循环。
4. **节奏自然化**:`easeInOutSine` 映射帧位置(起步/收步稍停、中间流畅)+ 每状态独立 `loopScale`(sleeping 放慢 1.6×、overload 加快 0.7×)。

#### 演进路径(Live2D / Spine)

- 新增 `live2d-renderer.ts` / `spine-renderer.ts`,实现同一 `PetRenderer` 接口,在 `createRenderer` 工厂的 `switch` 中注册即可;
- `Pet.tsx` 与协议层(`PetManifest`)零改动,只需把宠物目录换成 Live2D 模型 + 对应 `type` 的 manifest。

#### 素材可更换设计

- 宠物 = 一个目录(spritesheet + pet.json 声明帧尺寸/状态行/循环时间/缩放),渲染器只认协议,**不硬编码任何宠物长相**;
- 换宠物 = 换目录(参考 `src/assets/pets/qqpet-codex/`),不改代码。

### 2.4 手势表(P1-2 落地)

> 借鉴 Hermes「完整手势表」:拖拽 / 单击 / 双击 / 右键各司其职。判定逻辑集中在 `Pet.tsx`,右键菜单由 `App.tsx` 协调渲染,`styles.css` 提供样式;纯前端实现,**零 Rust 改动**,web 调试全可验证。

| 手势 | 判定 | 行为 |
|---|---|---|
| 拖拽 | pointer 事件链(pointerdown/move/up/cancel),位移 > `DRAG_THRESHOLD`(5px)判定为拖拽 | 移动宠物;位置写入 localStorage(`mo.pet.position`),刷新后保留;拖出屏幕边界自动 clamp 回可视区 |
| 单击 | 延迟 `CLICK_DELAY`(250ms)判定(等待双击窗口) | 切换信息面板 |
| 双击 | dblclick,取消挂起的单击 | 触发挥手动画(greet);独立覆盖窗占位 no-op(P1-1 未落地,TODO 标注) |
| 右键 | contextmenu,`preventDefault` 阻止浏览器默认菜单 | 上报 `clientX/clientY` 给 App 层,弹自定义菜单(设置 / 退出);屏幕边缘 clamp,全屏透明遮罩点击关闭 |

#### 实现要点

1. **拖拽判定**:`setPointerCapture` 保证鼠标移出元素后仍能收到 move/up;位移超过 5px 才判定为拖拽,未超阈值回落为单击(自然支持「点一下不动」)。仅左键拖拽(`e.button !== 0` 直接忽略)。
2. **拖拽后 click 抑制**:浏览器在 pointerup 释放后仍会派发 click,用 ref(`isDraggingRef`)而非 state 标记拖拽中——state 更新有延迟,click 到达时可能已重置;ref 保证标记保持到 click 消费,避免拖拽结束误触发面板切换。
3. **单击/双击分离**:单击延迟 250ms 判定(给双击留窗口);第二次点击取消挂起单击,交给随后的 dblclick 触发双击;双击也主动清掉挂起单击,防双触发。
4. **位置持久化**:拖拽结束把最终位置写入 `localStorage`;组件初始化时读取并校验(x/y 为 number,损坏 JSON / 非法值回退默认底部居中布局);存储不可用(隐私模式)时静默降级为会话内有效。
5. **首次拖拽固化**:默认布局是 CSS 底部居中 + `translateX(-50%)`,首次按下时用 `getBoundingClientRect()` 补偿水平位移,固化为可写的 absolute left/top(styles.css `.pet.positioned` 接管,取消居中 transform)。
6. **右键菜单**:`Pet.tsx` 只上报坐标,不感知菜单内容(App 层 `onContextMenu(x, y)` 回调);App 层做屏幕边缘 clamp(菜单不超出视口)+ 全屏透明遮罩(`.context-menu-overlay`)点击/右键关闭并拦截下方交互。菜单项:设置(打开设置弹窗)、退出(no-op 占位,web 下无效)。

### 2.5 状态渲染解耦(P1-4 落地)

> 目标:CPU 数值每秒波动,但宠物主体(动画/呼吸/眨眼)不应随之频繁 re-render。手段:①轮询降频(气泡 1s → 2s);②`usePolling` 增加 `isEqual` 参数(值相等不 setState);③CPU 气泡独立成组件。

#### 数据通道

```
usePetStatus(2s)  → 宠物主体(Pet.tsx)状态驱动:getStatus(cpu) 映射离散 PetStatus,isEqual 值相等不 setState
useCpuUsage(2s)   → CpuBubble 独立组件:数字实时跳动只更新气泡自身
useSystemInfo(2s) → 信息面板:CPU + 内存
```

#### 关键机制

1. **`usePolling` 加 `isEqual` 参数**(P1-4 核心):`setData(prev => isEqual(prev, value) ? prev : value)`——新旧值相等时保留旧引用,React 跳过 re-render。`isEqual` 用 ref 持有,内联比较函数不会导致 effect 重建。
2. **`usePetStatus`(新增,2s 轮询)**:`getStatus(await getCpuUsage())` 把连续 CPU 数值映射为离散状态(`overload/working/thinking/idle/sleeping`),`isEqual: (a, b) => a === b`——CPU 在同一状态区间内波动时,宠物主体**零 re-render**;只有跨阈值切换状态才更新,并经由渲染器 `play(status)` 平滑过渡。
3. **`CpuBubble` 独立组件(新增,2s 轮询)**:数字气泡自持 `useCpuUsage(2000)`,数据变化只更新气泡 DOM,与宠物主体互不影响。
4. **渲染循环本就不经过 React**:P1-3 渲染器用 requestAnimationFrame 驱动 canvas 原生绘制,天然不受 React 更新影响;解耦后 React 更新频率进一步降低到「仅状态切换」。

#### 双通道评估结论(记录不改)

- **现状**:monitor 线程同一份缓存写两处(`cpu_usage` + `system_info.cpu_usage`),前端 `getCpuUsage` / `getSystemInfo` / `getMemoryInfo` 分别轮询读取(对应三组 hook)。
- **评估**:两个命令读的是同一 `Mutex` 缓存的不同字段,合并为单一 `get_system_info` 调用需动 Rust 侧(app.rs 命令定义)与全部调用方,收益有限、风险不小;
- **结论(2026-08-08)**:**本次不改**,保留双通道;留待 P1-5 抽 `monitor.rs` 时与 app.rs 拆分(第 3 节弱点 #1)一并整理。

### 2.6 桌面体验优化(透明窗口 / 桌面漫游 / CPU 平滑,2026-08-08 落地)

> 三项桌面体验优化一起落地:透明悬浮窗、宠物桌面漫游、CPU 数值平滑。前两项涉及 Rust 侧(app.rs)与前端协作,第三项纯前端。

#### 透明窗口(强制 wry 透明路径)

- **背景**:tauri.conf.json 原本就有 `transparent: true` + `decorations: false`,CSS 也是透明,但 niri/Wayland 下启动进程继承 `GDK_BACKEND=x11`(走 xwayland)时出现白标题栏 + 灰底;显式 `GDK_BACKEND=wayland` 后标题栏消失但 webview 内容层仍是 (80,80,80) 灰——config 透明配置在 WebKitGTK 的 alpha 合成路径上未生效。
- **落地**(app.rs setup):config 的 `windows` 数组清空(`"windows": []`),窗口改为 `WebviewWindowBuilder` 显式创建并**链式 `.transparent(true)`**(不依赖 config,强制 wry transparent 路径)+ `.decorations(false)` + `.always_on_top(true)` + `.skip_taskbar(true)`;再调 `window.set_background_color(Some(Color(0,0,0,0)))` 作第二保险。
- **运行要求**(niri/Wayland):必须 `env GDK_BACKEND=wayland WEBKIT_DISABLE_DMABUF_RENDERER=1` 启动,否则 xwayland 下出现白标题栏 / GBM buffer 失败灰屏。
- **实测结论**:透明窗口在 Wayland(`GDK_BACKEND=wayland`)/ X11(xwayland)两路均已尝试:标题栏问题已解决(无边框生效),但 webview 内容层透明均受 **WebKitGTK alpha 合成限制**(内容层始终不透明,实测为 (80,80,80) 灰底)——config 透明配置与 WebviewWindowBuilder 显式 `.transparent(true)` 在 WebKitGTK 上都不生效,属引擎硬伤,非配置问题。
- **后续方案(已立项)**:方案D——渲染下沉 Rust(WebKitGTK alpha 合成不可用时,窗口内容改由 Rust 侧自绘/合成),见 agent 待办,本次不实施。

#### 桌面漫游(roam.ts + move_window)

- **Rust**:app.rs 新增 `move_window(dx, dy)` 命令——读取主窗口 `outer_position()` 后按增量 `set_position(Position::Physical(...))`(物理像素),由前端逐帧调用。
- **前端**:新建 `src/services/roam.ts` 单例漫游控制器:
  - 随机目标点(屏幕内留 `EDGE_MARGIN=60px` 边距);每帧按 `STEP_PER_FRAME=0.8px` 平滑步进;
  - 到达后停留 5~15s 随机再选点;位置越界自动 clamp 并重选目标(「边缘回头」);
  - Tauri 环境调 `moveWindow` 移动窗口;web mock 用 CSS transform 平移宠物元素等效模拟;
  - 屏幕尺寸:Tauri 走 `currentMonitor()`(物理像素),mock 走 `window.innerWidth/Height`。
- **暂停机制**:Pet.tsx 挂载时 `startRoam(petRef.current)`、卸载 `stopRoam()`;pointerdown `pauseRoam()`、pointerup/cancel `resumeRoam()`——用户拖拽时漫游暂停,避免「窗口移动 + 拖拽移动」叠加;mock 下 resume 会重同步基准位置避免跳变。

#### CPU 平滑

- CpuBubble.tsx 对最近 5 次采样做滑动平均(`SMOOTH_WINDOW = 5`)后再显示,数字稳定不随单次采样大幅跳动;宠物主体状态判定(usePetStatus)仍用原始值,滑动平均只影响气泡展示。

### 2.7 Rust 原生渲染(方案D,2026-08-08 阶段1落地)

> 背景:2.6 节结论——透明窗口卡在 **WebKitGTK 内容层 alpha 合成硬伤**(Wayland/X11 两路 webview 内容层均不透明,灰底 (80,80,80))。方案D = 渲染下沉 Rust:窗口内容由 Rust 侧自绘 RGBA 像素缓冲,完全绕开 WebKit,从根上消除「内容层不合成 alpha」问题。

#### 架构:宠物窗口 Rust 绘制 / 面板 React(并存)

- **双渲染路径共存**,由环境变量 `MO_PET_MODE` 切换(app.rs setup):
  - `MO_PET_MODE=rust`(未设/其他值不触发)→ `pet_render::spawn_pet_window(app)` 创建 GTK 原生宠物窗口,不创建任何 Webview;monitor 线程 + 系统托盘照常启动。
  - 默认(未设置)→ 现有 WebKit 路径(WebviewWindowBuilder 显式透明),行为不变。**两条路径并存,互不删除**;面板/设置等 React UI 后续阶段在 WebKit 窗口或 layer-shell 面板窗口呈现。
- 阶段1 只做「Rust 宠物窗口 + 演示动画循环」,面板(React)尚未挂到 Rust 窗口——demo 状态序列(见下)证明 set_state + 动画循环工作。

#### pet_render 模块结构(src-tauri/src/pet_render/,5 文件)

| 文件 | 职责 |
|---|---|
| `mod.rs` | 窗口创建(`spawn_pet_window`)+ 透明 GTK 窗口(DrawingArea 自绘)+ 动画循环(glib timeout 16ms 驱动 tick→draw)+ 演示状态序列 |
| `renderer.rs` | `PetRenderer` 统一接口(与前端 `src/renderers/types.ts` 对齐):`size`/`set_state`/`tick`/`render`;`RenderFrame` = RGBA8 直通缓冲(straight alpha) |
| `sprite.rs` | `SpriteRenderer`:精灵图裁帧 → RGBA 缓冲,对齐前端「自然动效」四要素(分层状态机/呼吸/眨眼/淡入 180ms/easeInOutSine);**纯内部时钟**(tick 喂 dt),不依赖外部时间源 |
| `manifest.rs` | pet.json 协议解析(serde,camelCase,与前端 PetManifest 字段一致,可选字段带默认值) |
| `factory.rs` | 渲染器工厂:按 `type` 分发(sprite 实现;live2d/spine 报错暴露)。素材来源:env `MO_PET_DIR` 优先(可更换),默认编译期内嵌 `src/assets/pets/qqpet-codex/`(include_str!/include_bytes!) |

**透明实现原理**(mod.rs 头注释,绕开 WebKit 的三层):
1. 内容层:渲染器直接产出 RGBA8 像素缓冲(透明像素 alpha=0),不经 WebKit;
2. 窗口层:GTK 窗口 `app_paintable` + RGBA visual + `set_decorated(false)` + `skip_taskbar` + `keep_above`;
3. blit:draw 回调把 RGBA(straight)→ cairo ARGB32(预乘)一次贴图。

**阶段1 实测结论(2026-08-08,如实)**:窗口层透明**已达成**(四角圆角外透出下层,ARGB 表面混合生效、无边框/无标题栏/置顶均验证);**内容层透明未达成**——GTK 主题背景先填充了 DrawingArea,企鹅 blit 在主题背景上(默认主题 (80,80,80) 灰;`GTK_THEME=Adwaita:light` 后变 (78,201,176),实锤主题背景填充,与 WebKitGTK alpha 无关)。**待修(阶段2,一行)**:draw 回调开头 `cr.set_operator(cairo::Operator::Source)` + `set_source_rgba(0,0,0,0)` + `paint()` 清透明,或对 DrawingArea 设 `background: transparent` CSS。

#### MO_PET_MODE 开关与运行方式

- 启动(Rust 路径,不依赖 WebKit/WebView):`env MO_PET_MODE=rust /path/to/release/desktop-pet`
  - niri/Wayland 下推荐仍带 `GDK_BACKEND=wayland`(X11 下 GTK3 RGBA visual 会强制 CSD 白标题栏,属已知 Pitfall 22 同类问题);
  - 素材覆盖:`MO_PET_DIR=<目录>` 从磁盘读 pet.json + spritesheet.png;`MO_PET_SCALE=<f64>` 覆盖显示缩放;
  - 渲染器尺寸 = 帧尺寸 × scale(qqpet-codex 默认 192×208 × 0.4 ≈ 77×83 窗口)。
- 演示状态序列 `DEMO_STATES = ["idle", "waving", "thinking", "working", "jumping"]` 周期切换,证明状态机 + 动画循环工作;后续由面板事件驱动。

#### layer-shell 后续接入点(2026-08-08 实测)

- gtk-layer-shell crate 与 Cargo.lock 的 gtk 0.18 版本兼容,但需要系统库 libgtk-layer-shell(VM 编译机缺且 sudo 需密码装不了)→ 阶段1 降级为普通无边框窗口 + ARGB 自绘。
- 接入点已留:`window.upcast_ref::<gtk::Window>()` 拿到 GTK 窗口后调 `gtk_layer::LayerShell::for_window(...)` 提升即可(poe2-overlay 先例);overlay 层语义(悬浮全屏之上/穿透点击)后续补。

#### 与 WebKit 路径并存说明

- **不删 WebKit 路径**:默认模式仍是 Tauri webview 窗口(面板/设置等 React UI 所在);Rust 宠物窗口是并行方案,不替代。
- 渲染器接口 Rust 侧与前端对齐(PetRenderer 同名同语义),素材协议同一份 pet.json,前端渲染器/素材可直接复用,无概念割裂。

## 3. 模块化评估与改进计划

**结论:整体结构清晰,属于 Tauri + React 标准分层,小规模下优秀。** 前端分层(组件/hooks/services/i18n/types)职责明确,services 层统一封装命令值得保留。

### 弱点与改进计划

| # | 问题 | 现状 | 建议 |
|---|---|---|---|
| 1 | `app.rs` 职责过载 | 命令、监测线程、托盘、窗口控制、入口全在一个文件 | 拆分为 `monitor.rs`(轮询)、`tray.rs`(托盘)、`window.rs`(窗口)、`commands/`(命令),`app.rs` 只做注册 |
| 2 | `types/index.ts` 的 `PetState` 未使用 | Pet.tsx 用内联 `PetStatus` | 统一类型,删除死代码 |
| 3 | `styles.css` 单文件 | 靠注释分区 | 按组件拆文件(可选,现分区注释尚可) |
| 4 | 前后端类型双份定义 | Rust/Typescript 各一份 `SystemInfo` | 小项目可接受,不做 codegen |
| 5 | 三个 hook 结构重复 | useCpuUsage/useMemoryInfo/useSystemInfo 同模式 | 可抽 `usePolling`,非必须 |

## 4. 可借鉴设计(来自 Hermes Desktop 宠物)

对标 Hermes 宠物(精灵图 + 状态驱动 + 弹出覆盖窗 + 漫游 + 情感反馈),Mo 值得吸收的点,按性价比排序:

| # | 借鉴点 | Hermes 做法 | Mo 落地 |
|---|---|---|---|
| 1 | **弹出覆盖窗口**(收益最大) | shift-click 把宠物弹到独立透明置顶窗,可拖拽、位置持久化、主窗最小化后仍可见 | Tauri 配置与命令能力已具备,加第二个透明置顶窗口即可 |
| 2 | **完整手势表** | 拖拽移动 / 单击 / 双击 / shift-click 各司其职 | ✅ **已落地(P1-2)**:拖拽 + localStorage 持久化 / 单击双击分离 / 右键自定义菜单,详见 2.4 |
| 3 | **精灵图替换 CSS 五官** | 状态 → sprite 行 → 帧动画,换宠物只换图集 | ✅ **已落地(P1-3)**:渲染器抽象层 + qqpet-codex,详见 2.3 |
| 4 | **状态与渲染解耦** | 漫游用命令式 el.style,settle 才 commit React | ✅ **已落地(P1-4)**:usePetStatus + isEqual 值相等不 setState;CpuBubble 独立 2s 轮询,详见 2.5 |
| 5 | **阈值告警通知** | 任务完成 → 浮出 mail 图标,点击回到线程 | CPU/内存超阈值 → 警示动画 + 系统通知,让监控"有用" |
| 6 | **点击抚摸反馈** | 词表匹配"good bot"飘爱心(零模型调用) | ✅ **部分落地(P1-3/P1-2)**:单击/双击触发 waving 挥手动画(greet),飘爱心留待 P1-6 |

**明确不借鉴**:CLI/TUI 渲染、3200+ 宠物画廊生态、roam 漫游(surface 感知跳跃依赖 DOM 度量)、多 agent 网关。

**保留的独特优势**:Rust 后台线程零阻塞、托盘集成、i18n、轻量无依赖。Mo 定位守住"系统状态可视化 + 陪伴感",不学 Hermes 变成 AI 副驾驶。

## 5. 宠物记忆系统 + AI 对话 规划

### 5.1 总体架构(不破坏现有结构)

```
React 前端
  PetChat 面板(气泡内嵌 / 独立小窗)
    → Tauri invoke("chat_message", { session_id, text })
Rust 后端
  chat_message(异步任务,不阻塞 UI)
    ├─ 载入会话历史(短期记忆)
    ├─ 检索相关长期记忆 → 注入 system prompt
    ├─ 调 LLM(OpenAI 兼容协议)
    └─ 流式返回 → 前端逐字显示
        └─ 完成后异步提取重要信息 → 写入记忆库
存储:SQLite(rusqlite),位于 app data 目录
```

### 5.2 三层记忆

| 层 | 载体 | 说明 |
|---|---|---|
| 短期记忆 | `messages` 表,按 session 分组 | 对话历史 + 上下文窗口裁剪(最近 N 条 + token 上限) |
| 长期记忆 | `memories` 表 | 人格/用户偏好,按 `type`(fact/preference/event)+ `importance` + 访问时间衰减 |
| 记忆沉淀 | 后台异步提取 | 对话结束后调 LLM 提取值得记住的信息写入记忆库 |

### 5.3 数据表设计

```sql
CREATE TABLE sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT
);

CREATE TABLE messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id INTEGER REFERENCES sessions(id),
  role TEXT,                -- user | assistant | system
  content TEXT,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE memories (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  content TEXT NOT NULL,
  type TEXT,                -- fact | preference | event
  importance REAL DEFAULT 0.5,
  last_accessed TEXT,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE memory_sources (   -- 记忆溯源(可选)
  memory_id INTEGER REFERENCES memories(id),
  message_id INTEGER REFERENCES messages(id)
);
```

### 5.4 Rust 模块规划(结合第 3 节拆分)

```
src-tauri/src/
├── app.rs           # 入口:注册命令、初始化 DB、启动线程
├── commands/
│   ├── mod.rs
│   ├── chat.rs      # create_session / list_sessions / chat_message
│   └── memory.rs    # get_memories / add_memory / delete_memory
├── chat/
│   ├── mod.rs
│   ├── llm.rs       # OpenAI 兼容客户端(SSE 流式)
│   ├── context.rs   # 上下文窗口 + 记忆注入
│   └── extract.rs   # 对话后记忆提取
├── store/
│   ├── mod.rs
│   ├── db.rs        # SQLite 连接(单例)
│   ├── sessions.rs
│   └── memories.rs
├── monitor.rs       # 从 app.rs 抽出的 CPU 轮询线程
├── tray.rs          # 系统托盘
└── window.rs        # 窗口控制
```

### 5.5 命令接口

| 命令 | 签名 | 说明 |
|---|---|---|
| `create_session` | `() -> session_id` | 新建会话 |
| `list_sessions` | `() -> Vec<SessionMeta>` | 历史会话列表 |
| `chat_message` | `(session_id, text) -> 流式` | 核心对话,channel/Event 推送增量文本 |
| `get_memories` | `() -> Vec<Memory>` | 查看记忆库 |
| `add_memory` | `(content, type)` | 手动记一条 |
| `delete_memory` | `(id)` | 删除记忆 |

### 5.6 关键决策

- **模型协议**:OpenAI 兼容(`async-openai` 或 reqwest 手写),DeepSeek/智谱/OpenRouter/Ollama/OpenAI 通用
- **配置**:环境变量 `PET_LLM_BASE_URL` / `PET_LLM_API_KEY` / `PET_LLM_MODEL`,API key 不进前端
- **流式**:SSE + Tauri 2 channel/Event,逐字显示
- **聊天入口**:宠物气泡内嵌 + 右上角展开面板

### 5.7 Prompt 模板

记忆注入(系统提示):
```
你是桌面宠物 [名字],住在用户桌面上。关于用户的记忆:
- {记忆}
对话历史:
{最近消息}
```

记忆提取(对话后调用):
```
从对话中提取值得长期记住的信息(用户偏好、事实、重要事件)。
只输出 JSON 数组:[{"content":"...","type":"fact|preference|event"}]
无值得记的内容输出 []
```

### 5.8 分阶段路线图(逐步实施步骤)

> 每一步标注「改动文件 → 验证」。按序执行,每阶段完成即产生可运行成果。

#### P1 能对话(先跑通,非流式)

| # | 步骤 | 改动文件 | 验证 |
|---|---|---|---|
| 1 | 加 LLM 依赖:reqwest(开启 json/stream 特性)+ tokio | `src-tauri/Cargo.toml` | `cargo check` 通过 |
| 2 | 读取配置:环境变量 `PET_LLM_BASE_URL` / `PET_LLM_API_KEY` / `PET_LLM_MODEL`,提供缺省值与报错 | 新建 `src-tauri/src/chat/llm.rs` | 打印配置正确 |
| 3 | 实现 OpenAI 兼容 `chat_completion(messages) -> String`(非流式,先跑通) | `chat/llm.rs` | 单元测试或手动 curl 对比 |
| 4 | 定义 `chat_message(session_id, text) -> String` 命令,先不接库,直接调 LLM 返回 | 新建 `src-tauri/src/commands/chat.rs` | 命令可被前端调用 |
| 5 | 注册命令:`invoke_handler!` 加 `chat_message`;初始化时 manage LLM 客户端到 AppState | `src-tauri/src/app.rs` | `cargo check` |
| 6 | 前端封装:`services/system.ts` 加 `chatMessage(text)` | `src/services/system.ts` | `pnpm build` 通过 |
| 7 | 新建 `PetChat` 组件:气泡内嵌输入框 + 消息列表(先非流式整句显示) | 新建 `src/components/PetChat.tsx` + `styles.css` | 对话可用 |
| 8 | App.tsx 接入聊天面板开关(宠物气泡点击展开,需与现有信息面板互斥) | `src/App.tsx` | `pnpm tauri dev` 手动对话 |

**P1 完成标准**:点宠物能弹出聊天,输入后收到 LLM 回复。

#### P2 有记忆(SQLite + 记忆注入)

| # | 步骤 | 改动文件 | 验证 |
|---|---|---|---|
| 9 | 加 rusqlite(带 bundled 特性)依赖 | `src-tauri/Cargo.toml` | `cargo check` |
| 10 | `store/db.rs`:打开 app data 目录下 `pet_memory.db`,`CREATE TABLE IF NOT EXISTS` 建 4 张表(见 5.3),封装单例连接 | 新建 `src-tauri/src/store/db.rs` | 首次启动建表日志 |
| 11 | `store/sessions.rs` + `store/memories.rs`:增删查实现 | 新建两个文件 | 单元测试读写 |
| 12 | 建会话命令:`create_session` / `list_sessions` | `commands/chat.rs` | 命令可调用 |
| 13 | 改造 `chat_message`:消息写入 messages 表;载入最近 N 条历史;按 token 上限裁剪上下文窗口 | `commands/chat.rs` | 连续两轮能记住上文 |
| 14 | 记忆注入:查询 `memories` 表(按重要度/访问时间),拼入 system prompt(模板见 5.7) | `chat/context.rs`(新建)+ `commands/chat.rs` | 记忆中信息出现在回复 |
| 15 | 记忆命令:`get_memories` / `add_memory` / `delete_memory` | `commands/memory.rs`(新建) | 命令可调用 |
| 16 | 前端:PetChat 加载会话历史;可选"记忆管理"子面板 | `src/components/PetChat.tsx` | 重启后历史仍在 |

**P2 完成标准**:重启应用后能接着上次对话;手动写入的记忆会影响宠物回答。

#### P3 会沉淀(自动提取 + 衰减)

| # | 步骤 | 改动文件 | 验证 |
|---|---|---|---|
| 17 | `chat/extract.rs`:对话结束后后台异步调 LLM,按提取模板(5.7)输出 JSON,解析后写入 memories | 新建 `src-tauri/src/chat/extract.rs` | 多轮对话后记忆库自动增长 |
| 18 | 去重与阈值:相同 content 不重复入库;importance 低于阈值跳过 | `chat/extract.rs` | 重复对话不刷屏记忆 |
| 19 | 衰减:每次检索更新 `last_accessed`;检索排序加权 `importance * 衰减因子`;长期未用降权 | `store/memories.rs` + `chat/context.rs` | 久远记忆优先级下降 |
| 20 | 前端记忆管理视图:查看/删除记忆 | `src/components/` | 可人工清理记忆库 |

**P3 完成标准**:宠物能主动记住用户偏好并在后续对话中体现;记忆可被查看与删除。

#### 后续优化(可插队)

- **流式输出**:`chat_message` 改为 Tauri 2 channel 逐块推送,前端 `usePetChat` 增量拼接(替代 P1 的整句返回)
- **对话情感反馈**:结合第 4 节借鉴点 6,宠物回答后对特定词句做爱心/撒娇动画
- **阈值告警联动**:CPU/内存超阈值时,宠物主动提醒并可一键追问(借鉴点 5)

## 6. 开发规范

### 常用命令

```bash
pnpm install          # 安装依赖
pnpm tauri dev        # 开发模式
pnpm tauri build      # 构建生产版本
pnpm build            # 仅前端构建(tsc + vite)
cargo fmt / check     # 在 src-tauri 下,格式/检查
```

### 新增功能流程(遵循现有模式)

1. **新 Tauri 命令**:Rust 端在 `app.rs`(或拆分后的 `commands/`)定义 `#[tauri::command]` → 注册进 `invoke_handler!` → 前端在 `services/system.ts` 封装成函数 → 组件里调用。**前端禁止直接 `invoke`,一律走 services 层。**
2. **新 UI 组件**:放 `src/components/`,纯展示组件 + 由 App 协调状态。
3. **新数据轮询**:参照 `useCpuUsage` 模式(useEffect + setInterval + 失败静默)。
4. **新文案**:加进 `src/i18n/locales/zh.json` 与 `en.json`,用 `t("key")` 引用,不硬编码。
5. **新持久化**:放 Rust 端,不要用 localStorage 存应用逻辑数据(语言偏好等轻配置除外)。
6. **新宠物素材**:在 `src/assets/pets/<pet-id>/` 新建目录(pet.json 按 2.3 协议声明帧尺寸/状态行/循环时间/缩放 + spritesheet.png + index.ts 导出 manifest),组件零改动。
7. **新渲染技术(Live2D/Spine)**:新建渲染器实现 `PetRenderer` 接口,在 `src/renderers/index.ts` 工厂注册;`Pet.tsx` 不动。

### 约定

- 宠物外观/动画一律收敛到渲染器,`styles.css` 只负责布局与面板;**不要**回到 CSS 五官/动画方案。
- 宠物渲染循环用 `requestAnimationFrame` + canvas,不触发 React re-render;改渲染器时保持该约定。
- 透明窗口下避免深色背景铺满,保持 `background: transparent`。
- 内存型后台任务(监测线程)用 `AppState + Mutex` 缓存,命令只读缓存,不做同步 IO。

---

## 7. 分层架构规划(核心独立,远期方向)

### 7.1 动机与目标

应对三个诉求,结论先行:

| 诉求 | 结论 |
|---|---|
| Linux + Windows 桌面 | 当前 Tauri 2 已跨平台,不构成换架构理由 |
| 全栈 Rust(摆脱 JS/TS) | 可选路径:Slint / egui,不影响核心逻辑 |
| 未来移植 ESP32 | **核心逻辑独立成 crate**,UI 层各自实现 |

**核心原则**:把业务逻辑与 UI 框架解耦,让"换 UI"和"移植新平台"不触碰核心代码。

### 7.2 目标结构

```
mo-core(平台无关 Rust crate,核心逻辑)
├─ pet/state.rs       宠物状态机:CPU 阈值 → PetStatus(现有 getStatus 纯函数化)
├─ store/             记忆库(sessions/messages/memories,表结构见 5.3)
├─ chat/              AI 对话客户端(OpenAI 兼容,配置由 UI 层注入)
└─ sysinfo/           SystemInfoProvider trait(抽象系统信息采集)
    ├─ desktop 实现   sysinfo 库(现有 monitor.rs 逻辑)
    └─ embedded 实现  传感器/低资源环境(远期 ESP32)

各端 UI(依赖 mo-core):
├─ desktop 桌面       当前 Tauri 2 + React(远期可换 Slint/egui,不动 core)
└─ embedded 嵌入式    LCD + 简化动画(Slint MCU / embedded-graphics,远期)
```

### 7.3 依赖方向与约束

- **单向依赖**:UI 层 → 核心层。核心层禁止依赖 tauri / webview / React。
- **trait 抽象**:系统信息采集(`SystemInfoProvider`)与持久化(`MemoryStore`)用 trait,桌面(线程 + SQLite)与嵌入式(传感器 + Flash)各提供实现。
- **配置注入**:LLM 的 `BASE_URL/API_KEY/MODEL` 由 UI 层读取后传入核心层,核心层不碰环境变量与密钥。
- **状态机纯函数**:`getStatus(cpu) -> PetStatus` 为纯函数,不依赖任何平台能力,天然可移植。

### 7.4 分步迁移(不阻塞当前功能)

| # | 步骤 | 涉及内容 | 是否阻塞现有开发 |
|---|---|---|---|
| 1 | 新建 `mo-core` crate,把系统信息抽象成 `SystemInfoProvider` trait,抽出纯函数状态机 | 现有 `app.rs` 的监测逻辑 | 否,纯增量 |
| 2 | 记忆库 `store/`(第 5 节表结构)移入核心层 | DEV.md 5.4 的 store 模块 | 否,按原规划做,落点改为 core |
| 3 | AI 对话客户端 `chat/` 移入核心层 | DEV.md 5.4 的 chat 模块 | 否,同 store |
| 4 | Tauri 层瘦身为壳:只留窗口、托盘、命令转发,业务都调 core | `src-tauri/src/` | 在 P1-P3 之后 |
| 5 | (远期)新增 ESP32 前端,复用核心层 | esp-hal + LCD | 不启动,仅规划 |

### 7.5 对现有规划的影响

- **记忆系统(第 5 节)天然属于核心层**——数据表与 AI 客户端本来就是平台无关 Rust,现有 P1-P3 规划按原样实施,只调整落点目录。
- **换 UI 框架不破坏核心逻辑**:若日后从 React 迁到 Slint/egui,只需重写第 2 节前端层与 Tauri 壳,core 不动。
- **分步迁移 Step 1-3 都是增量**:不删除现有代码,只是把逻辑搬进独立 crate,当前功能持续可运行。

### 7.6 决策要点

- 本规划是**远期方向**,不阻塞当前开发。P1-P3 记忆/AI 功能仍按第 5 节在现有架构上实施。
- 是否换纯 Rust UI(Slint/egui)取决于需求定夺(A 全栈 Rust / B 仅跨平台 / C 认真做 ESP32),定夺前不动 UI 层。
- ESP32 是另一个世界(微控制器 + LCD),移植的是核心逻辑而非 UI。
