import { useTranslation } from "react-i18next";

interface Props {
  onClose: () => void;
  onChangeLanguage: (lang: string) => void;
  currentLanguage: string;
  onHide: () => void;
  onToggleTop: () => void;
  onExit: () => void;
}

export default function SettingsModal({
  onClose,
  onChangeLanguage,
  currentLanguage,
  onHide,
  onToggleTop,
  onExit,
}: Props) {
  const { t } = useTranslation();

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <span>{t("app.settings")}</span>
          <button className="close-btn" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="modal-content">
          <div className="setting-item">
            <span className="setting-label">{t("app.language")}</span>
            <select
              value={currentLanguage}
              onChange={(e) => onChangeLanguage(e.target.value)}
            >
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
          <div className="setting-item">
            <button className="action-btn" onClick={onHide}>
              {t("app.hide")}
            </button>
            <button className="action-btn" onClick={onToggleTop}>
              {t("app.alwaysOnTop")}
            </button>
            <button className="action-btn danger" onClick={onExit}>
              {t("app.exit")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
