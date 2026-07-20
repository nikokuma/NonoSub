import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
  CHALK_TAIL_SIDE,
  captureTailRestPose,
  cueScreenPoint,
  pointTailStrengthTarget,
  POINT_TAIL_SIDE,
  presentationStrength,
  requiredTailStretch,
  restoreTailRestPose,
  tipUnderlineProgress,
  UNDERLINE_TIP_DROP_PX,
  underlineTailStrengthTarget,
  type TailPresentation,
} from "./tailPresentation";

describe("tail presentation geometry", () => {
  it("targets text edges and draws underlines in writing direction", () => {
    const rect = { left: 100, top: 40, width: 200, height: 30 };
    expect(cueScreenPoint(rect, "point")).toEqual({ x: 93, y: 55.6 });
    expect(cueScreenPoint(rect, "underline", 0, false).x).toBe(100);
    expect(cueScreenPoint(rect, "underline", 1, false).x).toBe(300);
    expect(cueScreenPoint(rect, "underline", 0, true).x).toBe(300);
    expect(cueScreenPoint(rect, "underline", 1, true).x).toBe(100);
    expect(cueScreenPoint(rect, "underline", 0.5).y).toBe(rect.top + rect.height + UNDERLINE_TIP_DROP_PX);
  });

  it("keeps phase strength bounded and assigns distinct tail roles", () => {
    const presentation: TailPresentation = { sequenceId: 1, phase: "retract", progress: 0.5 };
    expect(presentationStrength(presentation)).toBeCloseTo(0.5);
    expect(presentationStrength({ ...presentation, phase: "sustain" })).toBe(1);
    expect(requiredTailStretch(20, 10)).toBe(1.3);
    expect(requiredTailStretch(5, 10)).toBe(1);
    expect(CHALK_TAIL_SIDE).not.toBe(POINT_TAIL_SIDE);
  });

  it("computes point-tail strength targets by phase", () => {
    const strength = (phase: TailPresentation["phase"], progress: number) => pointTailStrengthTarget({ sequenceId: 1, phase, progress });
    expect(strength("idle", 1)).toBe(0);
    expect(strength("point", 0)).toBe(0);
    expect(strength("point", 0.5)).toBe(0.5);
    expect(strength("point", 1)).toBe(1);
    for (const phase of ["hold", "underline", "retract", "sustain"] as const) expect(strength(phase, 0.25)).toBe(1);
  });

  it("computes underline-tail strength targets by phase", () => {
    const strength = (phase: TailPresentation["phase"], progress: number) => underlineTailStrengthTarget({ sequenceId: 1, phase, progress });
    expect(strength("hold", 0)).toBe(0);
    expect(strength("hold", 0.5)).toBe(0.5);
    expect(strength("hold", 1)).toBe(1);
    expect(strength("underline", 0.25)).toBe(1);
    expect(strength("retract", 0)).toBe(1);
    expect(strength("retract", 0.25)).toBeCloseTo(0.875);
    expect(strength("retract", 1)).toBe(0);
    for (const phase of ["idle", "point", "sustain"] as const) expect(strength(phase, 0.5)).toBe(0);
  });

  it("derives monotonic underline progress from the projected tip", () => {
    const rect = { left: 100, top: 40, width: 200, height: 30 };
    expect(tipUnderlineProgress(rect, 150, false, 0)).toBe(0.25);
    expect(tipUnderlineProgress(rect, 150, true, 0)).toBe(0.75);
    expect(tipUnderlineProgress(rect, 50, false, 0)).toBe(0);
    expect(tipUnderlineProgress(rect, 350, false, 0)).toBe(1);
    expect(tipUnderlineProgress(rect, 140, false, 0.8)).toBe(0.8);
    expect(tipUnderlineProgress({ ...rect, width: 0 }, 200, false, 0.6)).toBe(0.6);
  });

  it("restores tail positions and rotations without accumulating stretch", () => {
    const chain = Array.from({ length: 8 }, () => new THREE.Bone());
    for (let index = 1; index < chain.length; index += 1) {
      chain[index].position.set(0.5, 0, 0);
      chain[index - 1].add(chain[index]);
    }
    const pose = captureTailRestPose(chain);
    chain[5].position.multiplyScalar(1.3);
    chain[5].rotateZ(0.4);
    restoreTailRestPose(chain, pose);
    chain.forEach((bone, index) => {
      expect(bone.position.distanceTo(pose.positions[index])).toBeLessThan(1e-8);
      expect(bone.quaternion.angleTo(pose.rotations[index])).toBeLessThan(1e-8);
    });
  });
});
