import { useState, useEffect, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useCpuUsage } from "../hooks/useSystemInfo";

interface PetProps {
  onClick: () => void;
}

type PetStatus = "sleeping" | "idle" | "thinking" | "working" | "overload";

function getStatus(cpu: number): PetStatus {
  if (cpu > 80) return "overload";
  if (cpu > 50) return "working";
  if (cpu > 20) return "thinking";
  if (cpu > 5) return "idle";
  return "sleeping";
}

export default function Pet({ onClick }: PetProps) {
  const { t } = useTranslation();
  const cpuUsage = useCpuUsage(1000);
  const status = useMemo(() => getStatus(cpuUsage), [cpuUsage]);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      onClick();
    },
    [onClick],
  );

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      onClick();
    },
    [onClick],
  );

  return (
    <div
      className={`pet pet-${status}`}
      onClick={handleClick}
      onContextMenu={handleContextMenu}
      title={t(`pet.${status}`)}
    >
      <div className="pet-body">
        <div className="pet-eyes">
          {status === "sleeping" ? (
            <>
              <div className="eye closed">—</div>
              <div className="eye closed">—</div>
            </>
          ) : status === "overload" ? (
            <>
              <div className="eye overload">!</div>
              <div className="eye overload">!</div>
            </>
          ) : (
            <>
              <div className={`eye ${status === "thinking" ? "squint" : ""}`} />
              <div className={`eye ${status === "thinking" ? "squint" : ""}`} />
            </>
          )}
        </div>
        <div className={`pet-mouth mouth-${status}`} />
        {status === "sleeping" && (
          <div className="pet-zzz">💤</div>
        )}
      </div>
      <div className="pet-bubble">
        {cpuUsage.toFixed(0)}%
      </div>
    </div>
  );
}
