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

export default function App() {
  const { t, i18n } = useTranslation();
  const [showPanel, setShowPanel] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const handlePetClick = useCallback(() => {
    setShowPanel((prev) => !prev);
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

  const handleChangeLanguage = useCallback(
    (lang: string) => {
      i18n.changeLanguage(lang);
      localStorage.setItem("language", lang);
    },
    [i18n],
  );

  return (
    <div className="app-container">
      <Pet onClick={handlePetClick} />
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
