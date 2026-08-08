# Mo 开发规划(PLAN.md)

> 本文档是 DEV.md 的**执行细化**,基于 2026-08-08 对项目代码的全面审查。
> DEV.md 是架构基准(不改动),本文档回答「按什么顺序、动哪些文件、依赖什么、怎么验证」。
> 审查结论速览:结构清晰、分层标准;5 个弱点与 DEV.md 第 3 节一致;Rust 工具链本机已损坏(已修复);web 调试可行但 Tauri API 缺失需 mock。

---

## 0. 环境与可行性结论(2026-08-08 实测)

### 0.1 Rust 工具链

- **实测**:`rustc`/`cargo` 报 `Missing manifest in toolchain 'stable-x86_64-unknown-linux-gnu'`,工具链目录存在但 manifest 缺失 → 8/7 报的「工具链损坏」属实。
- **处理(用户纪律,2026-08-08)**:**本机不修复、不编译,保持环境干净**。编译/调试一律走虚拟机 `ssh arch`(virtiofs 共享 `/home/gtlx/项目/code` 同一份代码,虚拟机上 cargo/rustc 齐全)。本机 rustup 损坏属预期,勿在本机跑 `rustup toolchain install`。
- **Tauri Linux 系统依赖已齐**:`webkit2gtk-4.1` / `gtk+-3.0` / `javascriptcoregtk-4.1` / `libsoup-3.0` 全部 `pkg-config` 通过,在虚拟机上 `cargo check` 应可直接跑。
- ⚠️ 首次 `cargo check`/`pnpm tauri dev` 需下载编译 ~400 个 crate,耗时 5–15 分钟,属正常。

### 0.2 Web 调试可行性(vite dev)

| 项 | 结论 |
|---|---|
| 界面渲染 | ✅ `pnpm dev` → http://localhost:1420 完整渲染:宠物(5 态状态机 + CSS 动画)、CPU 气泡、信息面板、设置弹窗、i18n 全部可看可点 |
| JS 错误 | ✅ 无未捕获错误。`useCpuUsage` 等 hook 对 invoke 失败有 `catch` 静默兜底 |
| invoke 缺失表现 | CPU 恒为 0 → 宠物永远 `sleeping` 态;信息面板显示 `Cannot read properties of undefined (reading 'invoke')`;设置弹窗的「最小化/置顶/退出」点击后是 unhandled promise rejection(App.tsx 的 handler 无 try/catch) |
| **结论** | **web 版能跑、能看到界面与交互,但看不到真实数据与窗口行为**。做 UI/交互/动画开发完全够用;做数据与桌面能力开发需 mock 或桌面调试 |

**Mock 方案(推荐,见 P0-3)**:在 `services/system.ts` 检测 `window.__TAURI_INTERNALS__` 不存在时返回模拟数据(随机 CPU、固定内存),窗口命令 no-op。零配置、不引依赖、符合「invoke 只进 services 层」铁律;vite 与 Tauri 环境自动切换,无需改调用方。

---

## P0 基础完善(让项目「干净 + 可运行」)

**目标**:清掉 DEV.md 第 3 节列的 5 个弱点中低风险项;让 web 调试和桌面调试两条路都通。

| # | 任务 | 涉及文件 | 依赖 | 验证 |
|---|---|---|---|---|
| P0-1 | 删除死代码 `PetState`(types 里从未被引用) | `src/types/index.ts` | 无 | `pnpm build`(tsc)通过 |
| P0-2 | **修当前构建失败**:`Pet.tsx` 第 1 行 `useState`/`useEffect` 未使用(tsc `noUnusedLocals` 拦截,2026-08-08 实测 `pnpm build` 报 TS6133,当前项目无法构建) | `src/components/Pet.tsx` | 无 | `pnpm build` 通过 |
| P0-3 | 类型统一:`PetStatus` 从 `Pet.tsx` 内联上提到 `types/index.ts`,与 `SystemInfo` 并列;`PetProps` 等组件接口不动 | `src/types/index.ts`、`src/components/Pet.tsx` | 无 | `pnpm build`;web 调试看 5 态动画 |
| P0-4 | **web mock 层**(可运行性关键):`services/system.ts` 顶部检测 Tauri 环境,缺失时 `getSystemInfo`/`getCpuUsage`/`getMemoryInfo` 返回模拟数据,`setWindowVisible`/`toggleAlwaysOnTop`/`closeApp` no-op;`App.tsx` 的 async handler 加 try/catch 防 unhandled rejection | `src/services/system.ts`、`src/App.tsx` | 无 | web 调试:宠物 CPU 气泡变化、面板出数据、设置弹窗按钮不报错 |
| P0-5 | 抽 `usePolling` hook:三个 hook 同构(useEffect + setInterval + catch),内部收敛为通用轮询,对外 API 不变 | `src/hooks/useSystemInfo.ts` | 无 | `pnpm build`;web 调试面板数据正常 |
| P0-6 | Rust 工具链修复 + `cargo check` 通过 + `pnpm tauri dev` 桌面首跑 | `src-tauri/`(不改代码) | rustup stable | `cargo check` 0 error;桌面窗口透明置顶、托盘出现、CPU 实时变化 |
| P0-7 | (可选,DEV.md #3 标注「尚可」)styles.css 按组件拆文件 | `src/styles.css` → `src/styles/*.css` | 无 | `pnpm build` + web 调试样式无回归 |

**P0 完成标准**:web 打开能看到「活」的宠物(数据在动);`cargo check` 通过;桌面版能启动。
**注意**:P0 不改任何业务行为,只做清理与可运行性。

---

## P1 借鉴落地(DEV.md 第 4 节性价比表,按序实施)

> DEV.md 排序即实施顺序:1 弹出覆盖窗 → 2 手势表 → 3 精灵图 → 4 状态渲染解耦 → 5 阈值告警 → 6 抚摸反馈。
> 工作量标注:S=半天内 M=1–2 天 L=3 天+。每项完成后项目保持可运行。

| # | 借鉴点 | 落地内容 | 涉及文件 | 依赖 | 工作量 | 验证 |
|---|---|---|---|---|---|---|
| P1-1 | **弹出覆盖窗**(收益最大) | 新增第二个透明置顶无边框窗口(如 160×160),宠物可从主窗「弹出」到独立窗,可拖拽、位置持久化;主窗最小化后覆盖窗仍在 | `src-tauri/tauri.conf.json`(加窗口)、`src-tauri/src/app.rs` 或拆分后 `window.rs`(新命令 `spawn_popup`/`move_popup`)、`src/components/PopupPet.tsx`(复用 Pet 渲染) | 无 | M | 桌面调试(多窗口/置顶/位置持久化);web 调试只能验证 PopupPet 组件本身 |
| P1-2 | **手势表** | ✅ **已完成(2026-08-08)**,见下方「P1-2/P1-4 落地记录」 | `src/components/Pet.tsx`、`src/App.tsx`、`src/styles.css` | 无 | S–M | ✅ `pnpm build` 0 error;web 调试:拖拽、双击挥手、右键菜单、位置刷新后保留 |
| P1-3 | **精灵图替换 CSS 五官** | ✅ **已完成(2026-08-08)**,见下方「P1-3 落地记录」 | `src/renderers/`(新建)、`src/assets/pets/qqpet-codex/`(新建)、`src/components/Pet.tsx`、`src/styles.css`、`src/vite-env.d.ts` | 素材已就位:qqpet-codex(来自 `~/.hermes/pets/qqpet-codex/`) | M | ✅ `pnpm build` 0 error;web 调试帧动画/眨眼/呼吸/greet 挥手;`cargo check` 通过(虚拟机) |
| P1-4 | **状态渲染解耦** | ✅ **已完成(2026-08-08)**,见下方「P1-2/P1-4 落地记录」 | `src/components/Pet.tsx`、新建 `src/components/CpuBubble.tsx`、`src/hooks/useSystemInfo.ts` | 无 | S | ✅ `pnpm build` 0 error;web 调试:气泡 2s 实时刷新,宠物仅状态切换才更新 |
| P1-5 | **阈值告警通知** | ✅ **已完成(2026-08-09)**,见下方「P1-5 落地记录」 | 新建 `src-tauri/src/monitor.rs`(从 app.rs 抽出)、`src-tauri/src/app.rs`、`src-tauri/src/pet_render/mod.rs`、`src-tauri/Cargo.toml`(notify-rust) | notify-rust(zbus,纯 Rust);本机通知 daemon 由 noctalia-shell 提供 | M | ✅ VM `cargo check` 0 error + release 构建;本机运行:高负载(故意 `yes > /dev/null`)→ overload 警示(跳跃动画+红边),解除后回 idle |
| P1-6 | **抚摸反馈** | 点击/抚摸宠物 → 飘爱心/撒娇动画(纯前端词表/动作触发,零模型调用) | `src/components/Pet.tsx`、`src/styles.css` | 无 | S | web 调试:点击出爱心动画 |
| P1-7 | **桌面体验优化**(插队项) | ✅ **已完成(2026-08-08)**,见下方「桌面体验优化落地记录」 | `src-tauri/src/app.rs`(WebviewWindowBuilder 显式透明重建窗口 + `move_window` 命令)、`src-tauri/tauri.conf.json`(windows 清空 / version 0.1.0)、新建 `src/services/roam.ts`、`src/services/system.ts`、`src/components/Pet.tsx`、`src/components/CpuBubble.tsx` | 无 | S | ✅ `pnpm build` 0 error;`cargo check` 通过(虚拟机);本机运行验证(透明 / 漫游 / CPU 平滑) |

**P1-3 落地记录(2026-08-08,已完成并验证)**:

- **实现**:渲染器抽象层 `src/renderers/`(types.ts 协议+接口 / sprite-renderer.ts canvas 帧动画 / index.ts 工厂分发)+ 首发宠物 `src/assets/pets/qqpet-codex/`(pet.json + spritesheet.png + index.ts);`Pet.tsx` 从 CSS 五官改造为「业务状态 → PetRenderer」桥接,点击触发 waving 挥手(greet)。
- **素材规格**:qqpet-codex 精灵图 **1536×1872**(帧 **192×208**,**8 列 × 9 行**),`stateRows` 九行映射(idle/running-right/running-left/waving/jumping/failed/waiting/running/review),`loopMs=1100`,`scale=0.4`。
- **状态行映射**(业务语义 → 动作语义):sleeping/idle → idle 行、thinking → waiting 行、working → running 行、overload → jumping 行。
- **自然动效**:呼吸(scaleY 正弦,每状态独立幅度/周期/初相)+ 眨眼(2.6~5.2s 随机,150ms 闭合,状态切换不重置计时)+ 状态切换淡入 180ms + easeInOutSine 帧节奏。
- **与计划的差异**:
  1. 新增 `framesPerState` 协议字段(计划未定义)——每状态最多播帧数的上限,配合「像素内容自动检测有效帧、空帧截断」适配各行帧数不一(如 idle 行 6 帧有效、failed 行静态);
  2. 素材目录为 `src/assets/pets/<id>/`(计划写 `src/assets/pet/`,按「一宠物一目录」命名);
  3. `framesPerRow=8`、`scale=0.4`(非 Hermes 默认 6 帧 / 0.33,按素材实测设定);
  4. 渲染器循环用 requestAnimationFrame 驱动 canvas,不触发 React re-render(延续 P0-5 思路,强于计划的 img 方案);
  5. 未做 CSS 五官 fallback(素材已到位,直接替换)。

**P1-2/P1-4 落地记录(2026-08-08,已完成并验证)**:

- **P1-2 手势表**:`Pet.tsx` 完整手势表——拖拽(pointer 事件 + `setPointerCapture`,位移 > 5px 判定,结束写入 `localStorage` key `mo.pet.position`,边界 clamp 可视区)、单击/双击分离(单击延迟 250ms 判定切面板,双击取消挂起单击并触发 greet 挥手,拖拽后 click 用 ref 抑制)、右键(阻止默认菜单,上报坐标由 App 层弹自定义菜单:设置/退出 + 屏幕边缘 clamp + 全屏遮罩点击关闭);`styles.css` 增 `.pet.positioned` / `.pet.dragging` / `.context-menu*` 样式。纯前端,零 Rust 改动。
- **P1-4 状态渲染解耦**:`usePolling` 新增 `isEqual` 参数(值相等不 setState,保留旧引用);新增 `usePetStatus`(2s 轮询,`getStatus(await getCpuUsage())` 映射离散状态,`isEqual: (a,b) => a===b`——同一状态区间内波动宠物主体零 re-render);新增 `CpuBubble` 独立组件(自持 `useCpuUsage(2000)`,数据变化只更新气泡)。宠物主体状态驱动、气泡 CPU 驱动,彻底分离。
- **与计划的差异**:
  1. **usePetStatus + isEqual 优于计划**:计划只写「降频 + 气泡独立组件」,实际额外实现「状态值相等不 setState」,宠物主体从「2s 更新一次」进一步降到「仅跨阈值切换时更新」;
  2. 双击行为改为「挥手动画 + 独立覆盖窗占位 no-op」:P1-1 覆盖窗未落地,双击暂不弹窗(TODO 标注),待 P1-1 实现后接入;
  3. 右键菜单落在 App.tsx(计划未明确落点),Pet.tsx 只上报坐标,组件解耦;
  4. **双通道记录不改**:`getCpuUsage` 与 `getSystemInfo` 读同一 Mutex 缓存不同字段,合并需动 Rust 侧,本次不改,留待 P1-5 抽 monitor.rs 时整理。

**桌面体验优化落地记录(2026-08-08,已完成并验证)**:

- **透明窗口**:config `windows` 清空,`app.rs` setup 用 `WebviewWindowBuilder` 显式链 `.transparent(true)` 重建主窗口(不依赖 config,强制 wry 透明路径)+ `.decorations(false)` + `set_background_color(Color(0,0,0,0))` 兜底;运行需 `GDK_BACKEND=wayland WEBKIT_DISABLE_DMABUF_RENDERER=1`(niri/Wayland)。
- **桌面漫游**:app.rs 新增 `move_window(dx,dy)` 增量移动命令(基于 outer_position 偏移);新建 `src/services/roam.ts`(随机目标点 + 每帧 0.8px 平滑步进 + 到达停留 5~15s + 边缘 clamp 回头 + 用户拖拽暂停);Pet.tsx 挂载启动 / 卸载停止 / 拖拽暂停恢复;web mock 用 CSS transform 模拟漫游。
- **CPU 平滑**:CpuBubble.tsx 最近 5 次滑动平均后展示,数字稳定(状态判定仍用原始值)。
- **版本对齐**:tauri.conf.json version 1.0.0 → 0.1.0(与 Cargo.toml 一致,顺手修复 Pitfall 16 遗留项)。

**方案D重构阶段1落地记录(2026-08-08,已完成并验证)**:Rust 原生宠物渲染(透明自绘,绕开 WebKitGTK alpha 硬伤)

- **背景**:2.6 桌面体验优化的结论是透明窗口卡在 WebKitGTK 内容层 alpha 合成(Wayland/X11 两路内容层均不透明)。方案D = 渲染下沉 Rust:窗口内容由 Rust 侧自绘 RGBA 像素缓冲,不经 WebKit,根除灰底。
- **实现**(新建 `src-tauri/src/pet_render/` 5 文件 + app.rs 开关):
  - `mod.rs`:`spawn_pet_window(app)` 创建 GTK 原生宠物窗口(无边框/置顶/跳过任务栏/RGBA visual + app_paintable/DrawingArea 自绘),glib timeout 16ms 动画循环,演示状态序列 `["idle","waving","thinking","working","jumping"]` 周期切换;
  - `renderer.rs`:`PetRenderer` 统一接口(与前端 types.ts 对齐)+ `RenderFrame` RGBA8 直通缓冲;
  - `sprite.rs`:`SpriteRenderer` 精灵图裁帧,对齐前端「自然动效」四要素,**纯内部时钟**(tick 喂 dt,不依赖外部时间源);
  - `manifest.rs`/`factory.rs`:pet.json 协议解析 + 工厂分发(素材 env `MO_PET_DIR` 优先,默认编译期内嵌 qqpet-codex);
  - `app.rs`:`MO_PET_MODE=rust` 环境变量开关——为 rust 时走 Rust 宠物窗口(不创建 webview),monitor + 托盘照常;默认仍走 WebKit 路径,并存不删。
- **依赖**:Cargo.toml 增 `gtk = "0.18"`(复用 tauri 依赖树的 0.18 系,零新增系统库)+ `image = "0.25"`(仅 png,复用 image-png feature 引入的 0.25 系)。
- **layer-shell 说明**:gtk-layer-shell crate 与 gtk 0.18 兼容但需要系统库 libgtk-layer-shell(VM 缺, sudo 需密码装不了)→ 阶段1 降级为普通无边框窗口 + ARGB 自绘;接入点已留(upcast 到 gtk::Window 后 for_window 提升,poe2-overlay 先例),overlay 语义后续补。
- **验证(如实,透明半达成)**:VM `pnpm tauri build --no-bundle` 出 release 二进制;本机 `env MO_PET_MODE=rust GDK_BACKEND=wayland` 运行,**企鹅渲染成功(睡觉帧:盖被子+Zzz)、无边框/无标题栏、置顶悬浮、动画在跑(两帧 33% 像素差异)**;**窗口层透明已达成**(窗口四角 niri 圆角外透出下层桌面/文档内容 (243,246,252))。**但内容层仍有 (80,80,80) 灰底**:draw 回调只把企鹅像素 blit 上去,GTK 主题背景先填充了 DrawingArea——换 `GTK_THEME=Adwaita:light` 后灰底变薄荷绿 (78,201,176),实锤主题背景填充。**根因与 WebKit 路径不同(不是 WebKitGTK alpha,是 GTK widget 主题背景),修法明确:draw 回调开头 `cr.set_operator(cairo::Operator::Source)` + `set_source_rgba(0,0,0,0)` + `paint()` 清透明(或 CSS background: transparent),下一阶段一行修复**。截图:`/tmp/mo-rust-final-1.png` / `/tmp/mo-rust-final-2.png` / `/tmp/mo-rust-context.png` / `/tmp/mo-rust-theme-light.png`。
- **与计划的差异**:
  1. 计划方案阶梯中方案D标「治本,大改」,本阶段按「阶段1 = 宠物窗口 + 演示动画」实施,面板(React)未挂到 Rust 窗口,留待阶段2;
  2. gtk-layer-shell 因 VM 系统库缺失降级,overlay 层语义后补(计划中方案C 的 layer-shell 提升未做);
  3. 素材内嵌(include_bytes)优于计划假设的运行时路径依赖——发布后素材随二进制走。
- **遗留(下一阶段)**:面板/设置 UI 挂到 Rust 宠物窗口(点击事件 → set_state);layer-shell 提升 + 穿透点击;MO_PET_MODE=rust 下漫游/拖拽。

**方案D重构阶段2落地记录(2026-08-08,完成)**:清除 DrawingArea 主题背景,内容层透明完全达成
- **修复方式**:`src-tauri/src/pet_render/mod.rs` draw 回调开头加 `cr.set_operator(cairo::Operator::Source)` + `set_source_rgba(0,0,0,0)` + `paint()` 清透明(直接覆盖目标含 alpha,抹掉 GTK 主题背景),再恢复 `Over` 混合正常 blit 企鹅像素(企鹅边缘半透明像素须保持混合)。
- **关键澄清**:阶段1 实测的「(80,80,80) 灰底」「换主题变薄荷绿」实为**下层 Hermes 窗口的薄荷绿宠物头像内容**误判——素材 spritesheet 全图扫描 0 命中该色;移走下层的 Hermes 窗口后,Mo 窗口区域 (77x83) 全部变为下层壁纸色 (243,246,252),窗内窗外完全一致 → **内容层透明 100% 达成,无灰底、无主题背景色残留**。
- **验证受阻说明**:像素级三要素验证(企鹅/透明/动画)遇屏幕空闲自动锁屏(noctalia-shell ext-session-lock),普通窗口不可见;非像素证据:进程稳定无崩溃、niri 浮层窗口创建正常、CPU ~3.9% 持续(glib 16ms 动画循环活跃=动画在跑)。解锁后补验命令见 DEV.md。
- **遗留(阶段3)**:面板/设置 UI 挂到 Rust 宠物窗口;layer-shell 提升 + 穿透点击;MO_PET_MODE=rust 下漫游/拖拽。

**P1-5 落地记录(2026-08-09,已完成并验证)**:阈值告警 + monitor.rs 抽取 + 真实状态驱动

- **monitor.rs 抽取**(app.rs 拆分建议第一块落地):新建 `src-tauri/src/monitor.rs`——`SystemInfo` 结构/共享缓存 `AppState`/轮询线程/系统信息命令(`get_system_info`/`get_cpu_usage`/`get_memory_info`)整体迁入;`start_monitor(app) -> mpsc::Receiver<MonitorEvent>` 返回事件接收端。app.rs 瘦身:只留窗口控制命令 + 托盘 + 入口注册,命令以 `monitor::xxx` 注册;lib.rs 注册 `pub mod monitor;`。
- **阈值告警 + 滞回**:CPU>85%(`MO_CPU_OVERLOAD_THR` 可配)或内存>90%(`MO_MEM_OVERLOAD_THR`)→ 过载;持续超阈值 3s(`MO_ENTER_OVERLOAD_MS`)确认进入、持续低于 5s(`MO_EXIT_OVERLOAD_MS`)确认退出(期间任一采样不满足即重置计时,防微波动抖动);进入/退出各发一次 `OverloadStarted`/`OverloadEnded`,每秒发 `Sample`(含瞬时负载档位 Low/Mid/Overload)。
- **真实状态驱动**(pet_render/mod.rs,替换演示随机模式):状态驱动 timeout 每秒消费 monitor 事件——过载 → overload 警示(jumping 行快速循环 + 急促呼吸,overload 状态 sprite.rs 早已就绪只缺驱动);低负载保持 idle 为主自然节奏(权重池 `[thinking:3,working:1,waving:1,jumping:1]`),中负载 working 权重提高(`[thinking:3,working:3,...]`);`MO_DEMO=1` 回退旧演示状态机(调试 fallback,不删)。
- **警示动画**:overload 时 draw 回调叠加红色脉冲边框(AtomicBool 标志共享,纯 cairo stroke,渲染器核心 sprite.rs 零改动)。
- **系统通知**:notify-rust(zbus 纯 Rust,无系统库)进入过载时发桌面通知(本机 niri 由 noctalia-shell 提供 `org.freedesktop.Notifications`);`MO_NOTIFY=0` 关闭;通知失败(无 daemon)静默忽略,宠物动画是主通道。
- **与计划的差异**:① 通知用 notify-rust 而非 tauri-plugin-notification——纯 Rust 零系统库,monitor 线程直接调用,不依赖前端 JS API;② 未做「点击通知回到应用」(notify-rust 无回调能力,tauri-plugin 才有;桌面宠物常驻低价值,留注释后续);③ 前端 WebKit 路径未改(usePetStatus 已按 CPU 映射状态,两条路径独立)。
- **验证**:VM `cargo check` 0 error;`pnpm tauri build --no-bundle` 出 release 二进制;本机运行 + 故意 `yes > /dev/null` 压 CPU → 约 3~8s 后宠物切 overload(跳跃动画 + 红边)+ 桌面通知,杀 yes 后约 5s 回 idle;截图 `/tmp/mo-p15-*.png`。

**P1 完成标准**:宠物可弹出独立窗、可拖拽、状态渲染流畅、超阈值会告警、点击有反馈。
**穿插建议**:P1-2 与 P1-1 的拖拽逻辑可合并实现;P1-3 依赖素材,素材不到位时先做 P1-4/P1-6。
**app.rs 拆分**(DEV.md 弱点 #1):**已部分落地(P1-5,2026-08-09)——monitor.rs 已抽出**;tray.rs/window.rs 按需再拆。

---

## P2 AI 对话 + 记忆系统(DEV.md 第 5 节,严格执行 5.8 分步表)

> DEV.md 5.8 已有完整步骤表(改动文件 + 验证),本规划只补充依赖、前置条件与验证衔接,不重复其表格。

### P2-1 能对话(DEV.md 5.8 P1,8 步)

- **前置依赖(必须)**:
  - LLM 服务可用:DeepSeek / 智谱 / OpenRouter / Ollama 任一,OpenAI 兼容协议;
  - 环境变量 `PET_LLM_BASE_URL` / `PET_LLM_API_KEY` / `PET_LLM_MODEL`(key 不进前端);
  - Rust 依赖:reqwest(json/stream)+ tokio。
- **涉及文件**:`src-tauri/Cargo.toml`、新建 `src-tauri/src/chat/llm.rs`、`src-tauri/src/commands/chat.rs`、`src-tauri/src/app.rs`(注册)、`src/services/system.ts`、新建 `src/components/PetChat.tsx`、`src/App.tsx`、`src/styles.css`。
- **验证**:`cargo check` → `pnpm build` → `pnpm tauri dev` 点宠物弹聊天、输入收到回复;web 调试需给 `chatMessage` 加 mock(返回固定回复)才能看 UI。
- **完成标准**:DEV.md「点宠物能弹出聊天,输入后收到 LLM 回复」。

### P2-2 有记忆(DEV.md 5.8 P2,8 步)

- **前置依赖**:rusqlite(bundled 特性,免系统 sqlite);SQLite 表按 DEV.md 5.3 四张。
- **涉及文件**:新建 `src-tauri/src/store/{db,sessions,memories}.rs`、`src-tauri/src/chat/context.rs`、`src-tauri/src/commands/{chat,memory}.rs`、`src/components/PetChat.tsx`。
- **验证**:`cargo check` + 桌面调试:重启后会话历史仍在、手写记忆影响回答。web 调试仅 UI 层(历史列表展示需 mock 数据)。
- **完成标准**:DEV.md「重启应用后能接着上次对话;手动写入的记忆会影响宠物回答」。

### P2-3 会沉淀(DEV.md 5.8 P3,4 步)

- **前置依赖**:P2-1/P2-2 完成;LLM 提取 prompt 可用(DEV.md 5.7 模板)。
- **涉及文件**:新建 `src-tauri/src/chat/extract.rs`、`store/memories.rs`(衰减)、`chat/context.rs`(加权排序)、前端记忆管理视图。
- **验证**:桌面调试多轮对话后记忆库增长、重复对话不刷屏、久远记忆降权;前端可查看/删除记忆。
- **完成标准**:DEV.md「宠物能主动记住用户偏好并在后续对话中体现;记忆可被查看与删除」。

### P2 后续优化(可插队,DEV.md 5.8「后续优化」)

- 流式输出:chat_message 改 Tauri 2 channel 逐块推送,前端增量拼接(替代 P2-1 整句返回);
- 对话情感反馈:回答后对特定词句触发爱心/撒娇动画(联动 P1-6);
- 阈值告警联动:超阈值时宠物主动提醒、可一键追问(联动 P1-5)。

---

## 3. 验证方式总表

| 方式 | 命令 | 能验证什么 | 不能验证什么 |
|---|---|---|---|
| Web 调试 | `pnpm dev` + 浏览器 http://localhost:1420 | UI 布局、5 态动画、交互手势、i18n、组件渲染、mock 数据流 | 真实系统数据、窗口/托盘/置顶、通知、LLM 真实调用 |
| 桌面调试 | `pnpm tauri dev` | 全部:透明窗口、多窗口弹出、托盘、真实 CPU/内存、通知、LLM 对话与记忆 | —(需 Rust 工具链 + 桌面环境) |
| 静态检查 | `cargo check` / `pnpm build` | 编译与类型正确性 | 运行时行为 |

**建议节奏**:纯前端改动(UI/动画/手势)→ web 调试为主,收尾 `pnpm tauri dev` 桌面确认;Rust 改动(命令/线程/存储)→ `cargo check` + 桌面调试;P0-3 的 mock 层让大部分 P1 项可在 web 侧完成开发。

---

## 附:审查发现的其他小问题(随手记,不进本规划主线)

- `App.tsx` 三个 async handler(handleMinimize/handleToggleTop/handleExit)无 try/catch,web 下会产生 unhandled rejection(P0-3 一并修)。
- `getCpuUsage` 与 `getSystemInfo` 数据双通道:monitor 线程同一份缓存写两处(`cpu_usage` + `system_info.cpu_usage`),前端又分别轮询,可后续合并成单一 `get_system_info` 调用(P1-4 顺带评估)。
- `SystemInfoPanel` 内联 `formatBytes` 可移到 utils(P0-6 可选)。
- `log`/`env_logger` 依赖已在 Cargo.toml 但 app.rs 未使用日志宏(死依赖候选,`cargo check` 会提示,可在 P0-5 时确认是否移除)。
