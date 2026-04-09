export interface SystemInfo {
  cpu_usage: number;
  memory_used: number;
  memory_total: number;
  memory_percent: number;
}

export interface PetState {
  status: "idle" | "working" | "thinking";
}
