import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
  captureTailRestPose,
  chooseNearestTail,
  cueScreenPoint,
  presentationStrength,
  requiredTailStretch,
  restoreTailRestPose,
  tipUnderlineProgress,
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
  });

  it("chooses the nearest resting tail and keeps phase strength bounded", () => {
    expect(chooseNearestTail({ x: 10, y: 20 }, { x: 90, y: 20 }, { x: 25, y: 30 })).toBe("left");
    const presentation: TailPresentation = { sequenceId: 1, phase: "retract", progress: 0.5 };
    expect(presentationStrength(presentation)).toBeCloseTo(0.5);
    expect(requiredTailStretch(20, 10)).toBe(1.3);
    expect(requiredTailStretch(5, 10)).toBe(1);
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
