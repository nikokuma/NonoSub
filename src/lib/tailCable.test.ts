import { describe, expect, it } from "vitest";
import * as THREE from "three";
import {
  blendGuidePolyline,
  buildCableCurve,
  curveLength,
  curvePointByArc,
  distributeStretch,
  fitChainToPolyline,
  TAIL_TUNING,
} from "./tailCable";
import { captureTailRestPose, restoreTailRestPose } from "./tailPresentation";

const ROOT = new THREE.Vector3();
const RIGHT = new THREE.Vector3(1, 0, 0);
const DOWN = new THREE.Vector3(0, -1, 0);
const FORWARD = new THREE.Vector3(0, 0, 1);

describe("tail cable curve", () => {
  it("maps straight-line arc distance linearly and builds an accurate curved LUT", () => {
    const straight = buildCableCurve({
      root: ROOT,
      baseTangent: RIGHT,
      target: new THREE.Vector3(10, 0, 0),
      tipTangent: RIGHT,
      chainLength: 10,
      bulgeDirection: FORWARD,
      sagDirection: new THREE.Vector3(),
    });
    for (let distance = 0; distance <= 10; distance += 0.25) {
      expect(curvePointByArc(straight, distance).distanceTo(new THREE.Vector3(distance, 0, 0))).toBeLessThan(1e-3);
    }

    const curved = buildCableCurve({
      root: ROOT,
      baseTangent: new THREE.Vector3(1, 1, 0).normalize(),
      target: new THREE.Vector3(4, 2, 1),
      tipTangent: new THREE.Vector3(1, -0.2, 0.4).normalize(),
      chainLength: 4,
      bulgeDirection: FORWARD,
      sagDirection: DOWN,
    });
    let previous = curvePointByArc(curved, 0);
    for (let index = 1; index <= TAIL_TUNING.lutSamples; index += 1) {
      const point = curvePointByArc(curved, curveLength(curved) * index / TAIL_TUNING.lutSamples);
      expect(point.distanceTo(previous)).toBeGreaterThan(0);
      previous = point;
    }
    const reference = referenceCurveLength(curved, 256);
    expect(Math.abs(curveLength(curved) - reference) / reference).toBeLessThan(0.005);
  });

  it("absorbs deep slack with a bulge and does not bulge an overreach target", () => {
    const slack = buildCableCurve({
      root: ROOT,
      baseTangent: RIGHT,
      target: new THREE.Vector3(0.5, 0, 0),
      tipTangent: RIGHT,
      chainLength: 1,
      bulgeDirection: new THREE.Vector3(0, 1, 0),
      sagDirection: new THREE.Vector3(),
    });
    expect(Math.abs(curveLength(slack) - 1)).toBeLessThan(0.02);
    expect(Math.abs(slack.p1.y) + Math.abs(slack.p2.y)).toBeGreaterThan(0);

    const overreach = buildCableCurve({
      root: ROOT,
      baseTangent: RIGHT,
      target: new THREE.Vector3(1.5, 0, 0),
      tipTangent: RIGHT,
      chainLength: 1,
      bulgeDirection: new THREE.Vector3(0, 1, 0),
      sagDirection: new THREE.Vector3(),
    });
    expect(overreach.p1.y).toBe(0);
    expect(overreach.p2.y).toBe(0);
    expect(curveLength(overreach)).toBeLessThanOrEqual(1.5 * 1.02);
  });

  it("handles degenerate and zero-valued input without NaNs", () => {
    const curve = buildCableCurve({
      root: ROOT,
      baseTangent: new THREE.Vector3(),
      target: ROOT,
      tipTangent: new THREE.Vector3(),
      chainLength: 0,
      bulgeDirection: new THREE.Vector3(),
      sagDirection: new THREE.Vector3(),
    });
    expect(curve.degenerate).toBe(true);
    expect(curveLength(curve)).toBe(0);
    const point = curvePointByArc(curve, 10);
    expect([point.x, point.y, point.z].every(Number.isFinite)).toBe(true);
  });
});

describe("tail cable shaping", () => {
  it("distributes stretch distally and preserves its exact requested total", () => {
    const rest = Array.from({ length: 11 }, (_, index) => 0.25 + index * 0.03);
    for (const stretch of [1.1, 1.3, 2]) {
      const result = distributeStretch(rest, stretch);
      const expected = Math.min(stretch, TAIL_TUNING.stretchMax) * rest.reduce((sum, value) => sum + value, 0);
      expect(result.reduce((sum, value) => sum + value, 0)).toBeCloseTo(expected, 9);
      for (let index = 0; index < TAIL_TUNING.stretchStartSegment; index += 1) {
        expect(result[index]).toBe(rest[index]);
      }
      let previousScale = 1;
      for (let index = TAIL_TUNING.stretchStartSegment; index < result.length; index += 1) {
        const scale = result[index] / rest[index];
        expect(scale).toBeGreaterThanOrEqual(previousScale - 1e-12);
        previousScale = scale;
      }
    }
    expect(distributeStretch(rest, 1)).toEqual(rest);
  });

  it("pins protected joints and smoothly increases guide influence", () => {
    const rest = Array.from({ length: 12 }, (_, index) => new THREE.Vector3(index, 0, 0));
    const curve = Array.from({ length: 12 }, (_, index) => new THREE.Vector3(index, index, 0));
    const out = rest.map(() => new THREE.Vector3());
    blendGuidePolyline(rest, curve, 0, out);
    out.forEach((point, index) => expect(point.equals(rest[index])).toBe(true));
    blendGuidePolyline(rest, curve, 1, out);
    for (let index = 0; index < TAIL_TUNING.protectedJoints; index += 1) {
      expect(out[index].equals(rest[index])).toBe(true);
    }
    for (let index = TAIL_TUNING.rampEnd; index < out.length; index += 1) {
      expect(out[index].equals(curve[index])).toBe(true);
    }
    let previousInfluence = 0;
    for (let index = TAIL_TUNING.protectedJoints; index <= TAIL_TUNING.rampEnd; index += 1) {
      const influence = out[index].y / curve[index].y;
      expect(influence).toBeGreaterThanOrEqual(previousInfluence);
      previousInfluence = influence;
    }
  });
});

describe("tail cable bone fitting", () => {
  it("fits and refits a smooth length-consistent polyline without changing segment lengths", () => {
    const chain = makeChain();
    const pose = captureTailRestPose(chain);
    const lengths = Array.from({ length: chain.length - 1 }, (_, index) => 0.42 + index * 0.01);
    const guide = makeGuide(lengths, 0.38);
    fitChainToPolyline(chain, pose.positions, guide, lengths);
    expectChainAtGuide(chain, guide, lengths, 1e-6);

    restoreTailRestPose(chain, pose);
    const restGuide = worldJoints(chain);
    const restLengths = segmentLengths(restGuide);
    fitChainToPolyline(chain, pose.positions, restGuide, restLengths);
    chain.forEach((bone, index) => {
      expect(bone.position.distanceTo(pose.positions[index])).toBeLessThan(1e-6);
      expect(bone.quaternion.angleTo(pose.rotations[index])).toBeLessThan(1e-6);
    });
  });

  it("does not drift across repeated restored fits", () => {
    const chain = makeChain();
    const pose = captureTailRestPose(chain);
    const lengths = Array.from({ length: chain.length - 1 }, () => 0.4);
    for (let cycle = 0; cycle < 200; cycle += 1) {
      restoreTailRestPose(chain, pose);
      fitChainToPolyline(chain, pose.positions, makeGuide(lengths, Math.sin(cycle * 0.07) * 0.5), lengths);
    }
    restoreTailRestPose(chain, pose);
    chain.forEach((bone, index) => {
      expect(bone.position.distanceTo(pose.positions[index])).toBeLessThan(1e-8);
      expect(bone.quaternion.angleTo(pose.rotations[index])).toBeLessThan(1e-8);
    });
  });

  it("uses a finite stable rotation for antiparallel directions", () => {
    const chain = makeChain(3, 1);
    const pose = captureTailRestPose(chain);
    const guide = [new THREE.Vector3(), new THREE.Vector3(-1, 0, 0), new THREE.Vector3(-2, 0, 0)];
    fitChainToPolyline(chain, pose.positions, guide, [1, 1]);
    chain.forEach((bone) => {
      expect([bone.quaternion.x, bone.quaternion.y, bone.quaternion.z, bone.quaternion.w].every(Number.isFinite)).toBe(true);
    });
    expect(worldJoints(chain)[1].distanceTo(guide[1])).toBeLessThan(1e-6);
  });

  it("keeps per-bone rotations continuous while the target sweeps an arc", () => {
    const chain = makeChain();
    const pose = captureTailRestPose(chain);
    const lengths = Array.from({ length: chain.length - 1 }, () => 0.4);
    let previous: THREE.Quaternion[] | undefined;
    for (let step = 0; step < 100; step += 1) {
      restoreTailRestPose(chain, pose);
      fitChainToPolyline(chain, pose.positions, makeGuide(lengths, -0.7 + 1.4 * step / 99), lengths);
      if (previous) {
        chain.forEach((bone, index) => expect(bone.quaternion.angleTo(previous![index])).toBeLessThan(0.35));
      }
      previous = chain.map((bone) => bone.quaternion.clone());
    }
  });
});

function makeChain(count = 12, localLength = 0.4): THREE.Bone[] {
  const chain = Array.from({ length: count }, () => new THREE.Bone());
  for (let index = 1; index < chain.length; index += 1) {
    chain[index].position.set(localLength, 0, 0);
    chain[index - 1].add(chain[index]);
  }
  chain[0].updateWorldMatrix(true, true);
  return chain;
}

function makeGuide(lengths: readonly number[], bend: number): THREE.Vector3[] {
  const result = [new THREE.Vector3()];
  for (let index = 0; index < lengths.length; index += 1) {
    const progress = (index + 1) / lengths.length;
    const angle = bend * progress;
    result.push(result[index].clone().add(new THREE.Vector3(Math.cos(angle), Math.sin(angle), 0).multiplyScalar(lengths[index])));
  }
  return result;
}

function worldJoints(chain: THREE.Bone[]): THREE.Vector3[] {
  chain[0].updateWorldMatrix(true, true);
  return chain.map((bone) => bone.getWorldPosition(new THREE.Vector3()));
}

function segmentLengths(joints: readonly THREE.Vector3[]): number[] {
  return joints.slice(1).map((joint, index) => joint.distanceTo(joints[index]));
}

function expectChainAtGuide(chain: THREE.Bone[], guide: readonly THREE.Vector3[], lengths: readonly number[], tolerance: number): void {
  const joints = worldJoints(chain);
  joints.forEach((joint, index) => expect(joint.distanceTo(guide[index])).toBeLessThan(tolerance));
  segmentLengths(joints).forEach((length, index) => expect(Math.abs(length - lengths[index])).toBeLessThan(tolerance));
}

function referenceCurveLength(curve: ReturnType<typeof buildCableCurve>, samples: number): number {
  const bezier = new THREE.CubicBezierCurve3(curve.p0, curve.p1, curve.p2, curve.p3);
  let previous = bezier.getPoint(0);
  let total = 0;
  for (let index = 1; index <= samples; index += 1) {
    const point = bezier.getPoint(index / samples);
    total += point.distanceTo(previous);
    previous = point;
  }
  return total;
}
