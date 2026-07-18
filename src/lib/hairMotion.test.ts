import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { applyHairMotion, resolveHairMotionRig, stepHairSpring } from "./hairMotion";

describe("procedural hair follow-through", () => {
  it("converges toward a target without exceeding a stable range", () => {
    let state = { pitch: 0, pitchVelocity: 0, yaw: 0, yawVelocity: 0 };
    for (let index = 0; index < 240; index += 1) state = stepHairSpring(state, 0.04, -0.03, 1 / 120);
    expect(state.pitch).toBeCloseTo(0.04, 3);
    expect(state.yaw).toBeCloseTo(-0.03, 3);
    expect(Math.abs(state.pitchVelocity)).toBeLessThan(0.001);
  });

  it("resolves supported hair roots and restores them for reduced motion", () => {
    const model = new THREE.Group();
    const head = new THREE.Bone();
    head.name = "spine.006";
    const root = new THREE.Bone();
    root.name = "spine.021";
    const tip = new THREE.Bone();
    tip.name = "spine.022";
    tip.position.set(0, 0.5, 0);
    model.add(head, root);
    root.add(tip);
    const rig = resolveHairMotionRig(model);
    expect(rig?.chains).toHaveLength(1);
    applyHairMotion(rig!, 1 / 60, 1_000, false);
    const moved = root.quaternion.angleTo(new THREE.Quaternion());
    expect(moved).toBeGreaterThan(0);
    applyHairMotion(rig!, 1 / 60, 1_016, true);
    expect(root.quaternion.angleTo(new THREE.Quaternion())).toBeLessThan(1e-8);
    expect(tip.quaternion.angleTo(new THREE.Quaternion())).toBeLessThan(1e-8);
  });

  it("resolves Blender dotted bone names after GLTFLoader sanitization", () => {
    const model = new THREE.Group();
    const root = new THREE.Bone();
    root.name = THREE.PropertyBinding.sanitizeNodeName("spine.021");
    const tip = new THREE.Bone();
    tip.name = THREE.PropertyBinding.sanitizeNodeName("spine.022");
    model.add(root);
    root.add(tip);
    expect(resolveHairMotionRig(model)?.chains).toHaveLength(1);
  });
});
