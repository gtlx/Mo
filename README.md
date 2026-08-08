# Desktop Pet 🐾

跨平台桌面宠物 + 系统监控小工具，基于 **Tauri 2 + React 18 + Rust**。

宠物会随 CPU 负载实时切换精灵图动画状态，点击可查看系统信息。首发宠物为 QQ 企鹅 Codex 定制（qqpet-codex），素材可更换。

---

## 特性

| 状态 | CPU 范围 | 动画表现(精灵图状态行) |
|---|---|---|
| 💤 睡大觉 | < 5% | idle 行慢速循环 + 大幅呼吸(闭眼) |
| 😊 空闲中 | 5–20% | idle 行标准待机 + 眨眼 |
| 🤔 思考中 | 20–50% | waiting 行小幅等待动作 |
| 💪 工作中 | 50–80% | running 行忙碌跑动 |
| 🔥 要炸了 | > 80% | jumping 行跳动警示(节奏加快) |

- **精灵图渲染**：canvas 帧动画（192×208 帧、9 状态行），呼吸 / 眨眼 / 平滑过渡等自然动效
- **素材可更换**：宠物 = 一个目录（pet.json 协议 + spritesheet.png），换宠物不改代码
- **手势互动**：拖拽移动宠物（位置自动保存，刷新不丢）/ 单击切换面板 / 双击挥手 / 右键菜单（设置、退出）
- **渲染解耦**：CPU 气泡独立 2s 轮询实时刷新；宠物主体仅状态切换才更新，动画流畅不卡顿
- **系统信息面板**：实时 CPU / 内存使用率（进度条 + 数值）
- **CPU 气泡**：宠物头顶实时显示 CPU%（独立轮询，不影响宠物动画）
- **系统托盘**：右键托盘图标 → 显示/隐藏窗口、退出（隐藏后可通过托盘恢复）
- **多语言**：中文 / English
- **透明悬浮窗**：无边框、置顶、跳过任务栏；Rust 侧强制 wry 透明路径（niri/Wayland 需 `GDK_BACKEND=wayland` 启动）
- **Rust 原生渲染（方案D）**：`MO_PET_MODE=rust` 启动时走 GTK 自绘宠物窗口（RGBA 像素缓冲直出，不依赖 WebKit/WebView），无边框置顶悬浮、动画流畅；与 WebKit 路径并存，默认模式不变（阶段1：窗口层透明已达成，内容层待清主题背景，见 DEV.md 2.7）
- **桌面漫游**：宠物在桌面范围内自动走动（随机选点 / 平滑移动 / 到达停留 / 边缘回头），用户拖拽时自动暂停
- **CPU 平滑**：气泡数值最近 5 次采样滑动平均，显示稳定不跳动
- **高性能**：Rust 后台线程轮询系统信息，前端零阻塞；渲染循环不触发 React re-render

---

## 截图

```
  ┌──────────┐
  │  🐧 精灵图 │   ← 宠物主体（canvas 帧动画，CPU < 5% 时 idle 行慢速循环）
  │ (192×208) │
  └──────────┘
    23%          ← CPU 气泡
```

---

## 快速开始

```bash
# 安装依赖
pnpm install

# 启动开发模式
pnpm tauri dev

# 构建生产版本
pnpm tauri build
```

### 系统要求

- Node.js >= 18
- pnpm >= 8
- Rust 稳定版
- Linux: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev`

---

## 项目结构

```
Mo/
├── src/                      # 前端（React + TypeScript）
│   ├── App.tsx               # 主应用
│   ├── main.tsx              # 入口
│   ├── components/
│   │   ├── Pet.tsx           # 桌面宠物（渲染器驱动 + 完整手势表）
│   │   ├── CpuBubble.tsx     # CPU 数字气泡（独立 2s 轮询，渲染解耦）
│   │   ├── SystemInfoPanel.tsx
│   │   └── SettingsModal.tsx
│   ├── renderers/            # 渲染器抽象层（为 Live2D/Spine 预留）
│   │   ├── types.ts          # PetManifest 协议 + PetRenderer 接口
│   │   ├── sprite-renderer.ts # 精灵图渲染器（canvas 帧动画 + 自然动效）
│   │   └── index.ts          # createRenderer 工厂
│   ├── assets/pets/          # 宠物素材库（每宠物一目录）
│   │   └── qqpet-codex/      # 首发宠物（pet.json + spritesheet.png）
│   ├── hooks/useSystemInfo.ts
│   ├── services/system.ts    # Tauri 命令封装
│   ├── i18n/                 # 国际化（zh / en）
│   └── styles.css            # 样式（布局 / 面板）
│
├── src-tauri/                # 后端（Rust）
│   ├── src/
│   │   ├── app.rs            # 命令 + 后台监测 + 系统托盘 + MO_PET_MODE 开关
│   │   ├── pet_render/       # 方案D：Rust 原生宠物渲染（GTK 自绘透明窗口）
│   │   │   ├── mod.rs        # 窗口创建 + 动画循环（spawn_pet_window）
│   │   │   ├── renderer.rs   # PetRenderer 接口 + RGBA 帧缓冲
│   │   │   ├── sprite.rs     # 精灵图渲染器（裁帧 + 呼吸/眨眼/淡入）
│   │   │   ├── manifest.rs   # pet.json 协议解析
│   │   │   └── factory.rs    # 渲染器工厂（素材内嵌/可更换）
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── package.json
├── vite.config.ts
└── tsconfig.json
```

---

## 技术栈

- **Tauri 2** — 跨平台桌面框架
- **React 18** — 前端 UI
- **Rust** — 系统信息采集 + 系统托盘
- **sysinfo** — 跨平台系统信息库
- **i18next** — 国际化
- **Vite** — 构建工具

---

## License

MIT
