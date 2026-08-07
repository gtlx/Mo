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
| P1-5 | **阈值告警通知** | Rust 监测线程加阈值判断(CPU/内存超线)→ 系统通知 + 宠物警示动画;点击通知回到应用 | `src-tauri/Cargo.toml`、新建 `src-tauri/src/monitor.rs`(从 app.rs 抽出)、`src/App.tsx`、`src/styles.css` | `tauri-plugin-notification` crate;平台通知权限(Linux 需通知 daemon) | M | 桌面调试(真实通知);警示动画 web 可验 |
| P1-6 | **抚摸反馈** | 点击/抚摸宠物 → 飘爱心/撒娇动画(纯前端词表/动作触发,零模型调用) | `src/components/Pet.tsx`、`src/styles.css` | 无 | S | web 调试:点击出爱心动画 |

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

**P1 完成标准**:宠物可弹出独立窗、可拖拽、状态渲染流畅、超阈值会告警、点击有反馈。
**穿插建议**:P1-2 与 P1-1 的拖拽逻辑可合并实现;P1-3 依赖素材,素材不到位时先做 P1-4/P1-6。
**app.rs 拆分**(DEV.md 弱点 #1)建议在 P1-5 做 monitor.rs 抽取时一并完成(monitor/tray/window/commands 四件套),避免单独一次大重构。

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
