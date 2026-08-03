---
name: mo-dev
description: Mo 桌面宠物项目(Tauri 2 + React 18 + Rust)的自动化开发技能。当任务涉及修改 Mo 项目的功能、新增 Tauri 命令、编写 React 组件、改动宠物状态机/动画、开发 AI 对话或记忆系统、重构或修复本项目时使用。执行开发前先读取本项目 DEV.md 获取完整架构与规划。
---

# Mo 项目开发技能

Mo 是「桌面宠物 + 系统监控」应用:宠物随 CPU 负载变换表情动画,支持托盘、置顶、多语言。Tauri 2 管理 Rust 后端与透明置顶窗口,React 18 渲染前端。

## 1. 结构地图

```
src/
├── App.tsx              主应用:协调宠物/面板/设置三个视图开关
├── components/          Pet(状态机→CSS动画) · SystemInfoPanel · SettingsModal
├── hooks/useSystemInfo.ts   轮询数据(useCpuUsage 等)
├── services/system.ts   唯一 invoke 封装层
├── i18n/locales/        zh.json · en.json
├── types/index.ts
└── styles.css           全部样式(CSS 宠物 + keyframes)

src-tauri/src/
└── app.rs               命令 + CPU轮询线程 + 托盘 + 窗口控制 + 入口(待拆分)
```

## 2. 核心模式与铁律

| 铁律 | 说明 |
|---|---|
| 前端禁止直接 `invoke` | 一律经 `services/system.ts` 封装后调用 |
| 命令只读缓存 | 后台线程写 `AppState`(Mutex),命令读缓存,不做同步 IO |
| 文案走 i18n | 中英双份加入 `locales/*.json`,不硬编码 |
| 应用持久化放 Rust 端 | localStorage 仅限语言等轻配置 |
| 状态机与 CSS 同步 | 改 `PetStatus` 必须同步 `styles.css` 对应动画 class |
| 透明窗口 | 保持 `background: transparent`,避免铺满深色背景 |

## 3. 新增 Tauri 命令(标准流程)

1. Rust:`app.rs` 定义 `#[tauri::command]`,参数用 `tauri::State<'_, AppState>` 读缓存
2. 注册进 `invoke_handler!`(app.rs 底部)
3. 前端:`services/system.ts` 加同名封装函数(带 TS 返回类型)
4. 组件/hooks 调用。参考现有 `get_cpu_usage` 全链路。

## 4. 新增 UI 组件

- 放 `src/components/`,纯展示组件,状态由 App 协调
- 新轮询逻辑参照 `useCpuUsage`(useEffect + setInterval + catch 静默)
- 样式加入 `styles.css` 对应分区注释块下

## 5. 宠物状态机改动

`Pet.tsx` 中 `PetStatus = sleeping | idle | thinking | working | overload`,`getStatus(cpu)` 做阈值映射。改动流程:
1. 加/改状态枚举与阈值
2. `styles.css` 加 `.pet-<status>` 选择器与 keyframes
3. 眼睛/嘴巴按状态在 Pet.tsx 条件渲染

## 6. AI 对话 + 记忆系统开发

完整规划见 **DEV.md 第 5 节**(架构、三层记忆、表结构、模块划分、命令接口、Prompt 模板、路线图)。**实施时严格按 DEV.md 5.8 的分步表格逐条执行**——每步标注了改动文件与验证方式,禁止跳步或合并步骤。要点:

- **P1 能对话 → P2 有记忆 → P3 会沉淀**,按此顺序实施
- 命令:`chat_message(session_id, text)` 流式返回,用 Tauri 2 channel/Event
- 存储:rusqlite(SQLite),表 `sessions / messages / memories / memory_sources`
- LLM:OpenAI 兼容协议,环境变量 `PET_LLM_BASE_URL / PET_LLM_API_KEY / PET_LLM_MODEL`,key 不进前端
- 记忆注入:system prompt 拼记忆 + 历史;对话后异步调提取 prompt(输出 JSON 数组)
- 实施时按 DEV.md 5.4 拆分 `commands/ · chat/ · store/`,勿再堆进 app.rs

## 7. 重构注意

`app.rs` 目前职责过载,拆分方向:monitor.rs(轮询)/ tray.rs / window.rs / commands/。若做拆分,保证 `invoke_handler!` 注册、`AppState` 管理、`setup` 初始化三处不丢。`types/index.ts` 的 `PetState` 是死代码,可删。

## 8. 构建与验证

```bash
pnpm tauri dev      # 开发
pnpm build          # 前端构建(tsc + vite)
cd src-tauri && cargo check    # Rust 检查
pnpm tauri build    # 生产构建
```

改动后至少 `cargo check` + `pnpm build` 通过再汇报。若涉及宠物渲染/窗口行为,说明无法在此环境自动验证 UI,交由用户运行 `pnpm tauri dev` 确认。
