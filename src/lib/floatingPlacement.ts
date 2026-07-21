import type { LessonPlacement } from "./contracts";

export interface MonitorGeometry {
  key: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FloatingWindowGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function fitLogicalWindowSize(
  desired: { width: number; height: number },
  monitorPhysical: { width: number; height: number },
  scaleFactor: number,
  maximumRatio = 0.9,
): { width: number; height: number } {
  const scale = Number.isFinite(scaleFactor) && scaleFactor > 0 ? scaleFactor : 1;
  return {
    width: Math.min(desired.width, Math.floor((monitorPhysical.width / scale) * maximumRatio)),
    height: Math.min(desired.height, Math.floor((monitorPhysical.height / scale) * maximumRatio)),
  };
}

export function fitLogicalWindowSizeProportionally(
  desired: { width: number; height: number },
  monitorPhysical: { width: number; height: number },
  scaleFactor: number,
  maximumRatio = 0.9,
): { width: number; height: number } {
  const scale = Number.isFinite(scaleFactor) && scaleFactor > 0 ? scaleFactor : 1;
  const maximumWidth = Math.floor((monitorPhysical.width / scale) * maximumRatio);
  const maximumHeight = Math.floor((monitorPhysical.height / scale) * maximumRatio);
  const fit = Math.min(1, maximumWidth / desired.width, maximumHeight / desired.height);
  return {
    width: Math.max(1, Math.floor(desired.width * fit)),
    height: Math.max(1, Math.floor(desired.height * fit)),
  };
}

export function makeMonitorKey(name: string | null, monitor: Omit<MonitorGeometry, "key">): string {
  return `${name ?? "display"}:${monitor.x},${monitor.y}:${monitor.width}x${monitor.height}`;
}

export function normalizeLessonPlacement(
  monitor: MonitorGeometry,
  window: FloatingWindowGeometry,
): LessonPlacement {
  return {
    monitorKey: monitor.key,
    x: clamp((window.x + window.width / 2 - monitor.x) / monitor.width, 0, 1),
    y: clamp((window.y + window.height / 2 - monitor.y) / monitor.height, 0, 1),
  };
}

export function resolveLessonPosition(
  monitor: MonitorGeometry,
  width: number,
  height: number,
  placement?: LessonPlacement,
): { x: number; y: number } {
  const margin = 18;
  const centerX = monitor.x + (placement?.x ?? 0.72) * monitor.width;
  const centerY = monitor.y + (placement?.y ?? 0.34) * monitor.height;
  return {
    x: Math.round(clamp(centerX - width / 2, monitor.x + margin, monitor.x + monitor.width - width - margin)),
    y: Math.round(clamp(centerY - height / 2, monitor.y + margin, monitor.y + monitor.height - height - margin)),
  };
}

export function shouldPersistLessonPlacement(
  mode: "compose" | "thinking" | "lesson" | "error",
  suppressedUntil: number,
  now: number,
): boolean {
  return mode === "lesson" && now >= suppressedUntil;
}

function clamp(value: number, minimum: number, maximum: number): number {
  if (maximum < minimum) return minimum;
  return Math.min(maximum, Math.max(minimum, value));
}
