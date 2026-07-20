import * as THREE from "three";

export interface ScalarSpringState {
  value: number;
  velocity: number;
  initialized: boolean;
}

export interface Vec3SpringState {
  position: THREE.Vector3;
  velocity: THREE.Vector3;
  initialized: boolean;
}

export interface TailDynamicsState {
  target: Vec3SpringState;
  lastRawTarget: THREE.Vector3;
  hasRawTarget: boolean;
  strength: ScalarSpringState;
  lastSequenceId: number;
}

export const TAIL_SPRING = {
  target: {
    point: { frequency: 3.2, damping: 0.8 },
    underline: { frequency: 6.0, damping: 1.0 },
    retract: { frequency: 2.5, damping: 1.0 },
    sustain: { frequency: 2.0, damping: 1.1 },
  },
  strength: { frequency: 4.0, damping: 1.0 },
} as const;

const MAX_SUBSTEP = 1 / 120;
const _acceleration = new THREE.Vector3();

export function createTailDynamicsState(): TailDynamicsState {
  return {
    target: {
      position: new THREE.Vector3(),
      velocity: new THREE.Vector3(),
      initialized: false,
    },
    lastRawTarget: new THREE.Vector3(),
    hasRawTarget: false,
    strength: { value: 0, velocity: 0, initialized: false },
    lastSequenceId: 0,
  };
}

export function resetTailDynamics(state: TailDynamicsState): void {
  state.target.velocity.set(0, 0, 0);
  state.target.initialized = false;
  state.strength.velocity = 0;
  state.strength.initialized = false;
  state.hasRawTarget = false;
}

export function stepScalarSpring(
  state: ScalarSpringState,
  target: number,
  dt: number,
  frequencyHz: number,
  dampingRatio: number,
): void {
  if (!state.initialized) {
    state.value = target;
    state.velocity = 0;
    state.initialized = true;
    return;
  }
  if (dt <= 0) return;
  const substeps = Math.ceil(dt / MAX_SUBSTEP);
  const h = dt / substeps;
  const omega = 2 * Math.PI * frequencyHz;
  for (let index = 0; index < substeps; index += 1) {
    const acceleration = omega * omega * (target - state.value) - 2 * dampingRatio * omega * state.velocity;
    state.velocity += acceleration * h;
    state.value += state.velocity * h;
  }
}

export function stepVec3Spring(
  state: Vec3SpringState,
  target: THREE.Vector3,
  dt: number,
  frequencyHz: number,
  dampingRatio: number,
): void {
  if (!state.initialized) {
    seedVec3Spring(state, target);
    return;
  }
  if (dt <= 0) return;
  const substeps = Math.ceil(dt / MAX_SUBSTEP);
  const h = dt / substeps;
  const omega = 2 * Math.PI * frequencyHz;
  const stiffness = omega * omega;
  const damping = 2 * dampingRatio * omega;
  for (let index = 0; index < substeps; index += 1) {
    _acceleration.copy(target).sub(state.position).multiplyScalar(stiffness).addScaledVector(state.velocity, -damping);
    state.velocity.addScaledVector(_acceleration, h);
    state.position.addScaledVector(state.velocity, h);
  }
}

export function seedVec3Spring(state: Vec3SpringState, position: THREE.Vector3): void {
  state.position.copy(position);
  state.velocity.set(0, 0, 0);
  state.initialized = true;
}
