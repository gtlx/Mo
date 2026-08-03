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
│   ├── Pet.tsx               宠物(5 态状态机 → CSS 动画)
│   ├── SystemInfoPanel.tsx   系统信息面板(进度条)
│   └── SettingsModal.tsx     设置弹窗(语言/置顶/最小化/退出)
├── hooks/useSystemInfo.ts    数据轮询 hooks(useCpuUsage 等)
├── services/system.ts        Tauri 命令封装层(唯一调 invoke 的地方)
├── i18n/                     国际化(locales/zh.json · en.json)
├── types/index.ts            共享类型
└── styles.css                全部样式(CSS 宠物五官 + keyframes)

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

关键机制:Rust 后台线程每 1s 轮询 CPU/内存写入 `AppState` 的 `Mutex`;前端每 1–2s 通过命令读缓存,前端零阻塞。

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
| 2 | **完整手势表** | 拖拽移动 / 单击 / 双击 / shift-click 各司其职 | 目前单击与右键同一动作;至少补拖拽 + 位置 localStorage 持久化 |
| 3 | **精灵图替换 CSS 五官** | 状态 → sprite 行 → 帧动画,换宠物只换图集 | 抽 `PetStatus → spriteRow → 帧`,DOM 放 img/canvas 而非 CSS 五官 |
| 4 | **状态与渲染解耦** | 漫游用命令式 el.style,settle 才 commit React | CPU 轮询降频;数字气泡独立组件,避免整只宠物每帧 re-render |
| 5 | **阈值告警通知** | 任务完成 → 浮出 mail 图标,点击回到线程 | CPU/内存超阈值 → 警示动画 + 系统通知,让监控"有用" |
| 6 | **点击抚摸反馈** | 词表匹配"good bot"飘爱心(零模型调用) | 点击/抚摸宠物 → 撒娇动画 |

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

### 约定

- 宠物状态机改 `PetStatus` 时,同步更新 `styles.css` 对应动画 class。
- 透明窗口下避免深色背景铺满,保持 `background: transparent`。
- 内存型后台任务(监测线程)用 `AppState + Mutex` 缓存,命令只读缓存,不做同步 IO。
