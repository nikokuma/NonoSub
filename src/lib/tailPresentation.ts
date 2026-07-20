import * as THREE from "three";
import type { ChalkColor } from "./contracts";

export type TailPresentationPhase = "idle" | "point" | "hold" | "underline" | "retract" | "sustain";

export interface TailPresentation {
  sequenceId: number;
  phase: TailPresentationPhase;
  progress: number;
  pointCueId?: string;
  underlineCueId?: string;
  underlineColor?: ChalkColor;
}

export interface RectLike {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ScreenPoint {
  x: number;
  y: number;
}

export interface TailRestPose {
  positions: THREE.Vector3[];
  rotations: THREE.Quaternion[];
}

export const IDLE_TAIL_PRESENTATION: TailPresentation = {
  sequenceId: 0,
  phase: "idle",
  progress: 0,
};

export const CURRENT_TAIL_BONES = {
  left: Array.from({ length: 12 }, (_, index) => `spine.${String(55 + index).padStart(3, "0")}`),
  right: Array.from({ length: 12 }, (_, index) => `spine.${String(67 + index).padStart(3, "0")}`),
} as const;

export const CHALK_TAIL_SIDE = "right" as const;
export const POINT_TAIL_SIDE: "left" | "right" = CHALK_TAIL_SIDE === "right" ? "left" : "right";
export const UNDERLINE_TIP_DROP_PX = 2;

export const SEMANTIC_TAIL_BONES = {
  left: Array.from({ length: 12 }, (_, index) => `tail.L.${String(index + 1).padStart(2, "0")}`),
  right: Array.from({ length: 12 }, (_, index) => `tail.R.${String(index + 1).padStart(2, "0")}`),
} as const;

export function cueScreenPoint(
  rect: RectLike,
  kind: "point" | "underline",
  progress = 0,
  rtl = false,
  out: ScreenPoint = { x: 0, y: 0 },
): ScreenPoint {
  if (kind === "point") {
    out.x = rect.left - 7;
    out.y = rect.top + rect.height * 0.52;
    return out;
  }
  const clamped = Math.max(0, Math.min(1, progress));
  const directionProgress = rtl ? 1 - clamped : clamped;
  out.x = rect.left + rect.width * directionProgress;
  out.y = rect.top + rect.height + UNDERLINE_TIP_DROP_PX;
  return out;
}

export function tipUnderlineProgress(rect: RectLike, tipX: number, rtl: boolean, previous: number): number {
  if (rect.width <= 0) return clamp01(previous);
  const fraction = clamp01((tipX - rect.left) / rect.width);
  return Math.max(clamp01(previous), rtl ? 1 - fraction : fraction);
}

export function presentationStrength(presentation: TailPresentation): number {
  const progress = Math.max(0, Math.min(1, presentation.progress));
  if (presentation.phase === "idle") return 0;
  if (presentation.phase === "point") return easeInOut(progress);
  if (presentation.phase === "retract") return 1 - easeInOut(progress);
  if (presentation.phase === "sustain") return 1;
  return 1;
}

export function pointTailStrengthTarget(presentation: TailPresentation): number {
  if (presentation.phase === "idle") return 0;
  if (presentation.phase === "point") return easeInOut(clamp01(presentation.progress));
  return 1;
}

export function underlineTailStrengthTarget(presentation: TailPresentation): number {
  if (presentation.phase === "hold") return clamp01(presentation.progress);
  if (presentation.phase === "underline") return 1;
  if (presentation.phase === "retract") return 1 - easeInOut(clamp01(presentation.progress));
  return 0;
}

export function captureTailRestPose(chain: THREE.Bone[]): TailRestPose {
  return {
    positions: chain.map((bone) => bone.position.clone()),
    rotations: chain.map((bone) => bone.quaternion.clone()),
  };
}

export function restoreTailRestPose(chain: THREE.Bone[], pose: TailRestPose): void {
  const count = Math.min(chain.length, pose.positions.length, pose.rotations.length);
  for (let index = 0; index < count; index += 1) {
    chain[index].position.copy(pose.positions[index]);
    chain[index].quaternion.copy(pose.rotations[index]);
  }
  chain[0]?.updateWorldMatrix(true, true);
}

export function requiredTailStretch(targetDistance: number, authoredReach: number, maximum = 1.3): number {
  if (!Number.isFinite(targetDistance) || !Number.isFinite(authoredReach) || authoredReach <= 0) return 1;
  return THREE.MathUtils.clamp(targetDistance / authoredReach, 1, maximum);
}

function easeInOut(value: number): number {
  return value < 0.5 ? 2 * value * value : 1 - Math.pow(-2 * value + 2, 2) / 2;
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}
