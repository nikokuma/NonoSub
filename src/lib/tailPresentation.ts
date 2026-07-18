import * as THREE from "three";
import type { ChalkColor } from "./contracts";

export type TailPresentationPhase = "idle" | "point" | "hold" | "underline" | "retract";

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

export const SEMANTIC_TAIL_BONES = {
  left: Array.from({ length: 12 }, (_, index) => `tail.L.${String(index + 1).padStart(2, "0")}`),
  right: Array.from({ length: 12 }, (_, index) => `tail.R.${String(index + 1).padStart(2, "0")}`),
} as const;

export function cueScreenPoint(rect: RectLike, kind: "point" | "underline", progress = 0, rtl = false): ScreenPoint {
  if (kind === "point") return { x: rect.left - 7, y: rect.top + rect.height * 0.52 };
  const clamped = Math.max(0, Math.min(1, progress));
  const directionProgress = rtl ? 1 - clamped : clamped;
  return {
    x: rect.left + rect.width * directionProgress,
    y: rect.top + rect.height + 4,
  };
}

export function chooseNearestTail(leftTip: ScreenPoint, rightTip: ScreenPoint, target: ScreenPoint): "left" | "right" {
  const distance = (point: ScreenPoint) => Math.hypot(point.x - target.x, point.y - target.y);
  return distance(leftTip) <= distance(rightTip) ? "left" : "right";
}

export function presentationStrength(presentation: TailPresentation): number {
  const progress = Math.max(0, Math.min(1, presentation.progress));
  if (presentation.phase === "idle") return 0;
  if (presentation.phase === "point") return easeInOut(progress);
  if (presentation.phase === "retract") return 1 - easeInOut(progress);
  return 1;
}

export function solveCcdChain(chain: THREE.Bone[], target: THREE.Vector3, blend = 1, iterations = 4, protectedJoints = 3): number {
  if (chain.length < 3 || blend <= 0) return Number.POSITIVE_INFINITY;
  const effector = chain.at(-1)!;
  const jointPosition = new THREE.Vector3();
  const effectorPosition = new THREE.Vector3();
  const effectorDirection = new THREE.Vector3();
  const targetDirection = new THREE.Vector3();
  const worldQuaternion = new THREE.Quaternion();
  const parentQuaternion = new THREE.Quaternion();
  const delta = new THREE.Quaternion();
  const limitedDelta = new THREE.Quaternion();
  const desiredWorld = new THREE.Quaternion();
  const desiredLocal = new THREE.Quaternion();
  const identity = new THREE.Quaternion();

  chain[0].updateWorldMatrix(true, true);
  for (let iteration = 0; iteration < iterations; iteration += 1) {
    for (let index = chain.length - 2; index >= Math.max(1, protectedJoints); index -= 1) {
      const joint = chain[index];
      joint.getWorldPosition(jointPosition);
      effector.getWorldPosition(effectorPosition);
      effectorDirection.subVectors(effectorPosition, jointPosition);
      targetDirection.subVectors(target, jointPosition);
      if (effectorDirection.lengthSq() < 1e-8 || targetDirection.lengthSq() < 1e-8) continue;
      effectorDirection.normalize();
      targetDirection.normalize();
      delta.setFromUnitVectors(effectorDirection, targetDirection);

      const angle = 2 * Math.acos(Math.min(1, Math.abs(delta.w)));
      const chainProgress = index / (chain.length - 1);
      const maximumStep = THREE.MathUtils.lerp(0.055, 0.28, chainProgress);
      limitedDelta.copy(delta);
      if (angle > maximumStep) limitedDelta.copy(identity).slerp(delta, maximumStep / angle);

      joint.getWorldQuaternion(worldQuaternion);
      desiredWorld.copy(limitedDelta).multiply(worldQuaternion);
      if (joint.parent) joint.parent.getWorldQuaternion(parentQuaternion).invert();
      else parentQuaternion.identity();
      desiredLocal.copy(parentQuaternion).multiply(desiredWorld);
      joint.quaternion.slerp(desiredLocal, Math.max(0, Math.min(1, blend)));
      joint.updateWorldMatrix(false, true);
    }
  }
  effector.getWorldPosition(effectorPosition);
  return effectorPosition.distanceTo(target);
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

export function tailChainReach(positions: readonly THREE.Vector3[]): number {
  return positions.slice(1).reduce((total, position) => total + position.length(), 0);
}

export function requiredTailStretch(targetDistance: number, authoredReach: number, maximum = 1.3): number {
  if (!Number.isFinite(targetDistance) || !Number.isFinite(authoredReach) || authoredReach <= 0) return 1;
  return THREE.MathUtils.clamp(targetDistance / authoredReach, 1, maximum);
}

/**
 * Extends only distal child offsets. Moving joint origins instead of scaling the
 * bones keeps the cable cross-section and the three plug-side segments stable.
 */
export function applyTailExtension(
  chain: THREE.Bone[],
  restPositions: readonly THREE.Vector3[],
  stretch: number,
  protectedSegments = 3,
  maximum = 1.3,
): number {
  const count = Math.min(chain.length, restPositions.length);
  if (count < 2) return 0;
  const authoredReach = tailChainReach(restPositions.slice(0, count));
  const desiredReach = authoredReach * THREE.MathUtils.clamp(stretch, 1, maximum);
  const firstStretchable = Math.min(Math.max(protectedSegments + 1, 1), count - 1);
  let weightedLength = 0;
  for (let index = firstStretchable; index < count; index += 1) {
    const progress = (index - firstStretchable + 1) / (count - firstStretchable);
    weightedLength += restPositions[index].length() * smoothStep(progress);
  }
  const extraReach = Math.max(0, desiredReach - authoredReach);
  const extraScale = weightedLength > 1e-8 ? extraReach / weightedLength : 0;

  for (let index = 1; index < count; index += 1) {
    if (index < firstStretchable) {
      chain[index].position.copy(restPositions[index]);
      continue;
    }
    const progress = (index - firstStretchable + 1) / (count - firstStretchable);
    chain[index].position.copy(restPositions[index]).multiplyScalar(1 + extraScale * smoothStep(progress));
  }
  chain[0].updateWorldMatrix(true, true);
  return chain.slice(1, count).reduce((total, bone) => total + bone.position.length(), 0);
}

function easeInOut(value: number): number {
  return value < 0.5 ? 2 * value * value : 1 - Math.pow(-2 * value + 2, 2) / 2;
}

function smoothStep(value: number): number {
  const clamped = Math.max(0, Math.min(1, value));
  return clamped * clamped * (3 - 2 * clamped);
}
