import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { seedVec3Spring, stepScalarSpring, stepVec3Spring, TAIL_SPRING, type ScalarSpringState, type Vec3SpringState } from "./tailDynamics";

describe("tail spring dynamics", () => {
  it("critically damped scalar motion converges without overshooting", () => {
    const state: ScalarSpringState = { value: 0, velocity: 0, initialized: true };
    let maximum = state.value;
    for (let index = 0; index < 120; index += 1) {
      stepScalarSpring(state, 1, 1 / 60, 4, 1);
      maximum = Math.max(maximum, state.value);
    }
    expect(state.value).toBeCloseTo(1, 3);
    expect(maximum).toBeLessThanOrEqual(1 + 1e-6);
  });

  it("underdamped scalar motion overshoots gently and settles", () => {
    const state: ScalarSpringState = { value: 0, velocity: 0, initialized: true };
    let maximum = state.value;
    for (let index = 0; index < 120; index += 1) {
      stepScalarSpring(state, 1, 1 / 60, 3.2, 0.8);
      maximum = Math.max(maximum, state.value);
    }
    expect(maximum).toBeGreaterThan(1);
    expect(maximum).toBeLessThan(1.15);
    expect(state.value).toBeCloseTo(1, 2);
  });

  it("snaps an uninitialized vec3 spring to its target, then converges on every component", () => {
    const state: Vec3SpringState = {
      position: new THREE.Vector3(),
      velocity: new THREE.Vector3(1, 2, 3),
      initialized: false,
    };
    const initialTarget = new THREE.Vector3(2, -3, 4);
    stepVec3Spring(state, initialTarget, 1 / 60, 4, 1);
    expect(state.position.equals(initialTarget)).toBe(true);
    expect(state.velocity.length()).toBe(0);

    const finalTarget = new THREE.Vector3(-1, 5, 0.5);
    for (let index = 0; index < 120; index += 1) stepVec3Spring(state, finalTarget, 1 / 60, 4, 1);
    expect(state.position.x).toBeCloseTo(finalTarget.x, 3);
    expect(state.position.y).toBeCloseTo(finalTarget.y, 3);
    expect(state.position.z).toBeCloseTo(finalTarget.z, 3);
  });

  it("stays stable at the maximum frame delta", () => {
    const state: ScalarSpringState = { value: 0, velocity: 0, initialized: true };
    for (let index = 0; index < 100; index += 1) {
      stepScalarSpring(state, 1, 0.05, 6, 1);
      expect(Math.abs(state.value)).toBeLessThan(10);
    }
    expect(state.value).toBeCloseTo(1, 3);
  });

  it("seeds vec3 position and clears velocity", () => {
    const state: Vec3SpringState = {
      position: new THREE.Vector3(),
      velocity: new THREE.Vector3(3, -2, 1),
      initialized: false,
    };
    const position = new THREE.Vector3(1, 2, 3);
    seedVec3Spring(state, position);
    expect(state.position.equals(position)).toBe(true);
    expect(state.velocity.length()).toBe(0);
    expect(state.initialized).toBe(true);
  });

  it("uses a calmer target spring for sustained pointing", () => {
    expect(TAIL_SPRING.target.sustain).toBeDefined();
    expect(TAIL_SPRING.target.sustain.frequency).toBeLessThan(TAIL_SPRING.target.underline.frequency);
  });
});
