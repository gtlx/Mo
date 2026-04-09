import { useTranslation } from "react-i18next";
import { useSystemInfo } from "../hooks/useSystemInfo";

interface Props {
  onClose: () => void;
}

function formatBytes(bytes: number): string {
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) {
    return `${gb.toFixed(1)} GB`;
  }
  const mb = bytes / (1024 * 1024);
  return `${mb.toFixed(0)} MB`;
}

export default function SystemInfoPanel({ onClose }: Props) {
  const { t } = useTranslation();
  const { info, loading, error } = useSystemInfo(2000);

  return (
    <div className="panel" onClick={(e) => e.stopPropagation()}>
      <div className="panel-header">
        <span>{t("app.title")}</span>
        <button className="close-btn" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="panel-content">
        {loading && <div className="loading">Loading...</div>}
        {error && <div className="error">{error}</div>}
        {info && (
          <>
            <div className="info-item">
              <span className="label">{t("system.cpu")}</span>
              <div className="progress-bar">
                <div
                  className="progress-fill cpu"
                  style={{ width: `${info.cpu_usage}%` }}
                ></div>
              </div>
              <span className="value">{info.cpu_usage.toFixed(1)}%</span>
            </div>
            <div className="info-item">
              <span className="label">{t("system.memory")}</span>
              <div className="progress-bar">
                <div
                  className="progress-fill memory"
                  style={{ width: `${info.memory_percent}%` }}
                ></div>
              </div>
              <span className="value">
                {formatBytes(info.memory_used)} / {formatBytes(info.memory_total)}
              </span>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
