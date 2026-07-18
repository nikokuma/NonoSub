import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
  applyTailExtension,
  captureTailRestPose,
  chooseNearestTail,
  cueScreenPoint,
  presentationStrength,
  requiredTailStretch,
  restoreTailRestPose,
  solveCcdChain,
  tailChainReach,
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
  });

  it("moves a synthetic bone chain closer to a reachable target", () => {
    const chain = Array.from({ length: 6 }, () => new THREE.Bone());
    for (let index = 1; index < chain.length; index += 1) {
      chain[index].position.set(1, 0, 0);
      chain[index - 1].add(chain[index]);
    }
    chain[0].updateWorldMatrix(true, true);
    const target = new THREE.Vector3(3.5, 2.5, 0);
    const before = new THREE.Vector3();
    chain.at(-1)!.getWorldPosition(before);
    const beforeDistance = before.distanceTo(target);
    const afterDistance = solveCcdChain(chain, target, 0.85, 4);
    expect(Number.isFinite(afterDistance)).toBe(true);
    expect(afterDistance).toBeLessThan(beforeDistance);
    expect(chain[0].quaternion.equals(new THREE.Quaternion())).toBe(true);
    expect(chain[1].quaternion.equals(new THREE.Quaternion())).toBe(true);
    expect(chain[2].quaternion.equals(new THREE.Quaternion())).toBe(true);
  });

  it("extends only distal tail segments and caps total reach at 130%", () => {
    const chain = Array.from({ length: 12 }, () => new THREE.Bone());
    for (let index = 1; index < chain.length; index += 1) {
      chain[index].position.set(1, 0, 0);
      chain[index - 1].add(chain[index]);
    }
    const pose = captureTailRestPose(chain);
    const authoredReach = tailChainReach(pose.positions);
    expect(requiredTailStretch(authoredReach * 2, authoredReach)).toBe(1.3);
    const extendedReach = applyTailExtension(chain, pose.positions, 1.3);
    expect(extendedReach).toBeCloseTo(authoredReach * 1.3, 5);
    expect(chain[1].position.equals(pose.positions[1])).toBe(true);
    expect(chain[2].position.equals(pose.positions[2])).toBe(true);
    expect(chain[3].position.equals(pose.positions[3])).toBe(true);
    expect(chain.at(-1)!.position.length()).toBeGreaterThan(1);
  });

  it("restores tail positions and rotations without accumulating stretch", () => {
    const chain = Array.from({ length: 8 }, () => new THREE.Bone());
    for (let index = 1; index < chain.length; index += 1) {
      chain[index].position.set(0.5, 0, 0);
      chain[index - 1].add(chain[index]);
    }
    const pose = captureTailRestPose(chain);
    applyTailExtension(chain, pose.positions, 1.3);
    chain[5].rotateZ(0.4);
    restoreTailRestPose(chain, pose);
    chain.forEach((bone, index) => {
      expect(bone.position.distanceTo(pose.positions[index])).toBeLessThan(1e-8);
      expect(bone.quaternion.angleTo(pose.rotations[index])).toBeLessThan(1e-8);
    });
  });
});
