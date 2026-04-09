import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useCpuUsage } from "../hooks/useSystemInfo";

interface PetProps {
  onClick: () => void;
}

export default function Pet({ onClick }: PetProps) {
  const { t } = useTranslation();
  const cpuUsage = useCpuUsage(1000);
  const [status, setStatus] = useState<"idle" | "working" | "thinking">("idle");

  useEffect(() => {
    if (cpuUsage > 50) {
      setStatus("working");
    } else if (cpuUsage > 20) {
      setStatus("thinking");
    } else {
      setStatus("idle");
    }
  }, [cpuUsage]);

  const handleClick = useCallback(() => {
    onClick();
  }, [onClick]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    onClick();
  }, [onClick]);

  return (
    <div
      className={`pet pet-${status}`}
      onClick={handleClick}
      onContextMenu={handleContextMenu}
      title={t(`pet.${status}`)}
    >
      <div className="pet-body">
        <div className="pet-eyes">
          <div className="eye left"></div>
          <div className="eye right"></div>
        </div>
        <div className="pet-mouth"></div>
      </div>
      <div className="pet-bubble">
        {cpuUsage.toFixed(0)}%
      </div>
    </div>
  );
}
