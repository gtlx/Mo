import { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { setWindowVisible, toggleAlwaysOnTop, closeApp } from "./services/system";
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

  const handleHide = useCallback(async () => {
    await setWindowVisible(false);
  }, []);

  const handleToggleTop = useCallback(async () => {
    await toggleAlwaysOnTop();
  }, []);

  const handleExit = useCallback(async () => {
    await closeApp();
  }, []);

  const handleChangeLanguage = useCallback(
    (lang: string) => {
      i18n.changeLanguage(lang);
      localStorage.setItem("language", lang);
    },
    [i18n]
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
          onHide={handleHide}
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
