import { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  toggleAlwaysOnTop,
  closeApp,
  minimizeToTray,
} from "./services/system";
import Pet from "./components/Pet";
import SystemInfoPanel from "./components/SystemInfoPanel";
import SettingsModal from "./components/SettingsModal";
import "./i18n";

/** 宠物右键菜单在屏幕上的弹出位置 */
interface MenuPosition {
  x: number;
  y: number;
}

/** 右键菜单尺寸(用于屏幕边缘 clamp,避免菜单弹出视口) */
const MENU_WIDTH = 150;
const MENU_HEIGHT = 96;

export default function App() {
  const { t, i18n } = useTranslation();
  const [showPanel, setShowPanel] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [menu, setMenu] = useState<MenuPosition | null>(null);

  /** 单击宠物 → 切换信息面板 */
  const handlePetClick = useCallback(() => {
    setShowPanel((prev) => !prev);
  }, []);

  /** 双击宠物 → 挥手动画由 Pet 内部渲染器触发,此处为独立窗口占位(P1-1 未落地) */
  const handlePetDoubleClick = useCallback(() => {
    // TODO(P1-1):弹出独立覆盖窗;当前双击仅触发挥手动画
  }, []);

  /** 右键宠物 → 在鼠标位置弹出自定义菜单(设置/退出) */
  const handlePetContextMenu = useCallback((x: number, y: number) => {
    // 屏幕边缘 clamp,保证菜单完整可见
    const left = Math.min(x, window.innerWidth - MENU_WIDTH);
    const top = Math.min(y, window.innerHeight - MENU_HEIGHT);
    setMenu({ x: Math.max(0, left), y: Math.max(0, top) });
  }, []);

  const closeMenu = useCallback(() => setMenu(null), []);

  /** 菜单「设置」:打开设置弹窗 */
  const handleMenuSettings = useCallback(() => {
    setShowSettings(true);
    setMenu(null);
  }, []);

  /** 最小化到系统托盘(web 调试下为 no-op,失败仅记日志不抛出) */
  const handleMinimize = useCallback(async () => {
    try {
      await minimizeToTray();
    } catch (e) {
      console.error("最小化到托盘失败:", e);
    }
  }, []);

  /** 切换窗口置顶(web 调试下为 no-op) */
  const handleToggleTop = useCallback(async () => {
    try {
      await toggleAlwaysOnTop();
    } catch (e) {
      console.error("切换置顶失败:", e);
    }
  }, []);

  /** 退出应用(web 调试下为 no-op) */
  const handleExit = useCallback(async () => {
    try {
      await closeApp();
    } catch (e) {
      console.error("退出应用失败:", e);
    }
  }, []);

  /** 菜单「退出」:关闭菜单后退出应用(web 调试下为 no-op) */
  const handleMenuExit = useCallback(() => {
    setMenu(null);
    void handleExit();
  }, [handleExit]);

  const handleChangeLanguage = useCallback(
    (lang: string) => {
      i18n.changeLanguage(lang);
      localStorage.setItem("language", lang);
    },
    [i18n],
  );

  return (
    <div className="app-container">
      <Pet
        onClick={handlePetClick}
        onDoubleClick={handlePetDoubleClick}
        onContextMenu={handlePetContextMenu}
      />
      {showPanel && <SystemInfoPanel onClose={() => setShowPanel(false)} />}
      {showSettings && (
        <SettingsModal
          onClose={() => setShowSettings(false)}
          onChangeLanguage={handleChangeLanguage}
          currentLanguage={i18n.language}
          onMinimize={handleMinimize}
          onToggleTop={handleToggleTop}
          onExit={handleExit}
        />
      )}
      {/* P1-2 右键菜单:全屏透明遮罩点击关闭 + 宠物菜单(设置/退出) */}
      {menu && (
        <>
          <div
            className="context-menu-overlay"
            onClick={closeMenu}
            onContextMenu={(e) => {
              e.preventDefault();
              closeMenu();
            }}
          />
          <div
            className="context-menu"
            style={{ left: menu.x, top: menu.y }}
            role="menu"
          >
            <button
              className="context-menu-item"
              onClick={handleMenuSettings}
              role="menuitem"
            >
              ⚙ {t("app.settings")}
            </button>
            <button
              className="context-menu-item danger"
              onClick={handleMenuExit}
              role="menuitem"
            >
              ⏻ {t("app.exit")}
            </button>
          </div>
        </>
      )}
      <button
        className="settings-btn"
        onClick={() => setShowSettings(true)}
        title={t("app.settings")}
      >
        ⚙
      </button>
    </div>
  );
}
