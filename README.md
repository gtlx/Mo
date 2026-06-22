# Desktop Pet 🐾

跨平台桌面宠物 + 系统监控小工具，基于 **Tauri 2 + React 18 + Rust**。

宠物会随 CPU 负载实时变换表情和动画，点击可查看系统信息。

---

## 特性

| 状态 | CPU 范围 | 表情 | 动画 |
|---|---|---|---|
| 💤 睡大觉 | < 5% | 闭眼 — — | 缓慢呼吸 |
| 😊 空闲中 | 5–20% | 正常 O O | 上下弹跳 |
| 🤔 思考中 | 20–50% | 眯眼 > < | 轻轻浮动 |
| 💪 工作中 | 50–80% | 专注    | 左右抖动 |
| 🔥 要炸了 | > 80% | 警告 !! | 剧烈抖动 + 红温 |

- **系统信息面板**：实时 CPU / 内存使用率（进度条 + 数值）
- **CPU 气泡**：宠物头顶实时显示 CPU%
- **系统托盘**：右键托盘图标 → 显示/隐藏窗口、退出（隐藏后可通过托盘恢复）
- **多语言**：中文 / English
- **透明窗口**：无边框、置顶、跳过任务栏
- **高性能**：Rust 后台线程轮询系统信息，前端零阻塞

---

## 截图

```
  ┌──────┐
  │  💤  │   ← 宠物主体（CPU < 5% 时睡觉状态）
  │ — —  │
  │  ～  │
  └──────┘
    23%      ← CPU 气泡
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
│   │   ├── Pet.tsx           # 桌面宠物（5 种状态）
│   │   ├── SystemInfoPanel.tsx
│   │   └── SettingsModal.tsx
│   ├── hooks/useSystemInfo.ts
│   ├── services/system.ts    # Tauri 命令封装
│   ├── i18n/                 # 国际化（zh / en）
│   └── styles.css            # 全部样式
│
├── src-tauri/                # 后端（Rust）
│   ├── src/
│   │   ├── app.rs            # 命令 + 后台监测 + 系统托盘
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
